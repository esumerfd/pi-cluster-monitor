/// System metrics collector.
///
/// Reads CPU, memory, temperature, fan, disk from /proc, /sys, and vcgencmd.
/// Runs on Linux (Raspberry Pi); on other platforms returns stub data.

use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::interval;
use tracing::debug;

use crate::app::{AppState, CpuCore, DiskInfo};

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const HISTORY_LEN: usize = 60;

pub async fn run(state: Arc<RwLock<AppState>>) {
    let mut ticker = interval(POLL_INTERVAL);
    let mut prev_cpu: Option<CpuSnapshot> = None;
    let mut prev_diskstats: Option<DiskStatsSnapshot> = None;

    // Collect static fields once at startup
    let hostname = read_hostname();
    let model = read_model();
    let (os_name, kernel) = read_os_info();
    let hailo_available = check_hailo();

    // Initialise static fields
    {
        let mut s = state.write().unwrap();
        s.system.hostname = hostname;
        s.system.model = model;
        s.system.os_name = os_name;
        s.system.kernel = kernel;
        s.system.hailo_available = hailo_available;
    }

    loop {
        ticker.tick().await;

        let cpu_snap = read_cpu_stat();
        let (cores_pct, avg_pct) = if let Some(prev) = &prev_cpu {
            compute_cpu_usage(prev, &cpu_snap)
        } else {
            (vec![], 0.0)
        };
        prev_cpu = Some(cpu_snap);

        let disk_rates = if let Some(prev) = &prev_diskstats {
            let cur = read_diskstats();
            let rates = compute_disk_rates(prev, &cur, POLL_INTERVAL);
            prev_diskstats = Some(cur);
            rates
        } else {
            prev_diskstats = Some(read_diskstats());
            (0.0, 0.0)
        };

        let uptime_secs = read_uptime();
        let load_avg = read_loadavg();
        let cpu_temp = read_cpu_temp();
        let rp1_temp = read_rp1_temp();
        let fan_rpm = read_fan_rpm();
        let (mem_total, mem_used, mem_available, mem_bufcache, swap_total, swap_used) =
            read_meminfo();
        let disks = read_disk_usage();
        let cpu_freq = read_vcgencmd_clock("arm").unwrap_or(0);
        let cpu_volt = read_vcgencmd_volts().unwrap_or(0.0);
        let throttle = read_vcgencmd_throttled().unwrap_or(0);
        let gpu_v3d = read_vcgencmd_clock("v3d").unwrap_or(0);
        let gpu_hevc = read_vcgencmd_clock("hevc").unwrap_or(0);
        let gpu_mem = read_vcgencmd_gpu_mem().unwrap_or(0);

        {
            let mut s = state.write().unwrap();
            let m = &mut s.system;

            // Update per-core CPU
            if !cores_pct.is_empty() {
                if m.cpu_cores.len() != cores_pct.len() {
                    m.cpu_cores = vec![CpuCore::default(); cores_pct.len()];
                }
                for (i, pct) in cores_pct.iter().enumerate() {
                    m.cpu_cores[i].usage_pct = *pct;
                    push_history(&mut m.cpu_cores[i].history, *pct, HISTORY_LEN);
                }
                m.cpu_avg_pct = avg_pct;
            }

            // Temperature history
            m.cpu_temp_c = cpu_temp;
            push_history(&mut m.temp_history, cpu_temp, HISTORY_LEN);
            m.rp1_temp_c = rp1_temp;

            m.fan_rpm = fan_rpm;
            m.uptime_secs = uptime_secs;
            m.load_avg = load_avg;
            m.cpu_freq_mhz = cpu_freq;
            m.cpu_voltage = cpu_volt;
            m.throttle_flags = throttle;
            m.gpu_v3d_mhz = gpu_v3d;
            m.gpu_hevc_mhz = gpu_hevc;
            m.gpu_mem_mb = gpu_mem;

            m.mem_total = mem_total;
            m.mem_used = mem_used;
            m.mem_available = mem_available;
            m.mem_bufcache = mem_bufcache;
            m.swap_total = swap_total;
            m.swap_used = swap_used;

            m.disks = disks;
            m.disk_read_kbps = disk_rates.0;
            m.disk_write_kbps = disk_rates.1;

            // Threshold alerts
            let mut alerts = 0u32;
            if m.cpu_avg_pct > 90.0 {
                alerts += 1;
            }
            if m.cpu_temp_c > 80.0 {
                alerts += 1;
            }
            if m.mem_total > 0
                && (m.mem_used as f32 / m.mem_total as f32) > 0.95
            {
                alerts += 1;
            }
            s.alert_count = alerts;
        }

        debug!("system collector tick complete");
    }
}

