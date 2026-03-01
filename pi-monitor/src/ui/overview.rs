use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Sparkline},
};

use crate::app::SystemMetrics;

pub fn render(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    // Main layout: top row (system + status) | quick metrics strip
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),      // top row
            Constraint::Length(3),   // quick metrics
        ])
        .split(area);

    // Top row: left (system info) | right (status + temp)
    let top_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    render_system_info(frame, top_cols[0], m);
    render_status_and_temp(frame, top_cols[1], m);
    render_quick_metrics(frame, rows[1], m);
}

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
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
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

    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                " System ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(para, area);
}

fn render_status_and_temp(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);

    // ── Status badges ─────────────────────────────────────────────────────────
    let hailo_span = if m.hailo_available {
        Span::styled("  Hailo NPU  ● ONLINE", Style::default().fg(Color::Green))
    } else {
        Span::styled(
            "  Hailo NPU  ○ NOT DETECTED",
            Style::default().fg(Color::DarkGray),
        )
    };

    let status_lines = vec![
        Line::from(hailo_span),
        Line::from(Span::styled(
            "  Docker     ○ (Phase 2)",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let status_para = Paragraph::new(status_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                " Status ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(status_para, right_rows[0]);

    // ── Temperature sparkline ─────────────────────────────────────────────────
    let temp_data: Vec<u64> = m
        .temp_history
        .iter()
        .map(|&t| t.max(0.0) as u64)
        .collect();

    let temp_label = format!("  {:.1} °C", m.cpu_temp_c);
    let temp_desc = temp_description(m.cpu_temp_c);

    let temp_block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Temperature ",
            Style::default().add_modifier(Modifier::BOLD),
        ));

    let inner = temp_block.inner(right_rows[1]);
    frame.render_widget(temp_block, right_rows[1]);

    if inner.height >= 3 {
        let temp_rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        let label_line =
            Paragraph::new(Line::from(Span::styled(
                format!("{}  ({})", temp_label, temp_desc),
                temp_color(m.cpu_temp_c),
            )));
        frame.render_widget(label_line, temp_rows[0]);

        let sparkline = Sparkline::default()
            .data(&temp_data)
            .max(100)
            .style(Style::default().fg(temp_color_raw(m.cpu_temp_c)));
        frame.render_widget(sparkline, temp_rows[1]);
    }
}

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

    // CPU
    let cpu_label = format!("CPU {:.0}%", m.cpu_avg_pct);
    let cpu_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" CPU "))
        .gauge_style(cpu_gauge_style(m.cpu_avg_pct))
        .ratio((m.cpu_avg_pct / 100.0).clamp(0.0, 1.0) as f64)
        .label(cpu_label);
    frame.render_widget(cpu_gauge, cols[0]);

    // RAM
    let ram_pct = if m.mem_total > 0 {
        m.mem_used as f64 / m.mem_total as f64
    } else {
        0.0
    };
    let ram_gb_used = m.mem_used as f64 / 1_073_741_824.0;
    let ram_gb_total = m.mem_total as f64 / 1_073_741_824.0;
    let ram_label = format!("{:.1}/{:.0} GB", ram_gb_used, ram_gb_total);
    let ram_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" RAM "))
        .gauge_style(Style::default().fg(Color::Blue))
        .ratio(ram_pct.clamp(0.0, 1.0))
        .label(ram_label);
    frame.render_widget(ram_gauge, cols[1]);

    // Disk /
    let root_disk = m.disks.iter().find(|d| d.mount == "/");
    let (disk_pct, disk_label) = if let Some(d) = root_disk {
        (
            d.used_pct() as f64 / 100.0,
            format!("/ {:.0}%", d.used_pct()),
        )
    } else {
        (0.0, "/ —".to_string())
    };
    let disk_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Disk / "))
        .gauge_style(Style::default().fg(Color::Magenta))
        .ratio(disk_pct.clamp(0.0, 1.0))
        .label(disk_label);
    frame.render_widget(disk_gauge, cols[2]);

    // Fan
    let fan_label = if m.fan_rpm > 0 {
        format!("{} RPM", m.fan_rpm)
    } else {
        "— RPM".to_string()
    };
    let fan_para = Paragraph::new(Line::from(vec![
        Span::styled("  ", Style::default()),
        Span::styled(
            fan_label,
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ]))
    .block(Block::default().borders(Borders::ALL).title(" Fan "));
    frame.render_widget(fan_para, cols[3]);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

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
    if t >= 80.0 {
        "hot"
    } else if t >= 65.0 {
        "warm"
    } else if t >= 50.0 {
        "moderate"
    } else {
        "cool"
    }
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
    if t >= 80.0 {
        Color::Red
    } else if t >= 65.0 {
        Color::Yellow
    } else {
        Color::Green
    }
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
