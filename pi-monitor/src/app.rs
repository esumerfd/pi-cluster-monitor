use std::sync::{Arc, RwLock};
use anyhow::Result;
use crate::inventory::InventoryNode;

/// Which tab is currently active (1-indexed to match keyboard shortcuts)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview = 0,
    System = 1,
    Network = 2,
    Processes = 3,
    Services = 4,
    Hardware = 5,
    Logs = 6,
    /// Conditional: only visible when hailortcli is detected
    Npu = 7,
}

impl Tab {
    /// All tabs in display order. NPU is last so it can be conditionally appended.
    pub const ALL: &'static [Tab] = &[
        Tab::Overview,
        Tab::System,
        Tab::Network,
        Tab::Processes,
        Tab::Services,
        Tab::Hardware,
        Tab::Logs,
        Tab::Npu,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::System => "System",
            Tab::Network => "Network",
            Tab::Processes => "Processes",
            Tab::Services => "Services",
            Tab::Hardware => "Hardware",
            Tab::Logs => "Logs",
            Tab::Npu => "NPU",
        }
    }

    /// Returns the tabs visible in the current session.
    /// NPU is only included when `hailo_available` is true.
    pub fn visible(hailo_available: bool) -> &'static [Tab] {
        if hailo_available {
            Self::ALL
        } else {
            &Self::ALL[..7] // everything except Npu
        }
    }

    /// Map a digit key to a tab based on the currently visible set.
    pub fn from_key(c: char, hailo_available: bool) -> Option<Tab> {
        let idx = c.to_digit(10)? as usize;
        if idx == 0 {
            return None;
        }
        Self::visible(hailo_available).get(idx - 1).copied()
    }

}

/// Per-core CPU stats
#[derive(Debug, Clone, Default)]
pub struct CpuCore {
    pub usage_pct: f32,
    pub history: Vec<f32>, // last 60 samples
}

/// System-level metrics shared between collectors and the render loop
#[derive(Debug, Clone, Default)]
pub struct SystemMetrics {
    pub hostname: String,
    pub model: String,
    pub os_name: String,
    pub kernel: String,
    pub uptime_secs: u64,
    pub ntp_synced: bool,
    pub timezone: String,

    /// Per-core utilization
    pub cpu_cores: Vec<CpuCore>,
    /// Average CPU usage across all cores
    pub cpu_avg_pct: f32,
    /// CPU clock frequency in MHz
    pub cpu_freq_mhz: u32,
    /// Core voltage in volts
    pub cpu_voltage: f32,
    /// Load averages: 1min, 5min, 15min
    pub load_avg: [f32; 3],
    /// Throttle flags bitmask from vcgencmd (0 = OK)
    pub throttle_flags: u32,

    /// CPU temperature in °C
    pub cpu_temp_c: f32,
    /// RP1 ADC temperature in °C (0 if unavailable)
    pub rp1_temp_c: f32,
    /// CPU temperature history (last 60 samples)
    pub temp_history: Vec<f32>,

    /// Fan speed in RPM (0 if unavailable)
    pub fan_rpm: u32,

    /// RAM: total bytes
    pub mem_total: u64,
    /// RAM: used bytes
    pub mem_used: u64,
    /// RAM: available bytes
    pub mem_available: u64,
    /// RAM buffer/cache bytes
    pub mem_bufcache: u64,
    /// Swap: total bytes
    pub swap_total: u64,
    /// Swap: used bytes
    pub swap_used: u64,

    /// GPU V3D clock in MHz
    pub gpu_v3d_mhz: u32,
    /// GPU HEVC clock in MHz
    pub gpu_hevc_mhz: u32,
    /// GPU memory in MB
    pub gpu_mem_mb: u32,

    /// Disk mounts
    pub disks: Vec<DiskInfo>,
    /// Disk I/O rates
    pub disk_read_kbps: f32,
    pub disk_write_kbps: f32,

    /// Whether hailortcli is detected on this system
    pub hailo_available: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DiskInfo {
    pub mount: String,
    pub fstype: String,
    pub total_kb: u64,
    pub used_kb: u64,
    #[allow(dead_code)]
    pub avail_kb: u64,
}

impl DiskInfo {
    pub fn used_pct(&self) -> f32 {
        if self.total_kb == 0 {
            return 0.0;
        }
        self.used_kb as f32 / self.total_kb as f32 * 100.0
    }

