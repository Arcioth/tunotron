mod action;
mod app;
mod audio;
mod event;
mod keymap;
mod library;
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

use action::Action;
use app::AppState;
use audio::{run_mpv_actor, MpvCommand, MpvSupervisor};
use event::AppEvent;
use keymap::{KeyChord, KeyMap, KeySequenceStateMachine};
use terminal::TerminalHarness;
use ui::{render_app, Theme};

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

    // 6. Initialize Application State & Keymap
    let mut app = AppState::new(music_dir, cmd_tx, event_tx.clone());
    let keymap = KeyMap::default();
    let mut key_state_machine = KeySequenceStateMachine::new();
    let theme = Theme::catppuccin_mocha();

    // 7. Event Loop Setup
    let mut reader = EventStream::new();
    let mut display_ticker = interval(Duration::from_millis(250));
    display_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut resync_ticker = interval(Duration::from_secs(5));
    resync_ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    // Initial draw
    terminal.draw(|f| render_app(f, &mut app, &theme))?;
    let mut should_render = false;

    // 8. Central Reactive Event Loop (Zero CPU when idle)
    while app.is_running {
        if should_render {
            terminal.draw(|f| render_app(f, &mut app, &theme))?;
            should_render = false;
        }

        tokio::select! {
            // Branch 1: Terminal Stdin & Mouse Input
            maybe_evt = reader.next() => {
                match maybe_evt {
                    Some(Ok(CrosstermEvent::Key(key))) if key.kind == KeyEventKind::Press => {
                        let chord = KeyChord::from(key);
                        if let Some(action) = key_state_machine.feed(chord, &keymap) {
                            app.handle_action(action);
                            should_render = true;
                        }
                    }
                    Some(Ok(CrosstermEvent::Mouse(mouse))) => {
                        match mouse.kind {
                            MouseEventKind::ScrollDown => {
                                app.handle_action(Action::MoveDown(2));
                                should_render = true;
                            }
                            MouseEventKind::ScrollUp => {
                                app.handle_action(Action::MoveUp(2));
                                should_render = true;
                            }
                            MouseEventKind::Down(MouseButton::Left) => {
                                if app.show_help {
                                    // Modal eats clicks; do not hit-test widgets underneath
                                    continue;
                                }
                                let pos = Position { x: mouse.column, y: mouse.row };
                                if app.progress_rect.contains(pos) {
                                    let relative_x = mouse.column.saturating_sub(app.progress_rect.x) as f64;
                                    let denom = app.progress_rect.width.max(1).saturating_sub(1).max(1) as f64;
                                    let ratio = (relative_x / denom).clamp(0.0, 1.0);
                                    app.handle_action(Action::SeekRatio(ratio));
                                    should_render = true;
                                } else if app.browser_rows_rect.contains(pos) {
                                    let visual = (mouse.row.saturating_sub(app.browser_rows_rect.y)) as usize;
                                    let idx = app.table_state.offset() + visual;
                                    app.handle_action(Action::SelectIndex(idx));
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

            // Branch 2: Domain Events (mpv IPC events, background scanner events, async directory loading)
            Some(domain_event) = event_rx.recv() => {
                match domain_event {
                    AppEvent::Mpv(mpv_ev) => {
                        let dirty = app.handle_mpv_event(mpv_ev);
                        should_render |= dirty;
                    }
                    AppEvent::TimePos(sec) => {
                        app.clock.sync(sec);
                        app.playback.current_time_sec = sec;
                        let current_sec = sec.floor() as u64;
                        if current_sec != app.last_rendered_sec {
                            app.last_rendered_sec = current_sec;
                            app.playback.update_time_label(sec);
                            should_render = true;
                        }
                    }
                    AppEvent::DirectoryLoaded { dir, items } if dir == app.current_dir => {
                        app.set_browser_items(items);
                        should_render = true;
                    }
                    AppEvent::Scanner(event::ScannerEvent::Batch(patches)) => {
                        app.apply_metadata_patches(patches);
                        should_render = true;
                    }
                    _ => {}
                }

                // Batch coalescing: drain pending domain events
                while let Ok(pending) = event_rx.try_recv() {
                    match pending {
                        AppEvent::Mpv(mpv_ev) => {
                            let dirty = app.handle_mpv_event(mpv_ev);
                            should_render |= dirty;
                        }
                        AppEvent::TimePos(sec) => {
                            app.clock.sync(sec);
                            app.playback.current_time_sec = sec;
                            let current_sec = sec.floor() as u64;
                            if current_sec != app.last_rendered_sec {
                                app.last_rendered_sec = current_sec;
                                app.playback.update_time_label(sec);
                                should_render = true;
                            }
                        }
                        AppEvent::DirectoryLoaded { dir, items } if dir == app.current_dir => {
                            app.set_browser_items(items);
                            should_render = true;
                        }
                        AppEvent::Scanner(event::ScannerEvent::Batch(patches)) => {
                            app.apply_metadata_patches(patches);
                            should_render = true;
                        }
                        _ => {}
                    }
                }
            }

            // Branch 3: Guarded Display Cadence Ticker (Active only while playing -> 0.0% idle CPU)
            // Checks monotonic PlaybackClock; triggers screen redraw ONLY when the displayed second changes (~1 Hz)
            _ = display_ticker.tick(), if app.is_playing() => {
                let current_sec = app.clock.now().floor() as u64;
                if current_sec != app.last_rendered_sec {
                    app.last_rendered_sec = current_sec;
                    app.playback.update_time_label(app.clock.now());
                    should_render = true;
                }
            }

            // Branch 4: Periodic mpv Drift Calibration Ticker (Every 5 seconds -> 0.2 Hz IPC)
            _ = resync_ticker.tick(), if app.is_playing() => {
                let _ = app.cmd_tx.try_send(MpvCommand::GetTimePos);
            }
        }
    }

    let _ = app.cmd_tx.try_send(MpvCommand::Quit);
    info!("Tunotron exiting gracefully");
    Ok(())
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
