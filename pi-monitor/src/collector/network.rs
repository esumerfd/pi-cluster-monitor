/// Network collector: probes cluster nodes from the Ansible inventory.
///
/// For each node:
///   1. DNS-resolve `ansible_host` (handles .local via mDNS on Linux/macOS)
///   2. TCP connect port 22 (SSH) and the pi-agent port concurrently
///   3. Record status in AppState.network

use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use tokio::net::TcpStream;
use tokio::time::{interval, timeout};
use tracing::debug;

use crate::app::{AppState, NodeStatus, ReachStatus};
use crate::inventory::InventoryNode;

const PROBE_INTERVAL: Duration = Duration::from_secs(10);
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

pub async fn run(
    state: Arc<RwLock<AppState>>,
    nodes: Vec<InventoryNode>,
    inventory_path: String,
    agent_port: u16,
) {
    // Write inventory path and initial Unknown entries once at startup
    {
        let mut s = state.write().unwrap();
        s.network.inventory_path = inventory_path;
        s.network.local_ip = local_ip();
        s.network.nodes = nodes
            .iter()
            .map(|n| NodeStatus {
                node: n.clone(),
                status: ReachStatus::Unknown,
                agent_up: None,
            })
            .collect();
    }

    if nodes.is_empty() {
        // Nothing to probe; keep local_ip refreshed but don't spin
        let mut ticker = interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            let ip = local_ip();
            state.write().unwrap().network.local_ip = ip;
        }
    }

    let mut ticker = interval(PROBE_INTERVAL);
    loop {
        ticker.tick().await;

        // Refresh local IP (may change on DHCP)
        let ip = local_ip();
        state.write().unwrap().network.local_ip = ip;

        // Probe each node concurrently
        let results = futures_probe(&nodes, agent_port).await;

        {
            let mut s = state.write().unwrap();
            for (idx, status, agent_up) in results {
                if let Some(entry) = s.network.nodes.get_mut(idx) {
                    entry.status = status;
                    entry.agent_up = agent_up;
                }
            }
        }

        debug!("network probe cycle complete ({} nodes)", nodes.len());
    }
}

/// Probe all nodes concurrently, returning (index, ReachStatus, agent_up) tuples.
async fn futures_probe(
    nodes: &[InventoryNode],
    agent_port: u16,
) -> Vec<(usize, ReachStatus, Option<bool>)> {
    let mut handles = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let host = node.ansible_host.clone();
        handles.push(tokio::spawn(async move {
            let (status, agent_up) = probe_node(&host, agent_port).await;
            (i, status, agent_up)
        }));
    }
    let mut results = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(r) = h.await {
            results.push(r);
        }
    }
    results
}

/// Probe a single host: DNS resolve once, then TCP connect port 22 and agent_port concurrently.
async fn probe_node(host: &str, agent_port: u16) -> (ReachStatus, Option<bool>) {
    let start = Instant::now();

    // Resolve hostname once (handles .local via mDNS on the OS side)
    let addrs: Vec<SocketAddr> = match tokio::net::lookup_host(format!("{}:22", host)).await {
        Ok(iter) => iter.collect(),
        Err(_) => return (ReachStatus::Down, None),
    };

    let Some(&addr22) = addrs.first() else {
        return (ReachStatus::Down, None);
    };

    let ip = addr22.ip().to_string();
    let mut addr_agent = addr22;
    addr_agent.set_port(agent_port);

    // Probe both ports concurrently
    let (ssh_result, agent_result) = tokio::join!(
        timeout(PROBE_TIMEOUT, TcpStream::connect(addr22)),
        timeout(PROBE_TIMEOUT, TcpStream::connect(addr_agent)),
    );

    let latency_ms = start.elapsed().as_millis() as u32;

    let ssh_status = match ssh_result {
        Ok(Ok(_)) => ReachStatus::Up { ip, latency_ms },
        _ => ReachStatus::Down,
    };

    let agent_up = Some(matches!(agent_result, Ok(Ok(_))));

    (ssh_status, agent_up)
}

/// Get the primary non-loopback IPv4 address by connecting a UDP socket.
/// No packets are sent; the OS just fills in the local address via the routing table.
fn local_ip() -> String {
    UdpSocket::bind("0.0.0.0:0")
        .and_then(|s| {
            s.connect("8.8.8.8:80")?;
            s.local_addr()
        })
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "—".to_string())
}
