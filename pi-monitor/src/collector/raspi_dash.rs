/// Polls each inventory node's raspi-dash `/api/stats` endpoint and extracts
/// the top-5 processes sorted by CPU%.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use tokio::time::interval;
use tracing::debug;

use crate::app::{ApiProcess, AppState, NodeProcesses, ProcessState};
use crate::inventory::InventoryNode;

const RASPI_DASH_PORT: u16 = 8766;
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);
const TOP_N: usize = 5;

/// Minimal deserialisation: only pull `processes` from the full stats blob.
#[derive(Deserialize)]
struct ApiStats {
    #[serde(default)]
    processes: Vec<ApiProcess>,
}

pub async fn run(state: Arc<RwLock<AppState>>, nodes: Vec<InventoryNode>) {
    if nodes.is_empty() {
        return;
    }

    // Initialise slots so the UI sees the node names immediately
    {
        let mut s = state.write().unwrap();
        s.processes = ProcessState {
            nodes: nodes
                .iter()
                .map(|n| NodeProcesses {
                    node_name: n.name.clone(),
                    ansible_host: n.ansible_host.clone(),
                    processes: vec![],
                    error: Some("connecting…".into()),
                })
                .collect(),
        };
    }

    let client = Client::builder()
        .timeout(HTTP_TIMEOUT)
        .build()
        .expect("failed to build HTTP client");

    let mut ticker = interval(POLL_INTERVAL);
    loop {
        ticker.tick().await;

        // Probe each node concurrently
        let handles: Vec<_> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let client = client.clone();
                let url = format!(
                    "http://{}:{}/api/stats",
                    node.ansible_host, RASPI_DASH_PORT
                );
                let node_name = node.name.clone();
                let host = node.ansible_host.clone();
                tokio::spawn(async move {
                    let result = fetch_processes(&client, &url).await;
                    (i, node_name, host, result)
                })
            })
            .collect();

        let mut updates: Vec<(usize, NodeProcesses)> = Vec::new();
        for h in handles {
            if let Ok((i, node_name, ansible_host, result)) = h.await {
                let entry = match result {
                    Ok(procs) => NodeProcesses {
                        node_name,
                        ansible_host,
                        processes: procs,
                        error: None,
                    },
                    Err(e) => NodeProcesses {
                        node_name,
                        ansible_host,
                        processes: vec![],
                        error: Some(e),
                    },
                };
                updates.push((i, entry));
            }
        }

        {
            let mut s = state.write().unwrap();
            for (i, entry) in updates {
                if let Some(slot) = s.processes.nodes.get_mut(i) {
                    *slot = entry;
                }
            }
        }

        debug!("raspi-dash poll complete ({} nodes)", nodes.len());
    }
}

async fn fetch_processes(client: &Client, url: &str) -> Result<Vec<ApiProcess>, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let stats: ApiStats = resp.json().await.map_err(|e| e.to_string())?;

    // Server already sorts by -%cpu, take top N
    let procs = stats.processes.into_iter().take(TOP_N).collect();
    Ok(procs)
}
