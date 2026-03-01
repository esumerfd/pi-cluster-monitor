use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::app::{NodeProcesses, ProcessState};

pub fn render(frame: &mut Frame, area: Rect, state: &ProcessState) {
    if state.nodes.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "  No inventory loaded — run with --inventory <path> to monitor cluster processes.",
            Style::default().fg(Color::DarkGray),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(
                    " Processes ",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
        );
        frame.render_widget(para, area);
        return;
    }

    let n = state.nodes.len();
    let constraints: Vec<Constraint> = (0..n)
        .map(|_| Constraint::Ratio(1, n as u32))
        .collect();

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    for (i, node) in state.nodes.iter().enumerate() {
        render_node_panel(frame, columns[i], node);
    }
}

fn render_node_panel(frame: &mut Frame, area: Rect, node: &NodeProcesses) {
    let title = format!(" {} ({}) ", node.node_name, node.ansible_host);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, Style::default().add_modifier(Modifier::BOLD)));

    if let Some(err) = &node.error {
        let para = Paragraph::new(Line::from(Span::styled(
            format!("  {}", err),
            Style::default().fg(Color::DarkGray),
        )))
        .block(block);
        frame.render_widget(para, area);
        return;
    }

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if node.processes.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  no processes",
                Style::default().fg(Color::DarkGray),
            ))),
            inner,
        );
        return;
    }

    // Split inner: header hint at top, table fills rest
    let rows_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let header = Row::new(vec!["PID", "User", "CPU%", "MEM%", "Command"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = node
        .processes
        .iter()
        .map(|p| {
            let cpu_style = if p.cpu_pct >= 50.0 {
                Style::default().fg(Color::Red)
            } else if p.cpu_pct >= 20.0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };

            // Truncate command to fit the column
            let cmd = truncate_cmd(&p.command, 22);

            Row::new(vec![
                Cell::from(p.pid.to_string()),
                Cell::from(p.user.clone()),
                Cell::from(format!("{:.1}", p.cpu_pct)).style(cpu_style),
                Cell::from(format!("{:.1}", p.mem_pct)),
                Cell::from(cmd),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(6),  // PID
            Constraint::Length(8),  // User
            Constraint::Length(5),  // CPU%
            Constraint::Length(5),  // MEM%
            Constraint::Min(0),     // Command (fills remaining)
        ],
    )
    .header(header);

    frame.render_widget(table, rows_layout[0]);

    // Bottom hint
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  top 5 by CPU% · raspi-dash",
            Style::default().fg(Color::DarkGray),
        ))),
        rows_layout[1],
    );
}

/// Shorten a command string: use only the basename of the executable
/// followed by the first argument, then "…" if truncated.
fn truncate_cmd(cmd: &str, max_chars: usize) -> String {
    let cmd = cmd.trim();
    if cmd.len() <= max_chars {
        return cmd.to_string();
    }
    // Try to shorten: take basename of first token
    let mut parts = cmd.splitn(3, ' ');
    let exe = parts.next().unwrap_or(cmd);
    let exe_base = exe.rsplit('/').next().unwrap_or(exe);
    let rest = parts.next().unwrap_or("");
    let short = if rest.is_empty() {
        exe_base.to_string()
    } else {
        format!("{} {}", exe_base, rest)
    };
    if short.len() <= max_chars {
        short
    } else {
        format!("{}…", &short[..max_chars.saturating_sub(1)])
    }
}
