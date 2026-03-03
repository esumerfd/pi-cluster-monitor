/// Background collector: polls /proc, /sys, vcgencmd, and hailortcli every
/// `interval_ms` milliseconds and writes the result to shared state.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::time::interval;
use tracing::debug;

use crate::types::*;

// ── Internal state tracking ────────────────────────────────────────────────

struct CpuPrev {
    active: u64,
    total: u64,
}

impl Default for CpuPrev {
    fn default() -> Self {
        Self { active: 0, total: 0 }
    }
}

struct CollectorCtx {
    /// Previous per-core (active, total) jiffies
    cpu_cores: Vec<CpuPrev>,
    /// Previous aggregate CPU line (active, total)
    cpu_agg: CpuPrev,
    /// Delta total jiffies from last tick — used for process CPU%
    cpu_delta_total: u64,
    /// Previous (utime + stime) per PID
    proc_jiffies: HashMap<u32, u64>,
    /// Hailo poll counter: poll every 5th 2-second tick
    hailo_tick: u32,
    /// Cached Hailo identity (updated every hailo poll)
    hailo: HailoInfo,
    /// Cached Hailo perf (updated every hailo poll)
    hailo_perf: HailoPerfInfo,
}

impl Default for CollectorCtx {
    fn default() -> Self {
        Self {
            cpu_cores: Vec::new(),
            cpu_agg: CpuPrev::default(),
            cpu_delta_total: 0,
            proc_jiffies: HashMap::new(),
            hailo_tick: 0,
            hailo: HailoInfo::default(),
            hailo_perf: HailoPerfInfo::default(),
        }
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

pub async fn run(state: Arc<RwLock<ApiStats>>, interval_ms: u64) {
    let mut ticker = interval(Duration::from_millis(interval_ms));
    let mut ctx = CollectorCtx::default();

    loop {
        ticker.tick().await;

        let poll_hailo = ctx.hailo_tick == 0;
        ctx.hailo_tick = (ctx.hailo_tick + 1) % 5;

        let stats = collect(&mut ctx, poll_hailo).await;
        *state.write().unwrap() = stats;
        debug!("collector tick complete");
    }
}

async fn collect(ctx: &mut CollectorCtx, poll_hailo: bool) -> ApiStats {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();

    let system = collect_system().await;
    let memory = collect_memory();
    let cpu = collect_cpu(ctx).await;
    let temperature = collect_temperature().await;
    let disk = collect_disk().await;
    let fan = collect_fan();
    let processes = collect_processes(ctx, memory.total_kb);

    if poll_hailo {
        if std::path::Path::new("/dev/hailo0").exists() {
            ctx.hailo = collect_hailo_identity().await;
            ctx.hailo_perf = collect_hailo_perf().await;
        } else {
            ctx.hailo = HailoInfo::default();
            ctx.hailo_perf = HailoPerfInfo::default();
        }
    }

    ApiStats {
        ts,
        system,
        cpu,
        temperature,
        memory,
        disk,
        fan,
        processes,
        hailo: ctx.hailo.clone(),
        hailo_perf: ctx.hailo_perf.clone(),
    }
}

// ── Subsystem collectors ───────────────────────────────────────────────────

async fn collect_system() -> SystemInfo {
    let hostname = read_file("/proc/sys/kernel/hostname")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    let model = read_file("/proc/device-tree/model")
        .map(|s| s.trim_end_matches('\0').trim().to_string())
        .unwrap_or_default();

    let os = parse_os_release();
    let kernel = read_file("/proc/version")
        .and_then(|s| s.split_whitespace().nth(2).map(|v| v.to_string()))
        .unwrap_or_default();

    let uptime_s = read_file("/proc/uptime")
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
        .unwrap_or(0.0);

    let cpu_count = read_file("/proc/cpuinfo")
        .map(|s| s.lines().filter(|l| l.starts_with("processor")).count() as u32)
        .unwrap_or(0);

    let (ntp_synced, timezone) = read_timedatectl().await;

    SystemInfo {
        hostname,
        model,
        os,
        kernel,
        uptime_s,
        cpu_count,
        ntp_synced,
        timezone,
    }
}

async fn collect_cpu(ctx: &mut CollectorCtx) -> CpuInfo {
    let content = read_file("/proc/stat").unwrap_or_default();

    let mut new_cores: Vec<CpuPrev> = Vec::new();
    let mut per_core_pct: Vec<f64> = Vec::new();
    let mut new_agg = CpuPrev::default();

    for line in content.lines() {
        if line.starts_with("cpu ") {
            if let Some((active, total)) = parse_cpu_stat_line(line) {
                let delta_total = total.saturating_sub(ctx.cpu_agg.total);
                let delta_active = active.saturating_sub(ctx.cpu_agg.active);
                ctx.cpu_delta_total = delta_total;
                let _ = if delta_total > 0 {
                    delta_active as f64 / delta_total as f64 * 100.0
                } else {
                    0.0
                };
                new_agg = CpuPrev { active, total };
            }
        } else if line.starts_with("cpu") && line.chars().nth(3).map_or(false, |c| c.is_ascii_digit()) {
            if let Some((active, total)) = parse_cpu_stat_line(line) {
                let core_idx = new_cores.len();
                let pct = if let Some(prev) = ctx.cpu_cores.get(core_idx) {
                    let dt = total.saturating_sub(prev.total);
                    let da = active.saturating_sub(prev.active);
                    if dt > 0 { da as f64 / dt as f64 * 100.0 } else { 0.0 }
                } else {
                    0.0
                };
                per_core_pct.push(pct);
                new_cores.push(CpuPrev { active, total });
            }
        }
    }

    ctx.cpu_cores = new_cores;
    ctx.cpu_agg = new_agg;

    let freq_mhz = vcgencmd("measure_clock arm").await
        .and_then(|s| {
            // "frequency(0)=2400000000"
            s.split('=').nth(1)?.trim().parse::<f64>().ok()
        })
        .map(|hz| hz / 1_000_000.0)
        .unwrap_or(0.0);

    let core_v = vcgencmd("measure_volts core").await
        .and_then(|s| {
            // "volt=0.8875V"
            let v = s.split('=').nth(1)?;
            v.trim_end_matches('V').trim().parse::<f64>().ok()
        })
        .unwrap_or(0.0);

    let (throttle_ok, throttle_flags) = read_throttle().await;

    let load_avg = read_loadavg();

    CpuInfo {
        freq_mhz,
        core_v,
        throttle_ok,
        throttle_flags,
        load_1: load_avg[0],
        load_5: load_avg[1],
        load_15: load_avg[2],
        per_core_pct,
    }
}

async fn collect_temperature() -> TemperatureInfo {
    let cpu_c = vcgencmd("measure_temp").await
        .and_then(|s| {
            // "temp=46.2'C"
            let v = s.split('=').nth(1)?;
            v.split('\'').next()?.trim().parse::<f64>().ok()
        })
        .unwrap_or(0.0);

    // RP1 temperature from hwmon — look for a hwmon that has "rp1" in name
    let rp1_c = find_rp1_temp();

    TemperatureInfo { cpu_c, rp1_c }
}

fn collect_memory() -> MemoryInfo {
    let content = read_file("/proc/meminfo").unwrap_or_default();
    let mut map: HashMap<&str, u64> = HashMap::new();
    for line in content.lines() {
        let mut parts = line.splitn(2, ':');
        if let (Some(key), Some(val)) = (parts.next(), parts.next()) {
            if let Some(kb) = val.split_whitespace().next().and_then(|v| v.parse::<u64>().ok()) {
                map.insert(key.trim(), kb);
            }
        }
    }

    let total_kb = map.get("MemTotal").copied().unwrap_or(0);
    let available_kb = map.get("MemAvailable").copied().unwrap_or(0);
    let free_kb = map.get("MemFree").copied().unwrap_or(0);
    let buffers_kb = map.get("Buffers").copied().unwrap_or(0);
    let cached_kb = map.get("Cached").copied().unwrap_or(0);
    let used_kb = total_kb.saturating_sub(free_kb + buffers_kb + cached_kb);

    MemoryInfo {
        total_kb,
        used_kb,
        available_kb,
        swap_total_kb: map.get("SwapTotal").copied().unwrap_or(0),
        swap_used_kb: map
            .get("SwapTotal")
            .copied()
            .unwrap_or(0)
            .saturating_sub(map.get("SwapFree").copied().unwrap_or(0)),
    }
}

async fn collect_disk() -> Vec<DiskInfo> {
    let out = run_cmd("df", &["-Pk"]).await.unwrap_or_default();
    let mut disks = Vec::new();

    for line in out.lines().skip(1) {
        // Filesystem  1024-blocks  Used  Available  Use%  Mounted on
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }
        let mount = parts[5].to_string();
        // Skip pseudo/virtual filesystems
        if !mount.starts_with('/') || mount.starts_with("/sys") || mount.starts_with("/proc") || mount.starts_with("/dev") {
            continue;
        }
        let total_kb: u64 = parts[1].parse().unwrap_or(0);
        let used_kb: u64 = parts[2].parse().unwrap_or(0);
        let used_pct = if total_kb > 0 { used_kb as f64 / total_kb as f64 * 100.0 } else { 0.0 };
        disks.push(DiskInfo { mount, total_kb, used_kb, used_pct });
    }

    disks
}

fn collect_fan() -> FanInfo {
    let rpm = find_fan_rpm().unwrap_or(0);
    FanInfo { rpm }
}

fn collect_processes(ctx: &mut CollectorCtx, mem_total_kb: u64) -> Vec<ProcessInfo> {
    let delta_total = ctx.cpu_delta_total;
    let ncores = ctx.cpu_cores.len().max(1) as f64;
    let uid_map = read_uid_map();

    let mut procs: Vec<ProcessInfo> = Vec::new();

    let dir = match std::fs::read_dir("/proc") {
        Ok(d) => d,
        Err(_) => return procs,
    };

    let mut new_proc_jiffies: HashMap<u32, u64> = HashMap::new();

    for entry in dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };

