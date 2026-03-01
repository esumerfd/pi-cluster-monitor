# Implementation Plan

## Phase 1: Foundation — Working TUI Shell

**Goal:** A compiled Rust binary that opens a TUI, renders tabs, handles keyboard input, and shows local network interface statistics.

### Steps

1. **Initialize Rust project**
   - `cargo new pi-monitor --bin`
   - Set up `Cargo.toml` with initial dependencies: `ratatui`, `crossterm`, `tokio`
   - Establish module structure: `main.rs`, `app.rs`, `ui/`, `network/`, `config/`

2. **Implement App state machine**
   - Define `App` struct holding current tab, selected row, running flag
   - Implement event loop: poll terminal events, update state, render frame
   - Handle `q` / `Ctrl+C` to exit; arrow keys for navigation

3. **Build tab layout**
   - Top tab bar: `Network | Nodes | Connections | Logs`
   - Main content area (switches based on active tab)
   - Status bar at bottom (clock, selected item summary, key hints)

4. **Local network interface panel** (Network tab, first pass)
   - Use `sysinfo` crate to list local interfaces
   - Show interface name, IP, bytes sent/received, link state
   - Auto-refresh every 1 second via `tokio::time::interval`

5. **Basic configuration**
   - `config.toml` in `~/.config/pi-monitor/` or project dir
   - Fields: refresh interval, network interface to monitor, static node list
   - Load on startup with `serde` + `toml`

**Deliverable:** `pi-monitor` binary that opens, shows network interfaces, navigates tabs, and exits cleanly.

---

## Phase 2: Network Connection View (rustnet-inspired)

**Goal:** Show active network connections on the local machine, styled after rustnet's connection-centric view.

### Steps

6. **Integrate libpcap**
   - Add `pcap` crate dependency
   - Capture packets on the primary interface
   - Parse Ethernet → IP → TCP/UDP headers

7. **Connection tracking**
   - Define `Connection` struct: src IP/port, dst IP/port, protocol, state, bytes, last seen
   - HashMap-based connection table keyed by 5-tuple
   - Apply lifecycle logic: active → stale (yellow) → expired (red) with configurable timeouts

8. **Connections tab UI**
   - Scrollable table: PID/Process | Protocol | Local | Remote | State | ↑ Bytes | ↓ Bytes | Age
   - Column sorting (click header or `s` key to cycle sort field)
   - `/` to enter filter mode; filter by `port:`, `process:`, `state:` prefixes

9. **Process attribution**
   - macOS: parse `/proc`-equivalent via `lsof` subprocess or `libproc` bindings
   - Linux: `/proc/net/tcp`, `/proc/net/udp` + `/proc/PID/fd`
   - Map 5-tuple → PID → process name

10. **Reverse DNS + GeoIP** (optional, Phase 2 stretch)
    - Async DNS resolution with LRU cache; display resolved hostname
    - GeoLite2-City database lookup for remote IPs; show country code

**Deliverable:** Connections tab showing live local connections with process names and bandwidth.

---

## Phase 3: Node Discovery

**Goal:** Automatically discover Pi cluster nodes on the LAN and display their reachability.

### Steps

11. **Node config + static list**
    - Extend `config.toml`: `[[nodes]]` array with `name`, `ip`, `user`, `ssh_key`
    - Render Nodes tab with static list first

12. **Reachability polling**
    - ICMP ping each node every 5 seconds (use `ping` subprocess or raw ICMP socket)
    - Track: last seen timestamp, round-trip time, consecutive failures
    - Color-code rows: green (up) / red (down) / yellow (degraded)

13. **mDNS discovery** (auto-discover nodes)
    - Use `mdns-sd` crate to listen for `_ssh._tcp.local` and similar services
    - Merge discovered nodes with static config list
    - Show discovery source: `static` | `mDNS`

14. **ARP scan fallback**
    - For subnets without mDNS, send ARP requests across CIDR range
    - Match MAC OUI prefix against known Raspberry Pi Foundation OUIs
    - Add discovered nodes to list with `ARP` source tag

