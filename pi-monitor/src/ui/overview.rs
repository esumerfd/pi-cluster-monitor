use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
};

use crate::app::{HailoState, SystemMetrics};

pub fn render(frame: &mut Frame, area: Rect, m: &SystemMetrics, hailo: &HailoState) {
    if hailo.available {
        render_dual(frame, area, m, hailo);
    } else {
        render_single(frame, area, m, hailo);
    }
}

// ── Single-column layout (no Hailo) ──────────────────────────────────────────

fn render_single(frame: &mut Frame, area: Rect, m: &SystemMetrics, hailo: &HailoState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(3)])
        .split(area);

    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    render_system_info(frame, top_cols[0], m);
    render_status_and_temp(frame, top_cols[1], m, hailo);
    render_quick_metrics(frame, rows[1], m);
}

// ── Dual-column layout (Pi left | Hailo right) ───────────────────────────────
//
// Both columns share the same row heights so each row type (identity, metrics,
// temperature) is horizontally aligned across Pi and Hailo panels.
//
// Row heights:
//   identity  Min(8)    — 6 fields + 2 border lines
//   metrics   Length(6) — 4 gauge/text lines + 2 border lines
//   temp      Min(0)    — fills remaining space; right side appends inference below sparkline

fn render_dual(frame: &mut Frame, area: Rect, m: &SystemMetrics, hailo: &HailoState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let row_constraints = [
        Constraint::Min(8),     // identity
        Constraint::Length(6),  // metrics / utilisation
        Constraint::Min(0),     // temperature (fills rest)
    ];

    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(cols[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(cols[1]);

    render_system_info(frame, left[0], m);
    render_hailo_identity(frame, right[0], hailo);

    render_pi_metrics(frame, left[1], m);
    render_hailo_metrics(frame, right[1], hailo);

    render_pi_temp(frame, left[2], m);
    render_hailo_temp(frame, right[2], hailo);
}

// ── System info (shared by both layouts) ─────────────────────────────────────

fn render_system_info(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let uptime = fmt_uptime(m.uptime_secs);
    let ntp_str = if m.ntp_synced {
        format!("✓ Synced ({})", m.timezone)
    } else {
        "✗ Not synced".to_string()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("  Hostname:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                m.hostname.clone(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Model:     ", Style::default().fg(Color::DarkGray)),
            Span::raw(m.model.clone()),
        ]),
        Line::from(vec![
            Span::styled("  OS:        ", Style::default().fg(Color::DarkGray)),
            Span::raw(m.os_name.clone()),
        ]),
        Line::from(vec![
            Span::styled("  Kernel:    ", Style::default().fg(Color::DarkGray)),
            Span::raw(m.kernel.clone()),
        ]),
        Line::from(vec![
            Span::styled("  Uptime:    ", Style::default().fg(Color::DarkGray)),
            Span::raw(uptime),
        ]),
        Line::from(vec![
            Span::styled("  NTP:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                ntp_str,
                if m.ntp_synced {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Yellow)
                },
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" System ", Style::default().add_modifier(Modifier::BOLD))),
        ),
        area,
    );
}

// ── Status + temperature (single-column only) ─────────────────────────────────

