/// Polls each inventory node's unified pi-agent `GET /api/stats` endpoint.
///
/// One HTTP call per node every 5 seconds provides all data:
///
///   • System metrics (CPU, memory, disk, temp, fan) — from the primary node
///     (first node in inventory, or the one named "control").
///   • Processes — from every node.
///   • Hailo identity + perf — from the first node that reports hailo.present.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use reqwest::Client;
use serde::Deserialize;
use tokio::time::interval;
use tracing::debug;

use crate::app::{
    ApiProcess, AppState, CpuCore, DiskInfo, HailoDevice, HailoPerf, HailoState, NodeProcesses,
    ProcessState, SystemMetrics,
};
use crate::inventory::InventoryNode;

const DEFAULT_AGENT_PORT: u16 = 8765;
const POLL_INTERVAL: Duration = Duration::from_secs(5);
const HTTP_TIMEOUT: Duration = Duration::from_secs(3);
const TOP_N: usize = 5;
const HISTORY_LEN: usize = 60;

// ── Deserialization structs matching pi-agent JSON schema ─────────────────────

#[derive(Deserialize, Clone)]
struct AgentStats {
    #[serde(default)]
    processes: Vec<ApiProcess>,
    #[serde(default)]
    hailo: HailoDevice,
    #[serde(default)]
    hailo_perf: HailoPerf,
    #[serde(default)]
    cpu: AgentCpu,
    #[serde(default)]
    system: AgentSystem,
    #[serde(default)]
    temperature: AgentTemperature,
    #[serde(default)]
    memory: AgentMemory,
    #[serde(default)]
    disk: Vec<AgentDisk>,
    #[serde(default)]
    fan: AgentFan,
}

#[derive(Deserialize, Clone, Default)]
struct AgentCpu {
    freq_mhz: f64,
    core_v: f64,
    throttle_ok: Option<bool>,
    #[serde(default)]
    throttle_flags: Vec<String>,
    load_1: f64,
    load_5: f64,
    load_15: f64,
    #[serde(default)]
    per_core_pct: Vec<f64>,
}

#[derive(Deserialize, Clone, Default)]
struct AgentSystem {
    hostname: String,
    model: String,
    os: String,
    kernel: String,
    uptime_s: f64,
    ntp_synced: bool,
    timezone: String,
}

#[derive(Deserialize, Clone, Default)]
struct AgentTemperature {
    cpu_c: f64,
    rp1_c: Option<f64>,
}

#[derive(Deserialize, Clone, Default)]
struct AgentMemory {
    total_kb: u64,
    used_kb: u64,
    #[allow(dead_code)]
    available_kb: u64,
    swap_total_kb: u64,
    swap_used_kb: u64,
}

#[derive(Deserialize, Clone, Default)]
struct AgentDisk {
    mount: String,
    total_kb: u64,
    used_kb: u64,
}

#[derive(Deserialize, Clone, Default)]
struct AgentFan {
    rpm: u32,
}

// ── Local history tracking (not shared) ───────────────────────────────────────

struct NodeCtx {
    /// CPU history per core (last HISTORY_LEN samples)
    cpu_core_history: Vec<Vec<f32>>,
    /// CPU temperature history (last HISTORY_LEN samples)
    temp_history: Vec<f32>,
}

