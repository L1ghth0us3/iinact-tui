use std::env;
use std::fs::{create_dir_all, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use std::{io, sync::Arc};

use anyhow::{bail, Context, Result};
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEvent,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use tokio::sync::{mpsc, RwLock};
use tokio::task;

mod config;
mod history;
mod model;
mod parse;
mod theme;
mod ui;
mod ui_history;
mod ui_idle;
mod ws_client;

use history::HistoryStore;
use model::{AppEvent, AppSettings, AppState, HistoryPanelLevel, SettingsField, WS_URL_DEFAULT};
use tracing::level_filters::LevelFilter;

const HISTORY_LIST_OFFSET: u16 = 4;

enum HistoryTask {
    LoadEncounters { date_id: String },
    LoadEncounterDetail { key: Vec<u8> },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = parse_cli()?;
    init_tracing(&cli)?;

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
    let event_tx = tx.clone();

    // Spawn WS client task (auto-connect and subscribe)
    let ws_url = WS_URL_DEFAULT.to_string();
    let history_tx = history_recorder.clone();
    let ws_tx = tx.clone();
    tokio::spawn(async move { ws_client::run(ws_url, ws_tx, history_tx).await });

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
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        let mut s = state.write().await;
                        if s.history.visible {
                            s.history.visible = false;
                            s.history.reset();
                        } else {
                            running = false;
                        }
                    }
                    KeyCode::Char('h') => {
                        let should_load = {
                            let mut s = state.write().await;
                            if s.toggle_history() {
                                s.history_set_loading();
                                true
                            } else {
                                false
                            }
                        };
                        if should_load {
                            let store = history_store.clone();
                            let tx = event_tx.clone();
                            tokio::spawn(async move {
                                match task::spawn_blocking(move || store.load_dates()).await {
                                    Ok(Ok(days)) => {
                                        let _ = tx.send(AppEvent::HistoryDatesLoaded { days });
                                    }
                                    Ok(Err(err)) => {
                                        let _ = tx.send(AppEvent::HistoryError {
                                            message: err.to_string(),
                                        });
                                    }
                                    Err(err) => {
                                        let _ = tx.send(AppEvent::HistoryError {
                                            message: format!("History load failed: {err}"),
                                        });
                                    }
                                }
                            });
                        }
                    }
                    KeyCode::Char('i') => {
                        let mut s = state.write().await;
                        if !s.history.visible {
                            let now = Instant::now();
                            if s.is_idle_at(now) {
                                s.show_idle_overlay = !s.show_idle_overlay;
                            }
                        }
                    }
                    _ => {
                        let mut pending_task = None;
                        let history_active = {
                            let mut s = state.write().await;
                            if s.history.visible {
                                match key.code {
                                    KeyCode::Up => s.history_move_selection(-1),
                                    KeyCode::Down => s.history_move_selection(1),
                                    KeyCode::PageUp => s.history_move_selection(-5),
                                    KeyCode::PageDown => s.history_move_selection(5),
                                    KeyCode::Left | KeyCode::Backspace => s.history_back(),
                                    KeyCode::Right | KeyCode::Enter => s.history_enter(),
                                    _ => {}
                                }
                                pending_task = determine_history_task(&mut s);
                                true
                            } else {
                                false
                            }
                        };

                        if let Some(task) = pending_task {
                            spawn_history_task(task, history_store.clone(), event_tx.clone());
                        }

                        if history_active {
                            continue;
                        }

                        match key.code {
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
                },
                Event::Mouse(mouse) => {
                    handle_history_mouse(mouse, &state).await;
                    let mut s = state.write().await;
                    if s.history.visible {
                        if let Some(task) = determine_history_task(&mut s) {
                            spawn_history_task(task, history_store.clone(), event_tx.clone());
                        }
                    }
                }
                _ => {}
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
    history_recorder.shutdown().await;
    Ok(())
}

#[derive(Debug, Default)]
struct CliArgs {
    debug: Option<DebugTarget>,
}

#[derive(Debug)]
enum DebugTarget {
    Default,
    Path(PathBuf),
}

fn parse_cli() -> Result<CliArgs> {
    let mut args = env::args().skip(1).peekable();
    let mut debug = None;

    while let Some(arg) = args.next() {
        if arg == "--debug" {
            if debug.is_some() {
                bail!("`--debug` specified more than once");
            }
            if let Some(next) = args.peek() {
                if !next.starts_with('-') {
                    let path = args
                        .next()
                        .map(PathBuf::from)
                        .expect("peek ensured next exists");
                    debug = Some(DebugTarget::Path(path));
                    continue;
                }
            }
            debug = Some(DebugTarget::Default);
        } else if let Some(rest) = arg.strip_prefix("--debug=") {
            if debug.is_some() {
                bail!("`--debug` specified more than once");
            }
            if rest.is_empty() {
                debug = Some(DebugTarget::Default);
            } else {
                debug = Some(DebugTarget::Path(PathBuf::from(rest)));
            }
        } else {
            bail!("unknown argument: {arg}");
        }
    }

    Ok(CliArgs { debug })
}

fn init_tracing(cli: &CliArgs) -> Result<()> {
    if let Some(target) = &cli.debug {
        let log_path = match target {
            DebugTarget::Default => config::config_dir().join("debug.log"),
            DebugTarget::Path(path) => path.clone(),
        };

        if let Some(parent) = log_path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent).with_context(|| {
                    format!("failed to create log directory {}", parent.display())
                })?;
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .write(true)
            .open(&log_path)
            .with_context(|| format!("failed to open log file {}", log_path.display()))?;

        let subscriber = tracing_subscriber::fmt()
            .with_writer(move || file.try_clone().expect("failed to clone log file handle"))
            .with_ansi(false)
            .with_target(false)
            .with_max_level(LevelFilter::DEBUG);

        subscriber.try_init().map_err(|err| {
            anyhow::anyhow!(
                "failed to initialize logging to {}: {}",
                log_path.display(),
                err
            )
        })?;
    }

    Ok(())
}

