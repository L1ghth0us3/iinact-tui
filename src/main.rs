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

mod config;
mod history;
mod model;
mod parse;
mod theme;
mod ui;
mod ui_idle;
mod ws_client;

use model::{AppEvent, AppSettings, AppState, SettingsField, WS_URL_DEFAULT};

#[tokio::main]
async fn main() -> Result<()> {
    // Shared app state
    let state = Arc::new(RwLock::new(AppState::default()));

    // History persistence (sled-backed)
    let history_store = Arc::new(history::HistoryStore::open_default()?);
    let history_recorder = history::spawn_recorder(history_store.clone());

    // Load persisted configuration into state
    let cfg = match config::load() {
        Ok(c) => c,
        Err(err) => {
            eprintln!("Failed to load config: {err:?}. Using defaults.");
            config::AppConfig::default()
        }
    };
    {
        let mut s = state.write().await;
        s.apply_settings(AppSettings::from(cfg.clone()));
    }

    // WS event channel
    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();

    // Spawn WS client task (auto-connect and subscribe)
    let ws_url = WS_URL_DEFAULT.to_string();
    let history_tx = history_recorder.clone();
    tokio::spawn(async move { ws_client::run(ws_url, tx, history_tx).await });

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
                    KeyCode::Char('d') => {
                        let mut s = state.write().await;
                        s.decoration = s.decoration.next();
                    }
                    KeyCode::Char('m') => {
                        let mut s = state.write().await;
                        s.mode = s.mode.next();
                    }
                    KeyCode::Char('s') => {
                        let mut s = state.write().await;
                        s.show_settings = !s.show_settings;
                        if s.show_settings {
                            s.settings_cursor = SettingsField::default();
                        }
                    }
                    KeyCode::Up => {
                        let mut s = state.write().await;
                        if s.show_settings {
                            s.prev_setting();
                        }
                    }
                    KeyCode::Down => {
                        let mut s = state.write().await;
                        if s.show_settings {
                            s.next_setting();
                        }
                    }
                    KeyCode::Left | KeyCode::Right => {
                        let forward = matches!(key.code, KeyCode::Right);
                        let updated = {
                            let mut s = state.write().await;
                            if s.show_settings && s.adjust_selected_setting(forward) {
                                Some(s.settings.clone())
                            } else {
                                None
                            }
                        };
                        if let Some(settings) = updated {
                            let cfg: config::AppConfig = settings.into();
                            if let Err(err) = config::save(&cfg) {
                                eprintln!("Failed to save config: {err:?}");
                            }
                        }
                    }
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
    history_recorder.shutdown();
    Ok(())
}
