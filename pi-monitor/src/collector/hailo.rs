/// Polls the Hailo-10H dashboard server (`GET /api/stats` on port 8765).
///
/// The server runs exclusively on the "control" inventory node.
/// Extracts `hailo`, `hailo_perf`, and `sentinel` keys; ignores everything
/// that is already covered by the system/raspi-dash collectors.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use tokio::time::interval;
use tracing::debug;

use crate::app::{AppState, HailoDevice, HailoPerf, HailoState};
use crate::inventory::InventoryNode;

const HAILO_SERVER_PORT: u16 = 8765;
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);

/// Minimal deserialisation of the `cpu` key — only throttle fields.
#[derive(Deserialize, Default)]
struct ApiCpu {
    throttle_ok: Option<bool>,
    #[serde(default)]
    throttle_flags: Vec<String>,
}

/// Minimal deserialisation — only the keys we care about.
#[derive(Deserialize)]
struct ApiStats {
    #[serde(default)]
    hailo: HailoDevice,
    #[serde(default)]
    hailo_perf: HailoPerf,
    #[serde(default)]
    cpu: ApiCpu,
    /// Sentinel returns an arbitrary object; we only need `fps`.
    #[serde(default)]
    sentinel: Value,
}

pub async fn run(state: Arc<RwLock<AppState>>, nodes: Vec<InventoryNode>) {
    // Find the control node — it's the only one that runs the Hailo server.
    // Fall back to the first node if none is named "control".
    let host = match nodes.iter().find(|n| n.name == "control") {
        Some(n) => n.ansible_host.clone(),
        None => match nodes.first() {
            Some(n) => n.ansible_host.clone(),
            None => return, // no inventory → nothing to do
        },
    };

    let url = format!("http://{}:{}/api/stats", host, HAILO_SERVER_PORT);

    let client = Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .expect("failed to build HTTP client");

    let mut ticker = interval(POLL_INTERVAL);
    loop {
        ticker.tick().await;

        match fetch(&client, &url).await {
            Ok(mut new_state) => {
                // Carry forward the temperature history and push the new sample.
                let mut history = state.read().unwrap().hailo.temp_history.clone();
                history.push(new_state.perf.on_die_temperature);
                if history.len() > 60 {
                    history.remove(0);
                }
                new_state.temp_history = history;
                state.write().unwrap().hailo = new_state;
            }
            Err(e) => {
                let mut s = state.write().unwrap();
                s.hailo.available = false;
                s.hailo.error = Some(e);
            }
        }

        debug!("hailo collector tick complete");
    }
}

async fn fetch(client: &Client, url: &str) -> Result<HailoState, String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let stats: ApiStats = resp.json().await.map_err(|e| e.to_string())?;

    let sentinel_fps = stats.sentinel
        .get("fps")
        .and_then(|v: &serde_json::Value| v.as_f64())
        .map(|f| f as f32);

    Ok(HailoState {
        available: true,
        device: stats.hailo,
        perf: stats.hailo_perf,
        sentinel_fps,
        throttle_ok: stats.cpu.throttle_ok,
        throttle_flags: stats.cpu.throttle_flags,
        temp_history: Vec::new(), // filled in by run() after fetch returns
        error: None,
    })
}