impl NodeCtx {
    fn new() -> Self {
        Self { cpu_core_history: Vec::new(), temp_history: Vec::new() }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run(state: Arc<RwLock<AppState>>, nodes: Vec<InventoryNode>, agent_port: u16) {
    if nodes.is_empty() {
        return;
    }

    // Initialise process slots immediately so the UI sees node names
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

    // Primary node: source of system metrics — "control" or the first node
    let primary_idx = nodes.iter().position(|n| n.name == "control").unwrap_or(0);
    let mut primary_ctx = NodeCtx::new();

    let mut ticker = interval(POLL_INTERVAL);
    loop {
        ticker.tick().await;

        // Probe every node concurrently
        let handles: Vec<_> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| {
                let client = client.clone();
                let url = format!("http://{}:{}/api/stats", node.ansible_host, agent_port);
                let node_name = node.name.clone();
                let host = node.ansible_host.clone();
                tokio::spawn(async move {
                    let result = fetch(&client, &url).await;
                    (i, node_name, host, result)
                })
            })
            .collect();

        let mut proc_updates: Vec<(usize, NodeProcesses)> = Vec::new();
        let mut hailo_update: Option<HailoState> = None;
        let mut primary_stats: Option<AgentStats> = None;

        for h in handles {
            let Ok((i, node_name, ansible_host, result)) = h.await else {
                continue;
            };

            match result {
                Ok(stats) => {
                    // Processes: every node
                    let procs = stats.processes.iter().cloned().take(TOP_N).collect();
                    proc_updates.push((
                        i,
                        NodeProcesses { node_name, ansible_host, processes: procs, error: None },
                    ));

                    // Hailo: first node that reports present
                    if hailo_update.is_none() && stats.hailo.present {
                        hailo_update = Some(HailoState {
                            available: true,
                            device: stats.hailo.clone(),
                            perf: stats.hailo_perf.clone(),
                            sentinel_fps: None,
                            throttle_ok: stats.cpu.throttle_ok,
                            throttle_flags: stats.cpu.throttle_flags.clone(),
                            temp_history: Vec::new(),
                            error: None,
                        });
                    }

                    // System metrics: primary node only
                    if i == primary_idx {
                        primary_stats = Some(stats);
                    }
                }
                Err(e) => {
                    proc_updates.push((
                        i,
                        NodeProcesses {
                            node_name,
                            ansible_host,
                            processes: vec![],
                            error: Some(e),
                        },
                    ));
                }
            }
        }

        // Commit all updates under a single write lock
        {
            let mut s = state.write().unwrap();

            for (i, entry) in proc_updates {
                if let Some(slot) = s.processes.nodes.get_mut(i) {
                    *slot = entry;
                }
            }

            if let Some(stats) = primary_stats {
                s.system = map_system_metrics(&stats, &mut primary_ctx);
            }

            if let Some(mut new_hailo) = hailo_update {
                let mut history = s.hailo.temp_history.clone();
                history.push(new_hailo.perf.on_die_temperature);
                if history.len() > 60 {
                    history.remove(0);
                }
                new_hailo.temp_history = history;
                s.hailo = new_hailo;
            } else {
                s.hailo.available = false;
            }

            // Alert thresholds (now driven by primary node data)
            let m = &s.system;
            let mut alerts = 0u32;
            if m.cpu_avg_pct > 90.0 { alerts += 1; }
            if m.cpu_temp_c > 80.0 { alerts += 1; }
            if m.mem_total > 0 && (m.mem_used as f32 / m.mem_total as f32) > 0.95 { alerts += 1; }
            s.alert_count = alerts;
        }

        debug!("node-agent poll complete ({} nodes)", nodes.len());
    }
}

// ── Mapping: AgentStats → SystemMetrics ───────────────────────────────────────

fn map_system_metrics(stats: &AgentStats, ctx: &mut NodeCtx) -> SystemMetrics {
    // Per-core CPU history
    let n_cores = stats.cpu.per_core_pct.len();
    if ctx.cpu_core_history.len() != n_cores {
        ctx.cpu_core_history = vec![Vec::new(); n_cores];
    }
    let cpu_cores: Vec<CpuCore> = stats
        .cpu
        .per_core_pct
        .iter()
        .enumerate()
        .map(|(i, &pct)| {
            let pct_f32 = pct as f32;
            push_history(&mut ctx.cpu_core_history[i], pct_f32);
            CpuCore { usage_pct: pct_f32, history: ctx.cpu_core_history[i].clone() }
        })
        .collect();

    let cpu_avg_pct = if cpu_cores.is_empty() {
        0.0
    } else {
        cpu_cores.iter().map(|c| c.usage_pct).sum::<f32>() / cpu_cores.len() as f32
    };

    // Temperature history
    let cpu_temp = stats.temperature.cpu_c as f32;
    push_history(&mut ctx.temp_history, cpu_temp);

    // Memory: pi-agent uses KB, SystemMetrics uses bytes
    let mem_total = stats.memory.total_kb * 1024;
    let mem_used = stats.memory.used_kb * 1024;
    let mem_available = (stats.memory.total_kb.saturating_sub(stats.memory.used_kb)) * 1024;
    let swap_total = stats.memory.swap_total_kb * 1024;
    let swap_used = stats.memory.swap_used_kb * 1024;

    // Disks: pi-agent omits fstype; avail_kb derived from total - used
    let disks: Vec<DiskInfo> = stats
        .disk
        .iter()
        .map(|d| DiskInfo {
            mount: d.mount.clone(),
            fstype: String::new(),
            total_kb: d.total_kb,
            used_kb: d.used_kb,
            avail_kb: d.total_kb.saturating_sub(d.used_kb),
        })
        .collect();

    SystemMetrics {
        hostname: stats.system.hostname.clone(),
        model: stats.system.model.clone(),
        os_name: stats.system.os.clone(),
        kernel: stats.system.kernel.clone(),
        uptime_secs: stats.system.uptime_s as u64,
        ntp_synced: stats.system.ntp_synced,
        timezone: stats.system.timezone.clone(),
        cpu_cores,
        cpu_avg_pct,
        cpu_freq_mhz: stats.cpu.freq_mhz as u32,
        cpu_voltage: stats.cpu.core_v as f32,
        load_avg: [
            stats.cpu.load_1 as f32,
            stats.cpu.load_5 as f32,
            stats.cpu.load_15 as f32,
        ],
        throttle_flags: stats.cpu.throttle_flags.clone(),
        cpu_temp_c: cpu_temp,
        rp1_temp_c: stats.temperature.rp1_c.unwrap_or(0.0) as f32,
        temp_history: ctx.temp_history.clone(),
        fan_rpm: stats.fan.rpm,
        mem_total,
        mem_used,
        mem_available,
        mem_bufcache: 0, // not provided by pi-agent
        swap_total,
        swap_used,
        gpu_v3d_mhz: 0,  // not provided by pi-agent
        gpu_hevc_mhz: 0, // not provided by pi-agent
        gpu_mem_mb: 0,   // not provided by pi-agent
        disks,
        disk_read_kbps: 0.0, // not provided by pi-agent
        disk_write_kbps: 0.0,
    }
}

fn push_history(history: &mut Vec<f32>, value: f32) {
    history.push(value);
    if history.len() > HISTORY_LEN {
        history.remove(0);
    }
}

// ── HTTP fetch ────────────────────────────────────────────────────────────────

async fn fetch(client: &Client, url: &str) -> Result<AgentStats, String> {
    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }
    resp.json::<AgentStats>().await.map_err(|e| e.to_string())
}