**Deliverable:** Nodes tab with live reachability indicators for all cluster nodes.

---

## Phase 4: Node Health Metrics

**Goal:** Show CPU, RAM, disk, temperature, and load per node by polling over SSH.

### Steps

15. **SSH connection pool**
    - Use `russh` or `ssh2` crate
    - Maintain persistent SSH connections per node (reconnect on failure)
    - Connection pool managed as background Tokio tasks

16. **Remote metric collection**
    - Run one-liner shell commands over SSH to read procfs/sysfs:
      - CPU: `/proc/stat` delta
      - RAM: `/proc/meminfo`
      - Disk: `df -h /`
      - Temp: `/sys/class/thermal/thermal_zone0/temp`
      - Load: `/proc/loadavg`
    - Parse output in Rust; store in `NodeMetrics` struct

17. **Node detail panel**
    - Pressing `Enter` on a node opens a detail panel (split view or modal)
    - Sparkline charts for CPU and RAM history (last 60 samples)
    - Disk bar, temperature, uptime, load averages

18. **Threshold alerting**
    - Config-driven thresholds: CPU %, RAM %, disk %, temp °C
    - Highlight cell in red when threshold exceeded
    - Status bar alert count badge

**Deliverable:** Per-node health stats visible in the Nodes tab with history sparklines.

---

## Phase 5: Log Tailing

**Goal:** Stream log output from remote nodes into a Logs tab.

### Steps

19. **Log source config**
    - `config.toml` `[[logs]]` entries: `node`, `path`, `label`
    - Examples: `/var/log/syslog`, journald via `journalctl -f`, custom app logs

20. **Remote log streaming**
    - `tail -f <path>` or `journalctl -f` over persistent SSH channel
    - Ring buffer per log source (last N lines, configurable)

21. **Logs tab UI**
    - Left panel: list of log sources (node + label)
    - Right panel: scrollable log output for selected source
    - Highlight lines matching configurable regex patterns (errors, warnings)
    - `/` to filter displayed lines

**Deliverable:** Live log tailing from any configured node, with pattern highlighting.

---

## Phase 6: Polish and Extensibility

**Goal:** Production-quality UX and a foundation for future data sources.

### Steps

22. **Help overlay**
    - `?` opens a modal with all keybindings per tab

23. **Mouse support**
    - Click to select rows, click tab headers to switch

24. **Configuration TUI**
    - Config tab for editing node list, thresholds, log sources without editing TOML by hand

25. **Export / snapshot**
    - `e` to export current view to JSON or CSV
    - Screenshot to file (capture terminal frame as text)

26. **Plugin/script panel**
    - Define custom panels via config: run a local or remote script, display stdout as table or text
    - Enables arbitrary extensibility without modifying core binary

27. **Packaging**
    - Single static binary via `cargo build --release`
    - Homebrew formula for macOS install
    - Debian/RPM package for Pi nodes (if agent needed)
    - GitHub Actions CI: build + test on push

---

## Dependency Summary

| Crate | Purpose |
|-------|---------|
| `ratatui` | TUI rendering |
| `crossterm` | Terminal events, raw mode |
| `tokio` | Async runtime |
| `serde` + `toml` | Config file |
| `pcap` | Packet capture (libpcap) |
| `sysinfo` | Local system metrics |
| `mdns-sd` | mDNS node discovery |
| `russh` or `ssh2` | SSH remote polling |
| `clap` | CLI argument parsing |
| `tracing` | Structured logging |
| `anyhow` | Error handling |

---

## Development Milestones

| Milestone | Phases | Description |
|-----------|--------|-------------|
| M1 | 1 | TUI shell with tabs and local interface stats |
| M2 | 1–2 | Local connection view (rustnet parity) |
| M3 | 2–3 | Node discovery and reachability |
| M4 | 3–4 | Node health metrics over SSH |
| M5 | 4–5 | Log tailing |
| M6 | 5–6 | Polish, packaging, extensibility |
