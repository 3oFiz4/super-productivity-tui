mod api;
mod app;
mod bridge;
mod cli;
mod config;
mod keybinds;
mod models;
mod notify;
mod offline;
mod short_syntax;
mod theme;
mod ui;

use anyhow::Result;
use app::{ApiEvent, App};
use config::AppConfig;
use crossterm::event::{DisableMouseCapture, EnableMouseCapture, Event, EventStream};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    if !cli_args.is_empty() {
        let color_on = !cli_args.iter().any(|a| a == "--no-color" || a == "-C")
            && std::env::var_os("NO_COLOR").is_none();
        if let Err(err) = cli::run(cli_args).await {
            cli::print_error(&err.to_string(), color_on);
            std::process::exit(1);
        }
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run(&mut terminal).await;

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let config = AppConfig::load();
    let (api_tx, mut api_rx) = mpsc::unbounded_channel::<ApiEvent>();
    let bridge = bridge::BridgeHandle::new();
    bridge::spawn_bridge_server(config.bridge_port, bridge.clone(), api_tx.clone());
    let mut app = App::new(config, api_tx);
    app.bridge = bridge;
    app.refresh_all();

    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(50));

    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;

        if app.should_quit {
            break;
        }

        tokio::select! {
            _ = tick.tick() => {
                app.tick();
            }
            Some(api_ev) = api_rx.recv() => {
                app.handle_api_event(api_ev);
            }
            Some(Ok(ev)) = events.next() => {
                match ev {
                    Event::Key(key) => app.handle_key(key),
                    Event::Mouse(mouse) => app.handle_mouse(mouse),
                    _ => {}
                }
            }
        }
    }

    Ok(())
}