        let Some((jiffies, rss_kb, uid, command)) = read_proc_info(pid) else {
            continue;
        };

        let prev_jiffies = ctx.proc_jiffies.get(&pid).copied().unwrap_or(jiffies);
        let delta_proc = jiffies.saturating_sub(prev_jiffies);
        let cpu_pct = if delta_total > 0 {
            delta_proc as f64 / delta_total as f64 * ncores * 100.0
        } else {
            0.0
        };

        let user = uid_map.get(&uid).cloned().unwrap_or_else(|| uid.to_string());
        let mem_pct = if mem_total_kb > 0 { rss_kb as f64 / mem_total_kb as f64 * 100.0 } else { 0.0 };
        let rss_mb = rss_kb as f64 / 1024.0;

        new_proc_jiffies.insert(pid, jiffies);
        procs.push(ProcessInfo { pid, user, cpu_pct, mem_pct, rss_mb, command });
    }

    ctx.proc_jiffies = new_proc_jiffies;

    procs.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    procs.truncate(15);
    procs
}

async fn collect_hailo_identity() -> HailoInfo {
    let out = run_cmd("hailortcli", &["fw-control", "identify"]).await.unwrap_or_default();
    if out.is_empty() {
        return HailoInfo { present: true, ..Default::default() };
    }

    let mut info = HailoInfo {
        present: true,
        firmware_ok: true,
        ddr_total_gb: 1.0, // Hailo-10H has 1 GB onboard DDR
        ..Default::default()
    };

    for line in out.lines() {
        let line = line.trim();
        if let Some(v) = strip_prefix_val(line, "Firmware Version:") {
            info.fw_version = Some(v.split_whitespace().next().unwrap_or(v).to_string());
        } else if let Some(v) = strip_prefix_val(line, "Device Architecture:") {
            info.architecture = Some(v.to_string());
        } else if let Some(v) = strip_prefix_val(line, "Neural Network Core Clock Rate:") {
            // "1000 MHz"
            if let Some(mhz) = v.split_whitespace().next().and_then(|s| s.parse::<u32>().ok()) {
                info.nn_clock_mhz = Some(mhz);
            }
        }
    }

    // Try to get PCIe info from scan
    if let Some(scan) = run_cmd("hailortcli", &["scan"]).await {
        for line in scan.lines() {
            let line = line.trim();
            if let Some(v) = strip_prefix_val(line, "PCIe link speed:") {
                info.pcie_current_link_speed = Some(v.to_string());
            } else if let Some(v) = strip_prefix_val(line, "PCIe link width:") {
                info.pcie_current_link_width = Some(v.to_string());
            }
        }
    }

    info
}

