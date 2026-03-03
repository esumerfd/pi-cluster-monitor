use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, Paragraph, Row, Sparkline, Table},
};

use crate::app::SystemMetrics;

pub fn render(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),   // CPU section
            Constraint::Length(5),   // Memory + Disk
            Constraint::Length(3),   // Thermal & Fan
        ])
        .split(area);

    render_cpu(frame, rows[0], m);
    render_mem_disk(frame, rows[1], m);
    render_thermal(frame, rows[2], m);
}

// ── CPU ───────────────────────────────────────────────────────────────────────

fn render_cpu(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " CPU ",
            Style::default().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Split inner: core gauges (top) | detail line | sparkline
    let cpu_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // core gauges (two rows of 2)
            Constraint::Length(1), // detail line
            Constraint::Min(1),    // sparkline
        ])
        .split(inner);

    // Core gauges: up to 4 cores in a 2x2 grid
    let cores = &m.cpu_cores;
    let num_cores = cores.len().min(4);

    if num_cores > 0 {
        let core_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![Constraint::Percentage(50); 2])
            .split(cpu_rows[0]);

        // We'll stack 2 rows ourselves since ratatui Gauge needs its own cell
        // Pack two cores per column in a vertical split
        for col in 0..2 {
            let col_rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Length(1)])
                .split(core_cols[col]);

            for row in 0..2 {
                let core_idx = col * 2 + row;
                if core_idx < num_cores {
                    let pct = cores[core_idx].usage_pct;
                    let label = format!("Core {} {:.0}%", core_idx, pct);
                    let gauge = Gauge::default()
                        .gauge_style(cpu_style(pct))
                        .ratio((pct / 100.0).clamp(0.0, 1.0) as f64)
                        .label(label);
                    frame.render_widget(gauge, col_rows[row]);
                }
            }
        }
    }

    // Detail line
    let throttle_str = if m.throttle_flags.is_empty() {
        Span::styled("✓ OK", Style::default().fg(Color::Green))
    } else {
        Span::styled(
            format!("⚠ {}", m.throttle_flags.join(",")),
            Style::default().fg(Color::Red),
        )
    };

    let detail = Line::from(vec![
        Span::styled("  Freq: ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{} MHz  ", m.cpu_freq_mhz)),
        Span::styled("Volt: ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{:.3}V  ", m.cpu_voltage)),
        Span::styled("Load: ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!(
            "{:.2} / {:.2} / {:.2}  ",
            m.load_avg[0], m.load_avg[1], m.load_avg[2]
        )),
        Span::styled("Throttle: ", Style::default().fg(Color::DarkGray)),
        throttle_str,
        Span::styled("  GPU: V3D ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{} MHz  ", m.gpu_v3d_mhz)),
        Span::styled("HEVC ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{} MHz  ", m.gpu_hevc_mhz)),
        Span::styled("Mem ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{} MB", m.gpu_mem_mb)),
    ]);
    frame.render_widget(Paragraph::new(detail), cpu_rows[1]);

    // CPU history sparkline
    // Average history across cores
    let avg_history: Vec<u64> = {
        let len = m
            .cpu_cores
            .iter()
            .map(|c| c.history.len())
            .max()
            .unwrap_or(0);
        (0..len)
            .map(|i| {
                let vals: Vec<f32> = m
                    .cpu_cores
                    .iter()
                    .filter_map(|c| c.history.get(i))
                    .copied()
                    .collect();
                if vals.is_empty() {
                    0
                } else {
                    (vals.iter().sum::<f32>() / vals.len() as f32) as u64
                }
            })
            .collect()
    };

    let sparkline_label = Paragraph::new(Line::from(Span::styled(
        "  History (60s): ",
        Style::default().fg(Color::DarkGray),
    )));

    let spark_area = cpu_rows[2];
    if spark_area.width > 20 && spark_area.height >= 1 {
        let spark_cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(0)])
            .split(spark_area);

        frame.render_widget(sparkline_label, spark_cols[0]);

        let sparkline = Sparkline::default()
            .data(&avg_history)
            .max(100)
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(sparkline, spark_cols[1]);
    }
}

// ── Memory + Disk ─────────────────────────────────────────────────────────────