// ── CPU ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct CpuSnapshot {
    /// One entry per core: (user, nice, system, idle, iowait, irq, softirq, steal)
    cores: Vec<[u64; 8]>,
}

fn read_cpu_stat() -> CpuSnapshot {
    #[cfg(target_os = "linux")]
    {
        let content = match std::fs::read_to_string("/proc/stat") {
            Ok(c) => c,
            Err(e) => {
                warn!("cannot read /proc/stat: {e}");
                return CpuSnapshot { cores: vec![] };
            }
        };
        let mut cores = Vec::new();
        for line in content.lines() {
            if line.starts_with("cpu") && !line.starts_with("cpu ") {
                let mut fields = line.split_whitespace();
                fields.next(); // skip "cpuN"
                let mut vals = [0u64; 8];
                for v in &mut vals {
                    *v = fields.next().and_then(|s| s.parse().ok()).unwrap_or(0);
                }
                cores.push(vals);
            }
        }
        CpuSnapshot { cores }
    }

    #[cfg(not(target_os = "linux"))]
    {
        // Stub: 4 cores at 0
        CpuSnapshot {
            cores: vec![[0u64; 8]; 4],
        }
    }
}

fn compute_cpu_usage(prev: &CpuSnapshot, cur: &CpuSnapshot) -> (Vec<f32>, f32) {
    if prev.cores.len() != cur.cores.len() || cur.cores.is_empty() {
        return (vec![], 0.0);
    }
    let mut pcts = Vec::with_capacity(cur.cores.len());
    for (p, c) in prev.cores.iter().zip(cur.cores.iter()) {
        let prev_idle = p[3] + p[4]; // idle + iowait
        let cur_idle = c[3] + c[4];
        let prev_total: u64 = p.iter().sum();
        let cur_total: u64 = c.iter().sum();
        let d_total = cur_total.saturating_sub(prev_total) as f32;
        let d_idle = cur_idle.saturating_sub(prev_idle) as f32;
        let pct = if d_total > 0.0 {
            (1.0 - d_idle / d_total) * 100.0
        } else {
            0.0
        };
        pcts.push(pct.clamp(0.0, 100.0));
    }
    let avg = pcts.iter().sum::<f32>() / pcts.len() as f32;
    (pcts, avg)
}

// ── Memory ──────────────────────────────────────────────────────────────────

fn read_meminfo() -> (u64, u64, u64, u64, u64, u64) {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let mut map: HashMap<&str, u64> = HashMap::new();
        for line in content.lines() {
            if let Some((key, val)) = line.split_once(':') {
                let kb: u64 = val.split_whitespace().next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                map.insert(key.trim(), kb * 1024);
            }
        }
        let total = map.get("MemTotal").copied().unwrap_or(0);
        let available = map.get("MemAvailable").copied().unwrap_or(0);
        let buffers = map.get("Buffers").copied().unwrap_or(0);
        let cached = map.get("Cached").copied().unwrap_or(0);
        let used = total.saturating_sub(available);
        let bufcache = buffers + cached;
        let swap_total = map.get("SwapTotal").copied().unwrap_or(0);
        let swap_free = map.get("SwapFree").copied().unwrap_or(0);
        let swap_used = swap_total.saturating_sub(swap_free);
        (total, used, available, bufcache, swap_total, swap_used)
    }
    #[cfg(not(target_os = "linux"))]
    {
        use sysinfo::System;
        let mut sys = System::new_all();
        sys.refresh_all();
        let total = sys.total_memory();
        let used = sys.used_memory();
        let available = total.saturating_sub(used);
        let swap_total = sys.total_swap();
        let swap_used = sys.used_swap();
        (total, used, available, 0, swap_total, swap_used)
    }
}

// ── Temperature ─────────────────────────────────────────────────────────────

