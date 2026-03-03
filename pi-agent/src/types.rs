use serde::{Deserialize, Serialize};

/// Unified API response returned by GET /api/stats
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiStats {
    pub ts: f64,
    pub system: SystemInfo,
    pub cpu: CpuInfo,
    pub temperature: TemperatureInfo,
    pub memory: MemoryInfo,
    pub disk: Vec<DiskInfo>,
    pub fan: FanInfo,
    pub processes: Vec<ProcessInfo>,
    pub hailo: HailoInfo,
    pub hailo_perf: HailoPerfInfo,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub model: String,
    pub os: String,
    pub kernel: String,
    pub uptime_s: f64,
    pub cpu_count: u32,
    pub ntp_synced: bool,
    pub timezone: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuInfo {
    pub freq_mhz: f64,
    pub core_v: f64,
    pub throttle_ok: bool,
    pub throttle_flags: Vec<String>,
    pub load_1: f64,
    pub load_5: f64,
    pub load_15: f64,
    pub per_core_pct: Vec<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TemperatureInfo {
    pub cpu_c: f64,
    pub rp1_c: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_kb: u64,
    pub used_kb: u64,
    pub available_kb: u64,
    pub swap_total_kb: u64,
    pub swap_used_kb: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskInfo {
    pub mount: String,
    pub total_kb: u64,
    pub used_kb: u64,
    pub used_pct: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FanInfo {
    pub rpm: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub user: String,
    pub cpu_pct: f64,
    pub mem_pct: f64,
    pub rss_mb: f64,
    pub command: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HailoInfo {
    pub present: bool,
    pub firmware_ok: bool,
    pub ddr_total_gb: f64,
    pub pcie_current_link_speed: Option<String>,
    pub pcie_current_link_width: Option<String>,
    pub fw_version: Option<String>,
    pub architecture: Option<String>,
    pub nn_clock_mhz: Option<u32>,
    pub loaded_networks: u32,
    pub network_names: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HailoPerfInfo {
    pub nnc_utilization: f64,
    pub cpu_utilization: f64,
    pub dsp_utilization: f64,
    pub on_die_temperature: f64,
    pub on_die_voltage: i64,
    pub ram_size_total: u64,
    pub ram_size_used: u64,
}
