use std::time::{Duration, Instant};
use std::{io, sync::Arc};

use anyhow::Result;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, RwLock};

mod model;
mod parse;
mod theme;
mod ui;
mod ws_client;

use model::{AppEvent, AppState, WS_URL_DEFAULT};

#[tokio::main]
async fn main() -> Result<()> {
    // Shared app state
    let state = Arc::new(RwLock::new(AppState::default()));

    // WS event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn WS client task (auto-connect and subscribe)
    let ws_url = WS_URL_DEFAULT.to_string();
    tokio::spawn(async move { ws_client::run(ws_url, tx).await });

    // TUI init
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // App loop
    let tick = Duration::from_millis(100);
    let mut last_draw = Instant::now();
    let mut running = true;

    while running {
        // Drain any incoming WS events into state
        while let Ok(evt) = rx.try_recv() {
            let mut s = state.write().await;
            s.apply(evt);
        }

        // Draw at most every tick interval or immediately on first loop
        if last_draw.elapsed() >= tick {
            let s = state.read().await.clone_snapshot();
            terminal.draw(|f| ui::draw(f, &s))?;
            last_draw = Instant::now();
        }

        // Non-blocking input with small timeout so we keep redrawing
        if event::poll(Duration::from_millis(10))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => running = false,
                    KeyCode::Char('g') => {
                        let mut s = state.write().await;
                        s.gradient_on = !s.gradient_on;
                    }
                    // Future: in-TUI controls (e.g., toggle bars, adjust top-N)
                    _ => {}
                }
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}