async fn collect_hailo_perf() -> HailoPerfInfo {
    let out = run_cmd("hailo_perf_query", &[]).await.unwrap_or_default();
    if out.is_empty() {
        return HailoPerfInfo::default();
    }

    let mut perf = HailoPerfInfo::default();
    for line in out.lines() {
        let line = line.trim();
        if let Some(v) = strip_prefix_val(line, "NNC Utilization:") {
            perf.nnc_utilization = v.split('%').next().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
        } else if let Some(v) = strip_prefix_val(line, "CPU Utilization:") {
            perf.cpu_utilization = v.split('%').next().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
        } else if let Some(v) = strip_prefix_val(line, "DSP Utilization:") {
            perf.dsp_utilization = v.split('%').next().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0);
        } else if let Some(v) = strip_prefix_val(line, "On-Die Temperature:") {
            perf.on_die_temperature = v.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
        } else if let Some(v) = strip_prefix_val(line, "On-Die Voltage:") {
            perf.on_die_voltage = v.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if let Some(v) = strip_prefix_val(line, "RAM Total:") {
            perf.ram_size_total = v.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
        } else if let Some(v) = strip_prefix_val(line, "RAM Used:") {
            perf.ram_size_used = v.split_whitespace().next().and_then(|s| s.parse().ok()).unwrap_or(0);
        }
    }

    perf
}