fn render_status_and_temp(frame: &mut Frame, area: Rect, m: &SystemMetrics, hailo: &HailoState) {
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    let hailo_span = if hailo.available {
        Span::styled("  Hailo NPU  ● ONLINE", Style::default().fg(Color::Green))
    } else if m.hailo_available {
        Span::styled("  Hailo NPU  ● LOCAL", Style::default().fg(Color::Yellow))
    } else {
        Span::styled("  Hailo NPU  ○ —", Style::default().fg(Color::DarkGray))
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(hailo_span),
            Line::from(Span::styled("  Docker     ○ (Phase 2)", Style::default().fg(Color::DarkGray))),
        ])
        .block(Block::default().borders(Borders::ALL).title(
            Span::styled(" Status ", Style::default().add_modifier(Modifier::BOLD)),
        )),
        right_rows[0],
    );

    let temp_data: Vec<u64> = m.temp_history.iter().map(|&t| t.max(0.0) as u64).collect();
    let temp_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Temperature ", Style::default().add_modifier(Modifier::BOLD)));
    let inner = temp_block.inner(right_rows[1]);
    frame.render_widget(temp_block, right_rows[1]);

    if inner.height >= 2 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {:.1} °C  ({})", m.cpu_temp_c, temp_description(m.cpu_temp_c)),
                temp_color(m.cpu_temp_c),
            ))),
            rows[0],
        );
        frame.render_widget(
            Sparkline::default()
                .data(&temp_data)
                .max(100)
                .style(Style::default().fg(temp_color_raw(m.cpu_temp_c))),
            rows[1],
        );
    }
}

// ── Quick metrics strip (single-column only) ──────────────────────────────────

fn render_quick_metrics(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(" CPU "))
            .gauge_style(cpu_gauge_style(m.cpu_avg_pct))
            .ratio((m.cpu_avg_pct / 100.0).clamp(0.0, 1.0) as f64)
            .label(format!("CPU {:.0}%", m.cpu_avg_pct)),
        cols[0],
    );

    let ram_pct = if m.mem_total > 0 { m.mem_used as f64 / m.mem_total as f64 } else { 0.0 };
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(" RAM "))
            .gauge_style(Style::default().fg(Color::Blue))
            .ratio(ram_pct.clamp(0.0, 1.0))
            .label(format!(
                "{:.1}/{:.0} GB",
                m.mem_used as f64 / 1_073_741_824.0,
                m.mem_total as f64 / 1_073_741_824.0,
            )),
        cols[1],
    );

    let root_disk = m.disks.iter().find(|d| d.mount == "/");
    let (disk_pct, disk_label) = root_disk
        .map(|d| (d.used_pct() as f64 / 100.0, format!("/ {:.0}%", d.used_pct())))
        .unwrap_or((0.0, "/ —".to_string()));
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL).title(" Disk / "))
            .gauge_style(Style::default().fg(Color::Magenta))
            .ratio(disk_pct.clamp(0.0, 1.0))
            .label(disk_label),
        cols[2],
    );

    let fan_label = if m.fan_rpm > 0 { format!("{} RPM", m.fan_rpm) } else { "— RPM".to_string() };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {}", fan_label),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )))
        .block(Block::default().borders(Borders::ALL).title(" Fan ")),
        cols[3],
    );
}

// ── Pi metrics (dual-column left row 1) ──────────────────────────────────────

fn render_pi_metrics(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Metrics ", Style::default().add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // CPU
            Constraint::Length(1), // RAM
            Constraint::Length(1), // Disk
            Constraint::Length(1), // Fan
            Constraint::Min(0),
        ])
        .split(inner);

    let ram_pct = if m.mem_total > 0 { m.mem_used as f64 / m.mem_total as f64 } else { 0.0 };
    let root_disk = m.disks.iter().find(|d| d.mount == "/");
    let (disk_ratio, disk_label) = root_disk
        .map(|d| (d.used_pct() as f64 / 100.0, format!("Disk  {:.0}%", d.used_pct())))
        .unwrap_or((0.0, "Disk  —".to_string()));

    frame.render_widget(
        Gauge::default()
            .gauge_style(cpu_gauge_style(m.cpu_avg_pct))
            .ratio((m.cpu_avg_pct / 100.0).clamp(0.0, 1.0) as f64)
            .label(format!("CPU  {:.0}%", m.cpu_avg_pct)),
        rows[0],
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(Color::Blue))
            .ratio(ram_pct.clamp(0.0, 1.0))
            .label(format!(
                "RAM  {:.1}/{:.0} GB",
                m.mem_used as f64 / 1_073_741_824.0,
                m.mem_total as f64 / 1_073_741_824.0,
            )),
        rows[1],
    );
    frame.render_widget(
        Gauge::default()
            .gauge_style(Style::default().fg(Color::Magenta))
            .ratio(disk_ratio.clamp(0.0, 1.0))
            .label(disk_label),
        rows[2],
    );
    let fan_text = if m.fan_rpm > 0 {
        format!("Fan  {} RPM", m.fan_rpm)
    } else {
        "Fan  — RPM".to_string()
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            fan_text,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ))),
        rows[3],
    );
}

