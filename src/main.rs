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
use futures::StreamExt;
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

    info!("Starting Tunotron TUI Music Player");

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

    // 3. Channels for cross-thread communication
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let (cmd_tx, cmd_rx) = mpsc::unbounded_channel::<MpvCommand>();

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
    let mut app = AppState::new(music_dir, cmd_tx);
    let keymap = KeyMap::default();
    let mut key_state_machine = KeySequenceStateMachine::new();
    let theme = Theme::catppuccin_mocha();

    // 7. Event Loop Setup
    let mut reader = EventStream::new();
    let mut ticker = interval(Duration::from_millis(250));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut should_render = true;

    // Initial draw
    terminal.draw(|f| render_app(f, &mut app, &theme))?;

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
                    Some(Ok(CrosstermEvent::Key(key))) => {
                        if key.kind == KeyEventKind::Press {
                            let chord = KeyChord::from(key);
                            if let Some(action) = key_state_machine.feed(chord, &keymap) {
                                app.handle_action(action);
                                should_render = true;
                            }
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
                                let p_rect = app.progress_rect;
                                if mouse.column >= p_rect.x
                                    && mouse.column < p_rect.x + p_rect.width
                                    && mouse.row >= p_rect.y
                                    && mouse.row < p_rect.y + p_rect.height
                                {
                                    let relative_x = mouse.column.saturating_sub(p_rect.x) as f64;
                                    let ratio = relative_x / (p_rect.width.max(1) as f64);
                                    app.handle_action(Action::SeekRatio(ratio));
                                    should_render = true;
                                } else {
                                    let b_rect = app.browser_rect;
                                    let header_offset = if app.density == app::ViewDensity::Compact { 1 } else { 2 };
                                    if mouse.column >= b_rect.x
                                        && mouse.column < b_rect.x + b_rect.width
                                        && mouse.row >= b_rect.y + header_offset
                                        && mouse.row < b_rect.y + b_rect.height.saturating_sub(1)
                                    {
                                        let clicked_row = (mouse.row - b_rect.y - header_offset) as usize;
                                        app.handle_action(Action::SelectIndex(clicked_row));
                                        should_render = true;
                                    }
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

            // Branch 2: Domain Events (mpv IPC events)
            Some(domain_event) = event_rx.recv() => {
                match domain_event {
                    AppEvent::Mpv(mpv_ev) => {
                        app.handle_mpv_event(mpv_ev);
                        should_render = true;
                    }
                    _ => {}
                }

                // Batch coalescing: drain pending domain events
                while let Ok(pending) = event_rx.try_recv() {
                    match pending {
                        AppEvent::Mpv(mpv_ev) => app.handle_mpv_event(mpv_ev),
                        _ => {}
                    }
                }
            }

            // Branch 3: Guarded Seekbar Timer (Deactivated when paused or stopped -> 0.0% CPU)
            _ = ticker.tick(), if app.is_playing() => {
                should_render = true;
            }
        }
    }

    info!("Tunotron exiting gracefully");
    Ok(())
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home/arcioth"))
}