// ── Low-level helpers ──────────────────────────────────────────────────────

fn read_file(path: &str) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

async fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    let out = tokio::process::Command::new(cmd)
        .args(args)
        .output()
        .await
        .ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout).ok()
    } else {
        None
    }
}

async fn vcgencmd(arg: &str) -> Option<String> {
    run_cmd("vcgencmd", &[arg]).await
}

fn parse_os_release() -> String {
    let content = read_file("/etc/os-release").unwrap_or_default();
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
            return rest.trim_matches('"').to_string();
        }
    }
    String::new()
}

async fn read_timedatectl() -> (bool, String) {
    let out = run_cmd("timedatectl", &["show"]).await.unwrap_or_default();
    let mut ntp_synced = false;
    let mut timezone = String::new();
    for line in out.lines() {
        if let Some(v) = line.strip_prefix("NTPSynchronized=") {
            ntp_synced = v.trim() == "yes";
        } else if let Some(v) = line.strip_prefix("Timezone=") {
            timezone = v.trim().to_string();
        }
    }
    (ntp_synced, timezone)
}

/// Parse a /proc/stat cpu* line into (active_jiffies, total_jiffies)
fn parse_cpu_stat_line(line: &str) -> Option<(u64, u64)> {
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|s| s.parse().ok())
        .collect();
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0); // idle + iowait
    let total: u64 = values.iter().sum();
    let active = total.saturating_sub(idle);
    Some((active, total))
}

async fn read_throttle() -> (bool, Vec<String>) {
    let raw = vcgencmd("get_throttled").await.unwrap_or_default();
    // "throttled=0x00050005"
    let hex = raw
        .split('=')
        .nth(1)
        .and_then(|v| v.trim().strip_prefix("0x"))
        .and_then(|v| u32::from_str_radix(v, 16).ok())
        .unwrap_or(0);
    let flags = decode_throttle_flags(hex);
    let ok = flags.is_empty();
    (ok, flags)
}

