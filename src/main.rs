mod action;
mod app;
mod audio;
mod event;
mod keymap;
mod library;
mod plugin;
mod terminal;
mod ui;

use std::path::PathBuf;
use std::time::Duration;
use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind, MouseButton, MouseEventKind};
use futures_util::StreamExt;
use ratatui::layout::Position;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};
use tracing::info;
use tracing_subscriber::EnvFilter;

use action::{Action, Effect};
use app::AppState;
use audio::{run_mpv_actor, MpvCommand, MpvSupervisor};
use event::AppEvent;
use keymap::{KeyChord, KeyMap, KeySequenceStateMachine};
use terminal::TerminalHarness;
use ui::{render_app, Theme, UiGeom};

#[tokio::main]
async fn main() -> Result<()> {
    // 0. Quick CLI flags (--version, --help)
    if let Some(arg) = std::env::args().nth(1) {
        if arg == "--version" || arg == "-V" {
            println!("tunotron {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        if arg == "--help" || arg == "-h" {
            println!("Tunotron v{} - Minimal, zero-bloat TUI music player", env!("CARGO_PKG_VERSION"));
            println!("Usage: tunotron [MUSIC_DIRECTORY]");
            return Ok(());
        }
    }

    // 1. Setup file-based logging (Never write to stdout/stderr in a TUI app!)
    let log_dir = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs_home().join(".local/state/tunotron")
        });
    let _ = std::fs::create_dir_all(&log_dir);
    let _log_file = log_dir.join("tunotron.log");
    let crash_file = log_dir.join("tunotron-crash.log");

    let file_appender = tracing_appender::rolling::never(&log_dir, "tunotron.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(non_blocking)
        .init();

    info!("Starting Tunotron TUI Music Player v{}", env!("CARGO_PKG_VERSION"));

    // 2. Parse music directory argument or default to ~/Music or current directory
    let music_dir = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home_music = dirs_home().join("Music");
            if home_music.exists() {
                home_music
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        });

    info!("Target music directory: {}", music_dir.display());

    // 3. Channels for cross-thread communication (bounded to prevent bufferbloat)
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(64);
    let (cmd_tx, cmd_rx) = mpsc::channel::<MpvCommand>(64);

    // 4. Spawn headless mpv process and async actor
    let supervisor = MpvSupervisor::spawn().await?;
    info!("Headless mpv daemon spawned and connected via UNIX socket");
    let mpv_stream = supervisor.stream;
    let event_tx_mpv = event_tx.clone();

    tokio::spawn(async move {
        if let Err(e) = run_mpv_actor(mpv_stream, cmd_rx, event_tx_mpv).await {
            tracing::error!("mpv actor error: {}", e);
        }
    });

    // 5. Setup RAII Terminal Harness (with crash recovery)
    let mut harness = TerminalHarness::init(crash_file)?;
    let terminal = harness.terminal_mut();

    // 6. Initialize Application State, UI Geometry, Keymap & Plugins
    let mut plugin_mgr = plugin::PluginManager::new();
    let _ = plugin_mgr.register(Box::new(plugin::TrackLoggerPlugin::new()));

    let plugin_dir = plugin::default_plugin_dir();
    let user_plugins_loaded = plugin::load_plugins_from_dir(&plugin_dir, Some(&music_dir), &mut plugin_mgr);
    info!(
        "Plugins initialized: {} built-in, {} external from {}",
        plugin_mgr.len().saturating_sub(user_plugins_loaded),
        user_plugins_loaded,
        plugin_dir.display()
    );

    let (mut app, init_effects) = AppState::new(music_dir);
    execute_effects(init_effects, &cmd_tx, &event_tx, &mut plugin_mgr);
    let mut geom = UiGeom::new();
    geom.clamp_selection(app.browser_items.len());

    let mut keymap = KeyMap::default();
    keymap.register_plugin_bindings(&plugin_mgr);
    let mut key_state_machine = KeySequenceStateMachine::new();
    let theme = Theme::catppuccin_mocha();

    // 7. Event Loop Setup
    let mut reader = EventStream::new();
    let mut display_ticker = interval(Duration::from_millis(250));
    display_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut resync_ticker = interval(Duration::from_secs(5));
    resync_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Initial draw
    terminal.draw(|f| render_app(f, &app, &mut geom, &theme))?;
    let mut should_render = false;

    // 8. Central Reactive Event Loop (Zero CPU when idle)
    while app.is_running {
        if should_render {
            terminal.draw(|f| render_app(f, &app, &mut geom, &theme))?;
            should_render = false;
        }

        tokio::select! {
            // Branch 1: Terminal Stdin & Mouse Input
            maybe_evt = reader.next() => {
                match maybe_evt {
                    Some(Ok(CrosstermEvent::Key(key))) if key.kind == KeyEventKind::Press => {
                        let chord = KeyChord::from(key);
                        if let Some(action) = key_state_machine.feed(chord, &keymap) {
                            let effects = app.reduce(action, &mut geom);
                            execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                            should_render = true;
                        }
                    }
                    Some(Ok(CrosstermEvent::Mouse(mouse))) => {
                        match mouse.kind {
                            MouseEventKind::ScrollDown => {
                                let effects = app.reduce(Action::MoveDown(2), &mut geom);
                                execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                should_render = true;
                            }
                            MouseEventKind::ScrollUp => {
                                let effects = app.reduce(Action::MoveUp(2), &mut geom);
                                execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                should_render = true;
                            }
                            MouseEventKind::Down(MouseButton::Left) => {
                                let pos = Position { x: mouse.column, y: mouse.row };
                                if geom.has_window() {
                                    // Modal click handling: click outside dismisses top modal
                                    if !geom.modal_rect.contains(pos) {
                                        geom.pop_window();
                                        should_render = true;
                                    }
                                    // Shield background widgets from clicks
                                    continue;
                                }
                                if geom.progress_rect.contains(pos) {
                                    let relative_x = mouse.column.saturating_sub(geom.progress_rect.x) as f64;
                                    let denom = geom.progress_rect.width.max(1).saturating_sub(1).max(1) as f64;
                                    let ratio = (relative_x / denom).clamp(0.0, 1.0);
                                    let effects = app.reduce(Action::SeekRatio(ratio), &mut geom);
                                    execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                    should_render = true;
                                } else if geom.browser_rows_rect.contains(pos) {
                                    let visual = (mouse.row.saturating_sub(geom.browser_rows_rect.y)) as usize;
                                    let idx = geom.scroll_offset() + visual;
                                    let effects = app.reduce(Action::SelectIndex(idx), &mut geom);
                                    execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                    should_render = true;
                                }
                            }
                            _ => {}
                        }
                    }
                    Some(Ok(CrosstermEvent::Resize(_, _))) => {
                        should_render = true;
                    }
                    Some(Err(e)) => {
                        tracing::error!("Terminal input error: {}", e);
                    }
                    None => break, // EOF on stdin
                    _ => {}
                }
            }

            // Branch 2: Domain Events (mpv IPC events, background scanner events, async directory loading, plugin actions)
            Some(domain_event) = event_rx.recv() => {
                match domain_event {
                    AppEvent::Mpv(mpv_ev) => {
                        let is_file_loaded = matches!(mpv_ev, audio::MpvEvent::FileLoaded);
                        let is_idle = matches!(mpv_ev, audio::MpvEvent::Idle);
                        let (dirty, effects) = app.handle_mpv_event(mpv_ev);
                        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                        should_render |= dirty;

                        // Broadcast domain events to plugins
                        if is_file_loaded {
                            if let Some(track) = &app.playback.current_track {
                                let ev = plugin::PluginEvent::TrackChanged {
                                    track_id: track.id,
                                    title: track.title.clone(),
                                    artist: track.artist.clone(),
                                    album: track.album.clone(),
                                    duration_sec: track.duration_sec,
                                    path: track.path.clone(),
                                };
                                let plugin_actions = plugin_mgr.dispatch_event(&ev);
                                for env in plugin_actions {
                                    let effects = app.reduce(env.action, &mut geom);
                                    execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                }
                            }
                        } else if is_idle {
                            let plugin_actions = plugin_mgr.dispatch_event(&plugin::PluginEvent::PlaybackStopped);
                            for env in plugin_actions {
                                let effects = app.reduce(env.action, &mut geom);
                                execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                            }
                        }
                    }
                    AppEvent::TimePos(sec) => {
                        app.clock.sync(sec);
                        app.playback.current_time_sec = sec;
                    }
                    AppEvent::DirectoryLoaded { dir, items } if dir == app.current_dir => {
                        let effects = app.set_browser_items(items);
                        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                        geom.clamp_selection(app.browser_items.len());
                        should_render = true;
                    }
                    AppEvent::Scanner(event::ScannerEvent::Batch(patches)) => {
                        app.apply_metadata_patches(patches);
                        should_render = true;
                    }
                    AppEvent::Action(env) => {
                        let effects = app.reduce(env.action, &mut geom);
                        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                        should_render = true;
                    }
                    _ => {}
                }

                // Batch coalescing: drain pending domain events
                while let Ok(pending) = event_rx.try_recv() {
                    match pending {
                        AppEvent::Mpv(mpv_ev) => {
                            let is_file_loaded = matches!(mpv_ev, audio::MpvEvent::FileLoaded);
                            let is_idle = matches!(mpv_ev, audio::MpvEvent::Idle);
                            let (dirty, effects) = app.handle_mpv_event(mpv_ev);
                            execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                            should_render |= dirty;

                            if is_file_loaded {
                                if let Some(track) = &app.playback.current_track {
                                    let ev = plugin::PluginEvent::TrackChanged {
                                        track_id: track.id,
                                        title: track.title.clone(),
                                        artist: track.artist.clone(),
                                        album: track.album.clone(),
                                        duration_sec: track.duration_sec,
                                        path: track.path.clone(),
                                    };
                                    let plugin_actions = plugin_mgr.dispatch_event(&ev);
                                    for env in plugin_actions {
                                        let effects = app.reduce(env.action, &mut geom);
                                        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                    }
                                }
                            } else if is_idle {
                                let plugin_actions = plugin_mgr.dispatch_event(&plugin::PluginEvent::PlaybackStopped);
                                for env in plugin_actions {
                                    let effects = app.reduce(env.action, &mut geom);
                                    execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                }
                            }
                        }
                        AppEvent::TimePos(sec) => {
                            app.clock.sync(sec);
                            app.playback.current_time_sec = sec;
                        }
                        AppEvent::DirectoryLoaded { dir, items } if dir == app.current_dir => {
                            let effects = app.set_browser_items(items);
                            execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                            geom.clamp_selection(app.browser_items.len());
                            should_render = true;
                        }
                        AppEvent::Scanner(event::ScannerEvent::Batch(patches)) => {
                            app.apply_metadata_patches(patches);
                            should_render = true;
                        }
                        AppEvent::Action(env) => {
                            let effects = app.reduce(env.action, &mut geom);
                            execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                            should_render = true;
                        }
                        _ => {}
                    }
                }
            }

            // Branch 3: Guarded Display Cadence Ticker (Active only while playing -> 0.0% idle CPU)
            // Checks monotonic PlaybackClock; triggers screen redraw & plugin tick ONLY when displayed second changes (~1 Hz)
            _ = display_ticker.tick(), if app.is_playing() => {
                let pos = app.clock.now();
                let current_sec = pos.floor() as u64;
                if current_sec != app.last_rendered_sec {
                    app.last_rendered_sec = current_sec;
                    app.playback.update_time_label(pos);
                    should_render = true;

                    let tick_ev = plugin::PluginEvent::Tick {
                        position: pos,
                        duration: app.playback.duration_sec,
                    };
                    let envelopes = plugin_mgr.dispatch_event(&tick_ev);
                    for env in envelopes {
                        let effects = app.reduce(env.action, &mut geom);
                        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                    }
                }
            }

            // Branch 4: Periodic mpv Drift Calibration Ticker (Every 5 seconds -> 0.2 Hz IPC)
            _ = resync_ticker.tick(), if app.is_playing() => {
                let _ = cmd_tx.try_send(MpvCommand::GetTimePos);
            }
        }
    }

    let _ = cmd_tx.try_send(MpvCommand::Quit);
    info!("Tunotron exiting gracefully");
    Ok(())
}