// ── Hailo identity (dual-column right row 0) ──────────────────────────────────

fn render_hailo_identity(frame: &mut Frame, area: Rect, h: &HailoState) {
    let d = &h.device;

    let pcie = match (&d.pcie_current_link_speed, &d.pcie_current_link_width) {
        (Some(speed), Some(width)) => {
            let gen_str = if speed.contains("8.0") { "Gen3" }
                         else if speed.contains("16.0") { "Gen4" }
                         else { speed.as_str() };
            format!("{} x{}", gen_str, width)
        }
        _ => "—".to_string(),
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("  FW:     ", Style::default().fg(Color::DarkGray)),
            Span::raw(d.fw_version.as_deref().unwrap_or("—")),
        ]),
        Line::from(vec![
            Span::styled("  Arch:   ", Style::default().fg(Color::DarkGray)),
            Span::raw(d.architecture.as_deref().unwrap_or("—")),
        ]),
        Line::from(vec![
            Span::styled("  PCIe:   ", Style::default().fg(Color::DarkGray)),
            Span::raw(pcie),
        ]),
        Line::from(vec![
            Span::styled("  NN Clk: ", Style::default().fg(Color::DarkGray)),
            Span::raw(d.nn_clock_mhz.map(|v| format!("{} MHz", v)).unwrap_or_else(|| "—".to_string())),
        ]),
        Line::from(vec![
            Span::styled("  DDR:    ", Style::default().fg(Color::DarkGray)),
            Span::raw(if d.ddr_total_gb > 0.0 { format!("{} GB", d.ddr_total_gb) } else { "—".to_string() }),
        ]),
        Line::from(vec![
            Span::styled("  Loaded: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                d.loaded_networks.to_string(),
                if d.loaded_networks > 0 {
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" Hailo-10H ", Style::default().add_modifier(Modifier::BOLD))),
        ),
        area,
    );
}

// ── Hailo utilisation metrics (dual-column right row 1) ───────────────────────

fn render_hailo_metrics(frame: &mut Frame, area: Rect, h: &HailoState) {
    let p = &h.perf;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Utilization ", Style::default().add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // NNC
            Constraint::Length(1), // SoC CPU
            Constraint::Length(1), // DSP
            Constraint::Length(1), // voltage + throttle combined
            Constraint::Min(0),
        ])
        .split(inner);

    for (i, (label, pct, color)) in [
        ("NNC ", p.nnc_utilization, Color::Cyan),
        ("CPU ", p.cpu_utilization, Color::Green),
        ("DSP ", p.dsp_utilization, Color::Yellow),
    ]
    .iter()
    .enumerate()
    {
        frame.render_widget(
            Gauge::default()
                .gauge_style(Style::default().fg(*color))
                .ratio((*pct / 100.0).clamp(0.0, 1.0) as f64)
                .label(format!("{}{:.0}%", label, pct)),
            rows[i],
        );
    }

    // Voltage and throttle on one line to fit in Length(6) block
    let (throttle_str, throttle_style) = match h.throttle_ok {
        Some(true) => ("✓ OK".to_string(), Style::default().fg(Color::Green)),
        Some(false) => {
            let flags = if h.throttle_flags.is_empty() {
                "issues".to_string()
            } else {
                h.throttle_flags.join("+")
            };
            (flags, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        }
        None => ("—".to_string(), Style::default().fg(Color::DarkGray)),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("Volt  ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{:.3}V", p.on_die_voltage / 1000.0)),
            Span::styled("   Throttle  ", Style::default().fg(Color::DarkGray)),
            Span::styled(throttle_str, throttle_style),
        ])),
        rows[3],
    );
}