async fn handle_history_mouse(mouse: MouseEvent, state: &Arc<RwLock<AppState>>) {
    let mut s = state.write().await;
    if !s.history.visible || s.history.loading {
        return;
    }

    match mouse.kind {
        MouseEventKind::ScrollDown => s.history_move_selection(1),
        MouseEventKind::ScrollUp => s.history_move_selection(-1),
        MouseEventKind::Down(MouseButton::Left) => {
            let index = mouse.row.saturating_sub(HISTORY_LIST_OFFSET) as usize;
            match s.history.level {
                HistoryPanelLevel::Dates => {
                    if !s.history.days.is_empty() {
                        let max_index = s.history.days.len().saturating_sub(1);
                        s.history.selected_day = index.min(max_index);
                    }
                    s.history_enter();
                }
                HistoryPanelLevel::Encounters => {
                    if let Some(day) = s.history.current_day() {
                        if !day.encounters.is_empty() {
                            let max_index = day.encounters.len().saturating_sub(1);
                            s.history.selected_encounter = index.min(max_index);
                            s.history_enter();
                        }
                    }
                }
                HistoryPanelLevel::EncounterDetail => {}
            }
        }
        _ => {}
    }
}

fn determine_history_task(state: &mut AppState) -> Option<HistoryTask> {
    if state.history.loading {
        return None;
    }

    match state.history.level {
        HistoryPanelLevel::Encounters => {
            let need_load = state
                .history
                .current_day()
                .filter(|day| !day.encounters_loaded)
                .and_then(|day| {
                    if day.encounter_ids.is_empty() {
                        None
                    } else {
                        Some(day.iso_date.clone())
                    }
                });
            if let Some(date_id) = need_load {
                state.history_set_loading();
                return Some(HistoryTask::LoadEncounters { date_id });
            }
        }
        HistoryPanelLevel::EncounterDetail => {
            let need_load = state
                .history
                .current_encounter()
                .filter(|enc| enc.record.is_none())
                .map(|enc| enc.key.clone());
            if let Some(key) = need_load {
                state.history_set_loading();
                return Some(HistoryTask::LoadEncounterDetail { key });
            }
        }
        HistoryPanelLevel::Dates => {}
    }

    None
}

fn spawn_history_task(
    task: HistoryTask,
    store: Arc<HistoryStore>,
    tx: mpsc::UnboundedSender<AppEvent>,
) {
    match task {
        HistoryTask::LoadEncounters { date_id } => {
            let tx_enc = tx.clone();
            let store_clone = store.clone();
            tokio::spawn(async move {
                let date_for_block = date_id.clone();
                let result = task::spawn_blocking(move || {
                    store_clone.load_encounter_summaries(&date_for_block)
                })
                .await;
                match result {
                    Ok(Ok(encounters)) => {
                        let _ = tx_enc.send(AppEvent::HistoryEncountersLoaded {
                            date_id,
                            encounters,
                        });
                    }
                    Ok(Err(err)) => {
                        let _ = tx_enc.send(AppEvent::HistoryError {
                            message: err.to_string(),
                        });
                    }
                    Err(err) => {
                        let _ = tx_enc.send(AppEvent::HistoryError {
                            message: format!("History load failed: {err}"),
                        });
                    }
                }
            });
        }
        HistoryTask::LoadEncounterDetail { key } => {
            let tx_detail = tx.clone();
            let store_clone = store.clone();
            tokio::spawn(async move {
                let key_for_block = key.clone();
                let result =
                    task::spawn_blocking(move || store_clone.load_encounter_record(&key_for_block))
                        .await;
                match result {
                    Ok(Ok(record)) => {
                        let _ = tx_detail.send(AppEvent::HistoryEncounterLoaded { key, record });
                    }
                    Ok(Err(err)) => {
                        let _ = tx_detail.send(AppEvent::HistoryError {
                            message: err.to_string(),
                        });
                    }
                    Err(err) => {
                        let _ = tx_detail.send(AppEvent::HistoryError {
                            message: format!("History load failed: {err}"),
                        });
                    }
                }
            });
        }
    }
}