fn execute_effects(
    effects: Vec<Effect>,
    cmd_tx: &mpsc::Sender<MpvCommand>,
    event_tx: &mpsc::Sender<AppEvent>,
    plugin_mgr: &mut plugin::PluginManager,
) {
    for effect in effects {
        match effect {
            Effect::Mpv(cmd) => {
                let _ = cmd_tx.try_send(cmd);
            }
            Effect::LoadDirectory { dir, root } => {
                let tx = event_tx.clone();
                tokio::task::spawn_blocking(move || {
                    let items = library::read_directory(&dir, &root);
                    let _ = tx.blocking_send(AppEvent::DirectoryLoaded { dir, items });
                });
            }
            Effect::ScanMetadata(paths) => {
                library::Scanner::scan_paths_in_background(paths, event_tx.clone());
            }
            Effect::PluginAction { plugin_id, name, payload } => {
                let envelopes = plugin_mgr.dispatch_action(&plugin_id, &name, &payload);
                for env in envelopes {
                    if let Err(e) = event_tx.try_send(AppEvent::Action(env)) {
                        tracing::warn!("Domain event channel full, plugin action dropped: {}", e);
                    }
                }
            }
            Effect::Notify { summary, body } => {
                tokio::spawn(async move {
                    let res = tokio::process::Command::new("notify-send")
                        .arg("--app-name=Tunotron")
                        .arg(&summary)
                        .arg(&body)
                        .spawn();
                    if let Err(e) = res {
                        tracing::warn!("Failed to dispatch desktop notification: {}", e);
                    }
                });
            }
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