// ── Pi temperature (dual-column left row 2) ───────────────────────────────────

fn render_pi_temp(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let temp_data: Vec<u64> = m.temp_history.iter().map(|&t| t.max(0.0) as u64).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Temperature ", Style::default().add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {:.1} °C  ({})", m.cpu_temp_c, temp_description(m.cpu_temp_c)),
            temp_color(m.cpu_temp_c),
        ))),
        rows[0],
    );
    frame.render_widget(
        Sparkline::default()
            .data(&temp_data)
            .max(100)
            .style(Style::default().fg(temp_color_raw(m.cpu_temp_c))),
        rows[1],
    );
}

// ── Hailo temperature + inference (dual-column right row 2) ───────────────────
//
// Layout within the block's inner area:
//   1 row  — temperature text
//   ≤3 rows — sparkline (capped so inference can appear on bigger terminals)
//   rest   — inference lines (sentinel fps, network names)

fn render_hailo_temp(frame: &mut Frame, area: Rect, h: &HailoState) {
    let temp_data: Vec<u64> = h.temp_history.iter().map(|&t| t.max(0.0) as u64).collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Temperature ", Style::default().add_modifier(Modifier::BOLD)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 2 {
        return;
    }

    let spark_rows = inner.height.saturating_sub(1).min(3);
    let extra = inner.height.saturating_sub(1 + spark_rows);

    let mut c: Vec<Constraint> = vec![
        Constraint::Length(1),           // temp text
        Constraint::Length(spark_rows),  // sparkline
    ];
    if extra > 0 {
        c.push(Constraint::Length(extra));
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(c)
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  {:.1} °C  ({})", h.perf.on_die_temperature, temp_description(h.perf.on_die_temperature)),
            temp_color(h.perf.on_die_temperature),
        ))),
        rows[0],
    );
    frame.render_widget(
        Sparkline::default()
            .data(&temp_data)
            .max(100)
            .style(Style::default().fg(temp_color_raw(h.perf.on_die_temperature))),
        rows[1],
    );

    if extra > 0 {
        let d = &h.device;
        let mut lines = vec![Line::from("")]; // blank separator
        let sentinel_line = match h.sentinel_fps {
            Some(fps) => Line::from(vec![
                Span::styled("  Sentinel: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.1} fps", fps),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
            ]),
            None => Line::from(Span::styled(
                "  Sentinel: offline",
                Style::default().fg(Color::DarkGray),
            )),
        };
        lines.push(sentinel_line);
        if extra >= 3 {
            if let Some(name) = d.network_names.first() {
                lines.push(Line::from(vec![
                    Span::styled("  Network:  ", Style::default().fg(Color::DarkGray)),
                    Span::styled(name.clone(), Style::default().fg(Color::White)),
                ]));
            }
        }
        frame.render_widget(Paragraph::new(lines), rows[2]);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_uptime(secs: u64) -> String {
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        format!("{}d {}h {}m", days, hours, mins)
    } else if hours > 0 {
        format!("{}h {}m", hours, mins)
    } else {
        format!("{}m", mins)
    }
}

fn temp_description(t: f32) -> &'static str {
    if t >= 80.0 { "hot" } else if t >= 65.0 { "warm" } else if t >= 50.0 { "moderate" } else { "cool" }
}

fn temp_color(t: f32) -> Style {
    if t >= 80.0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if t >= 65.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

fn temp_color_raw(t: f32) -> Color {
    if t >= 80.0 { Color::Red } else if t >= 65.0 { Color::Yellow } else { Color::Green }
}

fn cpu_gauge_style(pct: f32) -> Style {
    if pct >= 90.0 {
        Style::default().fg(Color::Red)
    } else if pct >= 70.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}
