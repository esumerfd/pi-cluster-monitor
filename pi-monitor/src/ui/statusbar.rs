use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::SystemMetrics;

pub fn render(frame: &mut Frame, area: Rect, m: &SystemMetrics, alerts: u32) {
    let hostname = if m.hostname.is_empty() {
        "pi5".to_string()
    } else {
        m.hostname.clone()
    };

    let uptime = fmt_uptime(m.uptime_secs);
    let cpu_str = format!("{:.0}%", m.cpu_avg_pct);
    let temp_str = format!("{:.1}°C", m.cpu_temp_c);
    let fan_str = if m.fan_rpm > 0 {
        format!("fan: {} RPM", m.fan_rpm)
    } else {
        "fan: —".to_string()
    };

    let alert_span = if alerts > 0 {
        Span::styled(
            format!(" ⚠ {} alert{} ", alerts, if alerts == 1 { "" } else { "s" }),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " OK ".to_string(),
            Style::default().fg(Color::Green),
        )
    };

    let temp_style = temp_color(m.cpu_temp_c);

    let spans: Line = Line::from(vec![
        Span::styled(
            format!(" {} ", hostname),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("up {} ", uptime)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("CPU: {} ", cpu_str)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} ", temp_str), temp_style),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::raw(format!("{} ", fan_str)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        alert_span,
        Span::styled(
            "  [?]help  [q]quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let para = Paragraph::new(spans)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(para, area);
}

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

fn temp_color(temp: f32) -> Style {
    if temp >= 80.0 {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else if temp >= 70.0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    }
}