fn decode_throttle_flags(mask: u32) -> Vec<String> {
    const BITS: &[(u32, &str)] = &[
        (0, "under-voltage"),
        (1, "arm-frequency-capped"),
        (2, "throttled"),
        (3, "soft-temperature-limit"),
        (16, "under-voltage-occurred"),
        (17, "arm-frequency-capping-occurred"),
        (18, "throttling-occurred"),
        (19, "soft-temperature-limit-occurred"),
    ];
    BITS.iter()
        .filter(|(bit, _)| mask & (1 << bit) != 0)
        .map(|(_, name)| name.to_string())
        .collect()
}

fn read_loadavg() -> [f64; 3] {
    let content = read_file("/proc/loadavg").unwrap_or_default();
    let mut parts = content.split_whitespace();
    let a = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let b = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let c = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    [a, b, c]
}

fn find_rp1_temp() -> Option<f64> {
    for entry in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let hwmon = entry.path();
        let name = read_file(hwmon.join("name").to_str()?)?;
        if name.trim() != "rp1_adc" {
            continue;
        }
        // Look for temp1_input
        let raw = read_file(hwmon.join("temp1_input").to_str()?)?;
        let millideg: f64 = raw.trim().parse().ok()?;
        return Some(millideg / 1000.0);
    }
    None
}

fn find_fan_rpm() -> Option<u32> {
    for entry in std::fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let hwmon = entry.path();
        let fan_path = hwmon.join("fan1_input");
        if fan_path.exists() {
            let raw = read_file(fan_path.to_str()?)?;
            return raw.trim().parse().ok();
        }
    }
    None
}

/// Read /proc/[pid]/stat, /proc/[pid]/status, /proc/[pid]/cmdline
/// Returns (cpu_jiffies, rss_kb, uid, command) or None if the process vanished.
fn read_proc_info(pid: u32) -> Option<(u64, u64, u32, String)> {
    let base = format!("/proc/{}", pid);

    let stat = read_file(&format!("{}/stat", base))?;
    let (utime, stime) = parse_proc_stat(&stat)?;
    let jiffies = utime + stime;

    let status = read_file(&format!("{}/status", base)).unwrap_or_default();
    let rss_kb = parse_status_field(&status, "VmRSS").unwrap_or(0);
    let uid = parse_status_field_u32(&status, "Uid").unwrap_or(0);

    let cmdline = read_file(&format!("{}/cmdline", base)).unwrap_or_default();
    let command = if cmdline.is_empty() {
        // Kernel thread: use comm from stat
        parse_proc_comm(&stat).unwrap_or_default()
    } else {
        cmdline.replace('\0', " ").trim().to_string()
    };

    Some((jiffies, rss_kb, uid, command))
}

/// Parse (utime, stime) from /proc/[pid]/stat
fn parse_proc_stat(content: &str) -> Option<(u64, u64)> {
    // Format: pid (comm) state ppid ... utime stime ...
    // comm can contain spaces, so find the last ')' to skip it
    let close = content.rfind(')')?;
    let rest = content[close + 1..].trim();
    // Fields after state: state ppid pgrp session tty_nr tty_pgrp flags
    //   minflt cminflt majflt cmajflt utime stime
    // Indices (0-based from 'rest' split): 0=state, 11=utime, 12=stime
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some((utime, stime))
}

/// Extract the process name from /proc/[pid]/stat (between parentheses)
fn parse_proc_comm(content: &str) -> Option<String> {
    let open = content.find('(')?;
    let close = content.rfind(')')?;
    Some(format!("[{}]", &content[open + 1..close]))
}

/// Parse a KB value from /proc/[pid]/status (e.g. "VmRSS: 12345 kB")
fn parse_status_field(content: &str, field: &str) -> Option<u64> {
    for line in content.lines() {
        if line.starts_with(field) {
            return line.split_whitespace().nth(1)?.parse().ok();
        }
    }
    None
}

