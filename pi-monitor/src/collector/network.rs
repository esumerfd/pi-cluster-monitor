/// Network collector: probes cluster nodes from the Ansible inventory.
///
/// For each node:
///   1. DNS-resolve `ansible_host` (handles .local via mDNS on Linux/macOS)
///   2. Attempt TCP connect to port 22 with a 2-second timeout
///   3. Record status (Up with IP + latency, or Down) in AppState.network

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
        let results: Vec<(usize, ReachStatus)> =
            futures_probe(&nodes).await;

        {
            let mut s = state.write().unwrap();
            for (idx, status) in results {
                if let Some(entry) = s.network.nodes.get_mut(idx) {
                    entry.status = status;
                }
            }
        }

        debug!("network probe cycle complete ({} nodes)", nodes.len());
    }
}

/// Probe all nodes concurrently, returning (index, ReachStatus) pairs.
async fn futures_probe(nodes: &[InventoryNode]) -> Vec<(usize, ReachStatus)> {
    let mut handles = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let host = node.ansible_host.clone();
        handles.push(tokio::spawn(async move {
            let status = probe_node(&host).await;
            (i, status)
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

/// Probe a single host: DNS resolve → TCP:22 connect.
async fn probe_node(host: &str) -> ReachStatus {
    let addr_str = format!("{}:22", host);
    let start = Instant::now();

    // Resolve hostname (handles .local via mDNS on the OS side)
    let addrs: Vec<SocketAddr> = match tokio::net::lookup_host(&addr_str).await {
        Ok(iter) => iter.collect(),
        Err(_) => return ReachStatus::Down,
    };

    let Some(addr) = addrs.first() else {
        return ReachStatus::Down;
    };

    let ip = addr.ip().to_string();

    // Try TCP connect to port 22
    match timeout(PROBE_TIMEOUT, TcpStream::connect(addr)).await {
        Ok(Ok(_)) => {
            let latency_ms = start.elapsed().as_millis() as u32;
            ReachStatus::Up { ip, latency_ms }
        }
        _ => ReachStatus::Down,
    }
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
