mod api;
mod app;
mod config;
mod keybinds;
mod models;
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
    let mut app = App::new(config, api_tx);
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