fn read_cpu_temp() -> f32 {
    #[cfg(target_os = "linux")]
    {
        // Try vcgencmd first (Pi-specific)
        if let Ok(out) = std::process::Command::new("vcgencmd")
            .arg("measure_temp")
            .output()
        {
            let s = String::from_utf8_lossy(&out.stdout);
            // Output: "temp=46.1'C\n"
            if let Some(val) = s.trim().strip_prefix("temp=").and_then(|s| {
                s.trim_end_matches("'C").parse::<f32>().ok()
            }) {
                return val;
            }
        }
        // Fallback: /sys/class/hwmon
        hwmon_temp("cpu_thermal").unwrap_or(0.0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        // macOS stub
        42.0
    }
}

fn read_rp1_temp() -> f32 {
    #[cfg(target_os = "linux")]
    {
        hwmon_temp("rp1").unwrap_or(0.0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        0.0
    }
}

#[cfg(target_os = "linux")]
fn hwmon_temp(name_fragment: &str) -> Option<f32> {
    let hwmon_base = std::path::Path::new("/sys/class/hwmon");
    if let Ok(entries) = std::fs::read_dir(hwmon_base) {
        for entry in entries.flatten() {
            let name_path = entry.path().join("name");
            if let Ok(n) = std::fs::read_to_string(&name_path) {
                if n.trim().contains(name_fragment) {
                    let temp_path = entry.path().join("temp1_input");
                    if let Ok(v) = std::fs::read_to_string(&temp_path) {
                        if let Ok(millic) = v.trim().parse::<i32>() {
                            return Some(millic as f32 / 1000.0);
                        }
                    }
                }
            }
        }
    }
    None
}

// ── Fan ─────────────────────────────────────────────────────────────────────

fn read_fan_rpm() -> u32 {
    #[cfg(target_os = "linux")]
    {
        let hwmon_base = std::path::Path::new("/sys/class/hwmon");
        if let Ok(entries) = std::fs::read_dir(hwmon_base) {
            for entry in entries.flatten() {
                let fan_path = entry.path().join("fan1_input");
                if fan_path.exists() {
                    if let Ok(v) = std::fs::read_to_string(&fan_path) {
                        if let Ok(rpm) = v.trim().parse::<u32>() {
                            return rpm;
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

// ── Disk usage (df) ──────────────────────────────────────────────────────────

fn read_disk_usage() -> Vec<DiskInfo> {
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("df")
            .args(["-T", "-k", "--exclude-type=tmpfs", "--exclude-type=devtmpfs"])
            .output();
        match out {
            Ok(o) => {
                let text = String::from_utf8_lossy(&o.stdout);
                parse_df_output(&text)
            }
            Err(e) => {
                warn!("df failed: {e}");
                vec![]
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        vec![DiskInfo {
            mount: "/".to_string(),
            fstype: "apfs".to_string(),
            total_kb: 500 * 1024 * 1024,
            used_kb: 200 * 1024 * 1024,
            avail_kb: 300 * 1024 * 1024,
        }]
    }
}

#[cfg(target_os = "linux")]
fn parse_df_output(text: &str) -> Vec<DiskInfo> {
    let mut result = Vec::new();
    for line in text.lines().skip(1) {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // df -T -k columns: Filesystem Type 1K-blocks Used Available Use% Mounted
        if cols.len() < 7 {
            continue;
        }
        let total_kb: u64 = cols[2].parse().unwrap_or(0);
        let used_kb: u64 = cols[3].parse().unwrap_or(0);
        let avail_kb: u64 = cols[4].parse().unwrap_or(0);
        result.push(DiskInfo {
            mount: cols[6].to_string(),
            fstype: cols[1].to_string(),
            total_kb,
            used_kb,
            avail_kb,
        });
    }
    result
}

// ── Disk I/O (/proc/diskstats) ───────────────────────────────────────────────

struct DiskStatsSnapshot {
    reads_sectors: u64,
    writes_sectors: u64,
    ts: std::time::Instant,
}

fn read_diskstats() -> DiskStatsSnapshot {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/diskstats").unwrap_or_default();
        let (mut r, mut w) = (0u64, 0u64);
        for line in content.lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 14 {
                continue;
            }
            let dev = cols[2];
            // Only track physical disks (sd*, mmcblk*, nvme*)
            if dev.starts_with("sd")
                || dev.starts_with("mmcblk")
                || dev.starts_with("nvme")
            {
                r += cols[5].parse::<u64>().unwrap_or(0);
                w += cols[9].parse::<u64>().unwrap_or(0);
            }
        }
        DiskStatsSnapshot {
            reads_sectors: r,
            writes_sectors: w,
            ts: std::time::Instant::now(),
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        DiskStatsSnapshot {
            reads_sectors: 0,
            writes_sectors: 0,
            ts: std::time::Instant::now(),
        }
    }
}

fn compute_disk_rates(
    prev: &DiskStatsSnapshot,
    cur: &DiskStatsSnapshot,
    _interval: Duration,
) -> (f32, f32) {
    let elapsed = cur.ts.duration_since(prev.ts).as_secs_f32().max(0.001);
    // Each sector = 512 bytes → KB/s
    let read_kbps =
        (cur.reads_sectors.saturating_sub(prev.reads_sectors)) as f32 * 512.0 / 1024.0 / elapsed;
    let write_kbps = (cur.writes_sectors.saturating_sub(prev.writes_sectors)) as f32 * 512.0
        / 1024.0
        / elapsed;
    (read_kbps, write_kbps)
}

// ── vcgencmd helpers ─────────────────────────────────────────────────────────

fn read_vcgencmd_clock(_source: &str) -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("vcgencmd")
            .args(["measure_clock", _source])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout);
        // "frequency(48)=1800000000"
        let hz: u64 = s.trim().split('=').nth(1)?.parse().ok()?;
        Some((hz / 1_000_000) as u32)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn read_vcgencmd_volts() -> Option<f32> {
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("vcgencmd")
            .args(["measure_volts", "core"])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout);
        // "volt=0.8750V"
        s.trim().strip_prefix("volt=")?.trim_end_matches('V').parse().ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn read_vcgencmd_throttled() -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("vcgencmd")
            .arg("get_throttled")
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout);
        // "throttled=0x0"
        let hex = s.trim().strip_prefix("throttled=0x")?;
        u32::from_str_radix(hex, 16).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn read_vcgencmd_gpu_mem() -> Option<u32> {
    #[cfg(target_os = "linux")]
    {
        let out = std::process::Command::new("vcgencmd")
            .args(["get_mem", "gpu"])
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&out.stdout);
        // "gpu=128M"
        s.trim().strip_prefix("gpu=")?.trim_end_matches('M').parse().ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

// ── /proc helpers ────────────────────────────────────────────────────────────

fn read_uptime() -> u64 {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/uptime").unwrap_or_default();
        content
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|f| f as u64)
            .unwrap_or(0)
    }
    #[cfg(not(target_os = "linux"))]
    {
        3600 * 60 // stub: 60 hours
    }
}

fn read_loadavg() -> [f32; 3] {
    #[cfg(target_os = "linux")]
    {
        let content = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
        let mut parts = content.split_whitespace();
        let a = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let b = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let c = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        [a, b, c]
    }
    #[cfg(not(target_os = "linux"))]
    {
        [1.5, 1.2, 0.9]
    }
}

// ── Static system info ───────────────────────────────────────────────────────

fn read_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| {
            hostname::get()
                .map(|h| h.to_string_lossy().into_owned())
                .unwrap_or_else(|_| "unknown".to_string())
        })
        .trim()
        .to_string()
}

fn read_model() -> String {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/device-tree/model")
            .unwrap_or_else(|_| "Unknown".to_string())
            .trim_matches('\0')
            .trim()
            .to_string()
    }
    #[cfg(not(target_os = "linux"))]
    {
        "Raspberry Pi 5 Model B Rev 1.0".to_string()
    }
}

fn read_os_info() -> (String, String) {
    #[cfg(target_os = "linux")]
    {
        let os = std::fs::read_to_string("/etc/os-release")
            .unwrap_or_default()
            .lines()
            .find(|l| l.starts_with("PRETTY_NAME="))
            .map(|l| {
                l.trim_start_matches("PRETTY_NAME=")
                    .trim_matches('"')
                    .to_string()
            })
            .unwrap_or_else(|| "Linux".to_string());

        let kernel = std::fs::read_to_string("/proc/version")
            .unwrap_or_default()
            .split_whitespace()
            .nth(2)
            .unwrap_or("unknown")
            .to_string();

        (os, kernel)
    }
    #[cfg(not(target_os = "linux"))]
    {
        ("Debian GNU/Linux 12 (bookworm)".to_string(), "6.6.74-v8+".to_string())
    }
}

fn check_hailo() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("which")
            .arg("hailortcli")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(target_os = "linux"))]
    {
        false
    }
}

// ── Utilities ────────────────────────────────────────────────────────────────

fn push_history(history: &mut Vec<f32>, value: f32, max_len: usize) {
    history.push(value);
    if history.len() > max_len {
        history.drain(..history.len() - max_len);
    }
}

// hostname crate shim for non-Linux
#[cfg(not(target_os = "linux"))]
mod hostname {
    pub fn get() -> Result<std::ffi::OsString, std::io::Error> {
        use std::ffi::OsString;
        Ok(OsString::from("mac-dev"))
    }
}