    pub fn total_gb(&self) -> f32 {
        self.total_kb as f32 / (1024.0 * 1024.0)
    }

    pub fn used_gb(&self) -> f32 {
        self.used_kb as f32 / (1024.0 * 1024.0)
    }
}

/// One process entry from raspi-dash `/api/stats` → `processes[]`
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ApiProcess {
    pub pid: u32,
    pub user: String,
    pub cpu_pct: f32,
    pub mem_pct: f32,
    #[allow(dead_code)]
    pub rss_mb: f32,
    pub command: String,
}

/// Process list fetched from one node
#[derive(Debug, Clone)]
pub struct NodeProcesses {
    pub node_name: String,
    pub ansible_host: String,
    /// Top-5 processes, sorted by CPU% descending
    pub processes: Vec<ApiProcess>,
    /// Set when the last poll failed
    pub error: Option<String>,
}

/// Holds the process state for all inventory nodes
#[derive(Debug, Default, Clone)]
pub struct ProcessState {
    pub nodes: Vec<NodeProcesses>,
}

/// Reachability status for a cluster node
#[derive(Debug, Clone, PartialEq)]
pub enum ReachStatus {
    /// Not yet probed
    Unknown,
    /// Probe succeeded: resolved IP + round-trip latency in ms
    Up { ip: String, latency_ms: u32 },
    /// Probe failed (DNS or TCP connect timed out)
    Down,
}

impl Default for ReachStatus {
    fn default() -> Self {
        ReachStatus::Unknown
    }
}

/// Live status for one inventory node
#[derive(Debug, Clone)]
pub struct NodeStatus {
    pub node: InventoryNode,
    pub status: ReachStatus,
}

/// Network-level state: local identity + cluster node reachability
#[derive(Debug, Default)]
pub struct NetworkState {
    /// Local machine's primary non-loopback IPv4 address
    pub local_ip: String,
    /// Inventory path passed on the command line (empty = none)
    pub inventory_path: String,
    /// One entry per node from the inventory
    pub nodes: Vec<NodeStatus>,
}

/// Shared application state written by collectors, read by the render loop
#[derive(Debug, Default)]
pub struct AppState {
    pub system: SystemMetrics,
    pub network: NetworkState,
    pub processes: ProcessState,
    pub alert_count: u32,
}

/// Top-level application struct (owns event loop state)
pub struct App {
    pub active_tab: Tab,
    pub running: bool,
    pub show_help: bool,
    pub state: Arc<RwLock<AppState>>,
}

impl App {
    pub fn new() -> Self {
        Self {
            active_tab: Tab::Overview,
            running: true,
            show_help: false,
            state: Arc::new(RwLock::new(AppState::default())),
        }
    }

    pub fn next_tab(&mut self) {
        let hailo = self.state.read().unwrap().system.hailo_available;
        let visible = Tab::visible(hailo);
        let pos = visible.iter().position(|t| *t == self.active_tab).unwrap_or(0);
        self.active_tab = visible[(pos + 1) % visible.len()];
    }

    pub fn prev_tab(&mut self) {
        let hailo = self.state.read().unwrap().system.hailo_available;
        let visible = Tab::visible(hailo);
        let pos = visible.iter().position(|t| *t == self.active_tab).unwrap_or(0);
        self.active_tab = visible[if pos == 0 { visible.len() - 1 } else { pos - 1 }];
    }

    pub fn handle_key(&mut self, c: char) -> Result<()> {
        let hailo = self.state.read().unwrap().system.hailo_available;
        match c {
            'q' | 'Q' => self.running = false,
            '?' => self.show_help = !self.show_help,
            c if c.is_ascii_digit() => {
                if let Some(tab) = Tab::from_key(c, hailo) {
                    self.active_tab = tab;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Convenience: read the current system metrics snapshot
    pub fn system_snapshot(&self) -> SystemMetrics {
        self.state.read().unwrap().system.clone()
    }

    pub fn process_snapshot(&self) -> ProcessState {
        self.state.read().unwrap().processes.clone()
    }

    pub fn network_snapshot(&self) -> NetworkState {
        let s = self.state.read().unwrap();
        NetworkState {
            local_ip: s.network.local_ip.clone(),
            inventory_path: s.network.inventory_path.clone(),
            nodes: s.network.nodes.clone(),
        }
    }

    pub fn alert_count(&self) -> u32 {
        self.state.read().unwrap().alert_count
    }
}