fn render_mem_disk(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_memory(frame, cols[0], m);
    render_disk(frame, cols[1], m);
}

fn render_memory(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Memory ",
            Style::default().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let ram_pct = if m.mem_total > 0 {
        m.mem_used as f64 / m.mem_total as f64
    } else {
        0.0
    };
    let ram_gb_used = m.mem_used as f64 / 1_073_741_824.0;
    let ram_gb_total = m.mem_total as f64 / 1_073_741_824.0;
    let bufcache_gb = m.mem_bufcache as f64 / 1_073_741_824.0;

    let swap_pct = if m.swap_total > 0 {
        m.swap_used as f64 / m.swap_total as f64
    } else {
        0.0
    };
    let swap_gb_used = m.swap_used as f64 / 1_073_741_824.0;
    let swap_gb_total = m.swap_total as f64 / 1_073_741_824.0;

    let mem_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    // RAM gauge
    let ram_gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Blue))
        .ratio(ram_pct.clamp(0.0, 1.0))
        .label(format!("RAM  {:.1}/{:.0} GB", ram_gb_used, ram_gb_total));
    frame.render_widget(ram_gauge, mem_rows[0]);

    // buf/cache line
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("  buf/cache: {:.1} GB", bufcache_gb),
            Style::default().fg(Color::DarkGray),
        ))),
        mem_rows[1],
    );

    // Swap gauge
    let swap_gauge = Gauge::default()
        .gauge_style(Style::default().fg(Color::Magenta))
        .ratio(swap_pct.clamp(0.0, 1.0))
        .label(format!("Swap {:.1}/{:.0} GB", swap_gb_used, swap_gb_total));
    frame.render_widget(swap_gauge, mem_rows[2]);
}

fn render_disk(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(
            " Disk ",
            Style::default().add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let disk_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    // Mount table
    let rows: Vec<Row> = m
        .disks
        .iter()
        .map(|d| {
            let pct = d.used_pct();
            let bar_full = 10;
            let filled = ((pct / 100.0) * bar_full as f32) as usize;
            let bar = format!(
                "[{}{}]",
                "█".repeat(filled),
                "░".repeat(bar_full - filled)
            );
            Row::new(vec![
                d.mount.clone(),
                d.fstype.clone(),
                bar,
                format!("{:.1} GB", d.used_gb()),
                format!("{:.1} GB", d.total_gb()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(8),
            Constraint::Length(8),
        ],
    )
    .header(
        Row::new(vec!["Mount", "FS", "Usage", "Used", "Total"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    );
    frame.render_widget(table, disk_rows[0]);

    // I/O rates
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  R: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{:.1} KB/s  ", m.disk_read_kbps)),
            Span::styled("W: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format!("{:.1} KB/s", m.disk_write_kbps)),
        ])),
        disk_rows[1],
    );
}

// ── Thermal & Fan ─────────────────────────────────────────────────────────────

fn render_thermal(frame: &mut Frame, area: Rect, m: &SystemMetrics) {
    let fan_pct = (m.fan_rpm as f64 / 5000.0).clamp(0.0, 1.0);

    let line = Line::from(vec![
        Span::styled("  CPU: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.1}°C", m.cpu_temp_c),
            temp_style(m.cpu_temp_c),
        ),
        Span::raw("  "),
        Span::styled("RP1 ADC: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if m.rp1_temp_c > 0.0 {
                format!("{:.1}°C", m.rp1_temp_c)
            } else {
                "—".to_string()
            },
            Style::default().fg(Color::Yellow),
        ),
        Span::styled("   Fan: ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            if m.fan_rpm > 0 {
                format!("{} RPM", m.fan_rpm)
            } else {
                "—".to_string()
            },
            Style::default().fg(Color::Cyan),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "[{}{}]",
                "█".repeat((fan_pct * 10.0) as usize),
                "░".repeat(10 - (fan_pct * 10.0) as usize)
            ),
            Style::default().fg(Color::Cyan),
        ),
    ]);

    let para = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                " Thermal & Fan ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(para, area);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn cpu_style(pct: f32) -> Style {
    if pct >= 90.0 {
        Style::default().fg(Color::Red)
    } else if pct >= 70.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}

fn temp_style(t: f32) -> Style {
    if t >= 80.0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if t >= 65.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}
