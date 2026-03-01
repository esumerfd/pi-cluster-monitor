mod app;
mod collector;
mod inventory;
mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;
use collector::start_collectors;

#[derive(Parser, Debug)]
#[command(name = "pi-monitor", about = "Raspberry Pi cluster TUI monitor")]
struct Args {
    /// Path to an Ansible inventory YAML file
    #[arg(short, long, value_name = "FILE")]
    inventory: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Set up tracing to a file so it doesn't pollute the TUI
    {
        use tracing_subscriber::prelude::*;
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(|| {
                        std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("/tmp/pi-monitor.log")
                            .expect("cannot open log file")
                    })
                    .with_ansi(false),
            )
            .init();
    }

    // Load inventory (optional)
    let (inventory_nodes, inventory_path) = match &args.inventory {
        Some(path) => {
            let nodes = inventory::parse(path).unwrap_or_else(|e| {
                eprintln!("Warning: could not load inventory {}: {}", path.display(), e);
                vec![]
            });
            let path_str = path.display().to_string();
            (nodes, path_str)
        }
        None => (vec![], String::new()),
    };

    let mut app = App::new();
    let state = app.state.clone();

    // Start background collectors
    start_collectors(state, inventory_nodes, inventory_path).await;

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Main render + event loop (~10 Hz)
    let tick = Duration::from_millis(100);

    while app.running {
        terminal.draw(|frame| ui::render(frame, &app))?;

        if event::poll(tick)? {
            match event::read()? {
                Event::Key(key) => {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('c')
                    {
                        app.running = false;
                        continue;
                    }
                    match key.code {
                        KeyCode::Char(c) => app.handle_key(c)?,
                        KeyCode::Tab => app.next_tab(),
                        KeyCode::BackTab => app.prev_tab(),
                        KeyCode::Left => app.prev_tab(),
                        KeyCode::Right => app.next_tab(),
                        KeyCode::Esc => {
                            if app.show_help {
                                app.show_help = false;
                            }
                        }
                        _ => {}
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}
