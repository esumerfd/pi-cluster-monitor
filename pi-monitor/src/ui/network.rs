use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
};

use crate::app::{NetworkState, ReachStatus, SystemMetrics};

pub fn render(frame: &mut Frame, area: Rect, net: &NetworkState, sys: &SystemMetrics) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // local summary
            Constraint::Min(0),    // node table
        ])
        .split(area);

    render_local_summary(frame, chunks[0], net, sys);
    render_nodes(frame, chunks[1], net);
}

fn render_local_summary(
    frame: &mut Frame,
    area: Rect,
    net: &NetworkState,
    sys: &SystemMetrics,
) {
    let hostname = if sys.hostname.is_empty() {
        "—".to_string()
    } else {
        sys.hostname.clone()
    };
    let ip = if net.local_ip.is_empty() {
        "—".to_string()
    } else {
        net.local_ip.clone()
    };

    let line = Line::from(vec![
        Span::styled("  Hostname: ", Style::default().fg(Color::DarkGray)),
        Span::styled(hostname, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("    IP: ", Style::default().fg(Color::DarkGray)),
        Span::styled(ip, Style::default().fg(Color::White)),
    ]);

    let para = Paragraph::new(line).block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(
                " Local ",
                Style::default().add_modifier(Modifier::BOLD),
            )),
    );
    frame.render_widget(para, area);
}

fn render_nodes(frame: &mut Frame, area: Rect, net: &NetworkState) {
    let title = if net.inventory_path.is_empty() {
        " Cluster Nodes ".to_string()
    } else {
        format!(" Cluster Nodes — {} ", net.inventory_path)
    };

    if net.nodes.is_empty() {
        let msg = if net.inventory_path.is_empty() {
            "  No inventory loaded.  Run with --inventory <path> to monitor cluster nodes."
        } else {
            "  Inventory loaded but contains no hosts."
        };
        let para = Paragraph::new(Line::from(Span::styled(
            msg,
            Style::default().fg(Color::DarkGray),
        )))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(title, Style::default().add_modifier(Modifier::BOLD))),
        );
        frame.render_widget(para, area);
        return;
    }

    let header = Row::new(vec!["Name", "Host", "Groups", "Status", "IP", "Latency"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = net
        .nodes
        .iter()
        .map(|ns| {
            let groups = if ns.node.groups.is_empty() {
                "—".to_string()
            } else {
                ns.node.groups.join(", ")
            };

            let (status_cell, ip_cell, lat_cell) = match &ns.status {
                ReachStatus::Unknown => (
                    Cell::from(Span::styled("● checking", Style::default().fg(Color::DarkGray))),
                    Cell::from("—"),
                    Cell::from("—"),
                ),
                ReachStatus::Up { ip, latency_ms } => (
                    Cell::from(Span::styled("● UP", Style::default().fg(Color::Green))),
                    Cell::from(ip.clone()),
                    Cell::from(format!("{} ms", latency_ms)),
                ),
                ReachStatus::Down => (
                    Cell::from(Span::styled("○ DOWN", Style::default().fg(Color::Red))),
                    Cell::from("—"),
                    Cell::from("—"),
                ),
            };

            Row::new(vec![
                Cell::from(ns.node.name.clone()),
                Cell::from(ns.node.ansible_host.clone()),
                Cell::from(groups),
                status_cell,
                ip_cell,
                lat_cell,
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12), // Name
            Constraint::Length(18), // Host
            Constraint::Length(14), // Groups
            Constraint::Length(12), // Status
            Constraint::Length(16), // IP
            Constraint::Length(10), // Latency
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(title, Style::default().add_modifier(Modifier::BOLD))),
    )
    .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    frame.render_widget(table, area);
}
