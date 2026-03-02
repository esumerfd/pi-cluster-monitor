mod network;
mod overview;
mod processes;
mod system;
mod stubs;
pub mod statusbar;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Tabs},
};

use crate::app::{App, Tab};

pub fn render(frame: &mut Frame, app: &App) {
    let state = app.system_snapshot();
    let net_state = app.network_snapshot();
    let proc_state = app.process_snapshot();
    let hailo_state = app.hailo_snapshot();
    let alerts = app.alert_count();

    let visible = Tab::visible(state.hailo_available);

    let tab_titles: Vec<Line> = visible
        .iter()
        .enumerate()
        .map(|(i, t)| {
            Line::from(vec![
                Span::styled(
                    format!("[{}]", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(t.title()),
            ])
        })
        .collect();

    let active_index = visible
        .iter()
        .position(|t| *t == app.active_tab)
        .unwrap_or(0);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // tab bar
            Constraint::Min(0),     // content
            Constraint::Length(1),  // status bar
        ])
        .split(frame.area());

    // ── Tab bar ───────────────────────────────────────────────────────────────
    let tab_bar = Tabs::new(tab_titles)
        .select(active_index)
        .block(Block::default().borders(Borders::BOTTOM))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw(" "));
    frame.render_widget(tab_bar, chunks[0]);

    // ── Content ───────────────────────────────────────────────────────────────
    match app.active_tab {
        Tab::Overview => overview::render(frame, chunks[1], &state, &hailo_state),
        Tab::System => system::render(frame, chunks[1], &state),
        Tab::Network => network::render(frame, chunks[1], &net_state, &state),
        Tab::Processes => processes::render(frame, chunks[1], &proc_state),
        Tab::Services => stubs::render_stub(frame, chunks[1], "Services", "[5]"),
        Tab::Hardware => stubs::render_stub(frame, chunks[1], "Hardware", "[6]"),
        Tab::Logs => stubs::render_stub(frame, chunks[1], "Logs", "[7]"),
        Tab::Npu => stubs::render_stub(frame, chunks[1], "NPU", "[8]"),
    }

    // ── Status bar ────────────────────────────────────────────────────────────
    statusbar::render(frame, chunks[2], &state, alerts);

    // ── Help overlay ─────────────────────────────────────────────────────────
    if app.show_help {
        render_help(frame);
    }
}

fn render_help(frame: &mut Frame) {
    use ratatui::{
        layout::Rect,
        widgets::{Clear, Paragraph, Wrap},
    };

    let area = frame.area();
    let width = (area.width * 60 / 100).max(40).min(area.width);
    let height = 16u16.min(area.height);
    let x = (area.width.saturating_sub(width)) / 2;
    let y = (area.height.saturating_sub(height)) / 2;
    let popup = Rect::new(x, y, width, height);

    frame.render_widget(Clear, popup);

    let help_text = vec![
        Line::from(Span::styled(
            " Keyboard Shortcuts ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("  [1-8]   Switch tabs"),
        Line::from("  Tab     Next tab"),
        Line::from("  ←/→     Previous/Next tab"),
        Line::from("  q       Quit"),
        Line::from("  ?       Toggle this help"),
        Line::from("  Esc     Close help / cancel"),
        Line::from(""),
        Line::from(Span::styled(
            "  Tabs",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("  1 Overview   2 System    3 Network"),
        Line::from("  4 Processes  5 Services  6 Hardware"),
        Line::from("  7 Logs       8 NPU (when Hailo detected)"),
        Line::from(""),
        Line::from(Span::styled(
            "  [Esc] to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let para = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });
    frame.render_widget(para, popup);
}