/// Parse the first UID from /proc/[pid]/status "Uid: real eff saved fs"
fn parse_status_field_u32(content: &str, field: &str) -> Option<u32> {
    for line in content.lines() {
        if line.starts_with(field) {
            return line.split_whitespace().nth(1)?.parse().ok();
        }
    }
    None
}

/// Build a UID→username map from /etc/passwd
fn read_uid_map() -> HashMap<u32, String> {
    let mut map = HashMap::new();
    let content = read_file("/etc/passwd").unwrap_or_default();
    for line in content.lines() {
        let mut parts = line.split(':');
        if let (Some(name), _, Some(uid_str)) = (parts.next(), parts.next(), parts.next()) {
            if let Ok(uid) = uid_str.parse::<u32>() {
                map.insert(uid, name.to_string());
            }
        }
    }
    map
}

fn strip_prefix_val<'a>(line: &'a str, prefix: &str) -> Option<&'a str> {
    line.strip_prefix(prefix).map(|s| s.trim())
}

// ── Unit tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_clean() {
        let flags = decode_throttle_flags(0x0);
        assert!(flags.is_empty());
    }

    #[test]
    fn throttle_under_voltage() {
        let flags = decode_throttle_flags(0x1);
        assert_eq!(flags, vec!["under-voltage"]);
    }

    #[test]
    fn throttle_multiple_flags() {
        // bits 0 and 2: under-voltage + currently throttled
        let flags = decode_throttle_flags(0x5);
        assert!(flags.contains(&"under-voltage".to_string()));
        assert!(flags.contains(&"throttled".to_string()));
    }

    #[test]
    fn throttle_historical_flags() {
        // bits 16-19: historical events
        let flags = decode_throttle_flags(0x000f0000);
        assert!(flags.contains(&"under-voltage-occurred".to_string()));
        assert!(flags.contains(&"arm-frequency-capping-occurred".to_string()));
        assert!(flags.contains(&"throttling-occurred".to_string()));
        assert!(flags.contains(&"soft-temperature-limit-occurred".to_string()));
    }

    #[test]
    fn cpu_stat_parse_aggregate() {
        let line = "cpu  1000 0 500 8000 100 0 50 0 0 0";
        let (active, total) = parse_cpu_stat_line(line).unwrap();
        // idle = 8000 + 100 = 8100, total = sum = 9650, active = 9650 - 8100 = 1550
        assert_eq!(total, 9650);
        assert_eq!(active, 1550);
    }

    #[test]
    fn proc_stat_parse() {
        let line = "1234 (myproc) S 1 1 1 0 -1 4194560 100 0 0 0 200 50 0 0 20 0 1 0 123456 12345678 3000";
        let (utime, stime) = parse_proc_stat(line).unwrap();
        assert_eq!(utime, 200);
        assert_eq!(stime, 50);
    }

    #[test]
    fn proc_stat_parse_comm_with_spaces() {
        // comm contains spaces — should still parse correctly
        let line = "5678 (my proc) R 1 1 1 0 -1 4194560 0 0 0 0 100 25 0 0 20 0 1 0 100 98304 2048";
        let (utime, stime) = parse_proc_stat(line).unwrap();
        assert_eq!(utime, 100);
        assert_eq!(stime, 25);
    }

    #[test]
    fn loadavg_parse() {
        // Simulate /proc/loadavg content
        let content = "0.52 0.48 0.45 2/345 6789";
        let mut parts = content.split_whitespace();
        let a: f64 = parts.next().unwrap().parse().unwrap();
        let b: f64 = parts.next().unwrap().parse().unwrap();
        let c: f64 = parts.next().unwrap().parse().unwrap();
        assert!((a - 0.52).abs() < 1e-6);
        assert!((b - 0.48).abs() < 1e-6);
        assert!((c - 0.45).abs() < 1e-6);
    }
}
