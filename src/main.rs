mod action;
mod app;
mod audio;
mod event;
mod keymap;
mod library;
mod plugin;
mod terminal;
mod ui;
mod ipc;

use std::path::PathBuf;
use std::time::{Duration, Instant};
use anyhow::Result;
use crossterm::event::{Event as CrosstermEvent, EventStream, KeyEventKind, MouseButton, MouseEventKind};
use futures_util::StreamExt;
use ratatui::layout::Position;
use tokio::sync::mpsc;
use tokio::time::{interval, MissedTickBehavior};
use tracing::info;
use tracing_subscriber::EnvFilter;

use action::{Action, ActionEnvelope, Effect};
use app::AppState;
use audio::{run_mpv_actor, MpvCommand, MpvSupervisor};
use event::AppEvent;
use keymap::{KeyChord, KeyMap, KeySequenceStateMachine};
use terminal::TerminalHarness;
use ui::{render_app, Theme, UiGeom};

#[tokio::main]
async fn main() -> Result<()> {
    // 0. Quick CLI flags (--version, --help) and CLI controller subcommands
    let cli_args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(first) = cli_args.first() {
        if first == "--version" || first == "-V" {
            println!("tunotron {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        if first == "--help" || first == "-h" {
            println!("Tunotron v{} - Minimal, zero-bloat TUI music player & audio runtime", env!("CARGO_PKG_VERSION"));
            println!("\nUsage:");
            println!("  tunotron [MUSIC_DIRECTORY]         Launch interactive TUI player");
            println!("  tunotron status [--json] [--follow] Query playback status (Waybar/SwayNC ready)");
            println!("  tunotron play | pause | toggle     Control playback");
            println!("  tunotron next | prev | stop        Navigate playlist");
            println!("  tunotron volume <[+|-]percent>     Adjust or set volume (e.g. +5, 80)");
            println!("  tunotron seek <[+|-]seconds>       Seek relative or absolute (e.g. +10, 45)");
            println!("  tunotron loop | shuffle            Toggle loop mode or shuffle");
            println!("  tunotron toast <message>           Display in-app toast notification");
            return Ok(());
        }

        match first.as_str() {
            "status" | "play" | "pause" | "toggle" | "stop" | "next" | "prev" | "seek" | "volume" | "loop" | "shuffle" | "toast" => {
                let sub_args = &cli_args[1..];
                if let Err(e) = ipc::run_cli_command(first, sub_args).await {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
                return Ok(());
            }
            _ => {}
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

    let mut plugin_dirs = vec![plugin::default_plugin_dir()];
    let dev_plugins = PathBuf::from("plugins");
    if dev_plugins.is_dir() && !plugin_dirs.contains(&dev_plugins) {
        plugin_dirs.push(dev_plugins);
    }
    let examples_plugins = PathBuf::from("examples/plugins");
    if examples_plugins.is_dir() && !plugin_dirs.contains(&examples_plugins) {
        plugin_dirs.push(examples_plugins);
    }

    let mut user_plugins_loaded = 0;
    let mut initial_envelopes = Vec::new();
    for pdir in &plugin_dirs {
        let (count, envs) = plugin::load_plugins_from_dir(pdir, Some(&music_dir), &mut plugin_mgr);
        user_plugins_loaded += count;
        initial_envelopes.extend(envs);
    }
    info!(
        "Plugins initialized: {} built-in, {} external from {:?}",
        plugin_mgr.len().saturating_sub(user_plugins_loaded),
        user_plugins_loaded,
        plugin_dirs
    );

    let (mut app, init_effects) = AppState::new(music_dir);
    app.sync_plugins(plugin_mgr.plugin_infos());
    execute_effects(init_effects, &cmd_tx, &event_tx, &mut plugin_mgr);
    let mut geom = UiGeom::new();
    geom.clamp_selection(app.browser_items.len());

    for env in initial_envelopes {
        let effects = app.reduce(env.action, &mut geom);
        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
    }
    app.sync_plugins(plugin_mgr.plugin_infos());

    let ipc_handle = match ipc::spawn_ipc_server(event_tx.clone(), ipc::TunotronStatus::from_app(&app)) {
        Ok(h) => {
            info!("IPC socket listener active at {}", h.socket_path.display());
            Some(h)
        }
        Err(e) => {
            tracing::warn!("Failed to start IPC Unix socket: {}", e);
            None
        }
    };

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
    let mut last_click: Option<(usize, Instant)> = None;

    // 8. Central Reactive Event Loop (Zero CPU when idle)
    while app.is_running {
        if should_render {
            terminal.draw(|f| render_app(f, &app, &mut geom, &theme))?;
            if let Some(ipc) = &ipc_handle {
                ipc.update_status(ipc::TunotronStatus::from_app(&app));
            }
            should_render = false;
        }

        tokio::select! {
            // Branch 1: Terminal Stdin & Mouse Input
            maybe_evt = reader.next() => {
                match maybe_evt {
                    Some(Ok(CrosstermEvent::Key(key))) if key.kind == KeyEventKind::Press => {
                        use crossterm::event::KeyCode;

                        // Global Tab cycling
                        if key.code == KeyCode::Tab {
                            let effects = app.reduce(Action::NextTab, &mut geom);
                            execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                            should_render = true;
                            continue;
                        } else if key.code == KeyCode::BackTab {
                            let effects = app.reduce(Action::PrevTab, &mut geom);
                            execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                            should_render = true;
                            continue;
                        }

                        // If Tab 1 (Extensions Manager) is active
                        if app.active_tab == 1 && !geom.has_window() {
                            let ext_action = match key.code {
                                KeyCode::Up | KeyCode::Char('k') => Some(Action::ExtensionNavUp),
                                KeyCode::Down | KeyCode::Char('j') => Some(Action::ExtensionNavDown),
                                KeyCode::Enter | KeyCode::Char(' ') => Some(Action::ToggleSelectedPlugin),
                                KeyCode::Esc => Some(Action::SwitchTab(0)),
                                KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                                    let digit_idx = (c as usize).saturating_sub('1' as usize);
                                    Some(Action::SwitchTab(digit_idx))
                                }
                                _ => None,
                            };

                            if let Some(action) = ext_action {
                                let effects = app.reduce(action, &mut geom);
                                execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                should_render = true;
                                continue;
                            }
                        }

                        // If an extension custom page tab is active (Tab >= 2), route arrows, enter, space to form controls
                        if app.active_tab >= 2 && !geom.has_window() {
                            let form_action = match key.code {
                                KeyCode::Up => Some(Action::FormNavUp),
                                KeyCode::Down => Some(Action::FormNavDown),
                                KeyCode::Left => Some(Action::FormAdjustLeft),
                                KeyCode::Right => Some(Action::FormAdjustRight),
                                KeyCode::Enter | KeyCode::Char(' ') => Some(Action::FormActivate),
                                KeyCode::Esc => Some(Action::SwitchTab(0)),
                                KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                                    let digit_idx = (c as usize).saturating_sub('1' as usize);
                                    Some(Action::SwitchTab(digit_idx))
                                }
                                _ => None,
                            };

                            if let Some(action) = form_action {
                                let effects = app.reduce(action, &mut geom);
                                execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                should_render = true;
                                continue;
                            }
                        }

                        // On Tab 0 (Browser), allow 1..9 to jump directly to tab
                        if app.active_tab == 0 && !geom.has_window() {
                            if let KeyCode::Char(c) = key.code {
                                if c.is_ascii_digit() && c != '0' {
                                    let digit_idx = (c as usize).saturating_sub('1' as usize);
                                    if digit_idx < app.tabs.len() {
                                        let effects = app.reduce(Action::SwitchTab(digit_idx), &mut geom);
                                        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                        should_render = true;
                                        continue;
                                    }
                                }
                            }
                        }

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

                                // 1. Click on Header Tab Pills
                                if let Some(&(_, tab_idx)) = geom.tab_rects.iter().find(|(r, _)| r.contains(pos)) {
                                    let effects = app.reduce(Action::SwitchTab(tab_idx), &mut geom);
                                    execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                    should_render = true;
                                } else if geom.progress_rect.contains(pos) {
                                    let relative_x = mouse.column.saturating_sub(geom.progress_rect.x) as f64;
                                    let denom = geom.progress_rect.width.max(1).saturating_sub(1).max(1) as f64;
                                    let ratio = (relative_x / denom).clamp(0.0, 1.0);
                                    let effects = app.reduce(Action::SeekRatio(ratio), &mut geom);
                                    execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
                                    should_render = true;
                                } else if app.active_tab == 0 && geom.browser_rows_rect.contains(pos) {
                                    let visual = (mouse.row.saturating_sub(geom.browser_rows_rect.y)) as usize;
                                    let idx = geom.scroll_offset() + visual;
                                    if idx < app.browser_items.len() {
                                        let now = Instant::now();
                                        let is_double_click = matches!(
                                            last_click,
                                            Some((last_idx, last_time))
                                                if last_idx == idx
                                                    && now.duration_since(last_time) < Duration::from_millis(500)
                                        );
                                        last_click = Some((idx, now));

                                        let mut effects = app.reduce(Action::SelectIndex(idx), &mut geom);
                                        if is_double_click {
                                            effects.extend(app.reduce(Action::PlaySelected, &mut geom));
                                        }
                                        execute_effects(effects, &cmd_tx, &event_tx, &mut plugin_mgr);
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

    plugin_mgr.unload_all();
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
            Effect::Broadcast { event, payload } => {
                let ev = plugin::PluginEvent::Custom {
                    source: "host".to_string(),
                    name: event,
                    payload,
                };
                let envelopes = plugin_mgr.dispatch_event(&ev);
                for env in envelopes {
                    if let Err(e) = event_tx.try_send(AppEvent::Action(env)) {
                        tracing::warn!("Domain event channel full, broadcast action dropped: {}", e);
                    }
                }
            }
            Effect::Notify { summary, body } => {
                static LAST_NOTIFICATION_TIMESTAMP_MS: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let last = LAST_NOTIFICATION_TIMESTAMP_MS.load(std::sync::atomic::Ordering::Relaxed);

                // Throttle notifications: minimum 1000ms cooldown between desktop alerts
                if now_ms.saturating_sub(last) < 1000 {
                    tracing::debug!("Throttled rapid desktop notification");
                } else {
                    LAST_NOTIFICATION_TIMESTAMP_MS.store(now_ms, std::sync::atomic::Ordering::Relaxed);

                    // Bound payload lengths to prevent argument flooding
                    let safe_summary: String = summary.chars().take(128).collect();
                    let safe_body: String = body.chars().take(512).collect();

                    tokio::spawn(async move {
                        let res = tokio::process::Command::new("notify-send")
                            .arg("--app-name=Tunotron")
                            .arg(&safe_summary)
                            .arg(&safe_body)
                            .spawn();
                        if let Err(e) = res {
                            tracing::warn!("Failed to dispatch desktop notification: {}", e);
                        }
                    });
                }
            }
            Effect::TogglePlugin(id) => {
                if let Some(disabled) = plugin_mgr.toggle_disabled(&id) {
                    let status = if disabled { "disabled" } else { "enabled" };
                    let _ = event_tx.try_send(AppEvent::Action(ActionEnvelope::internal(
                        Action::SyncPlugins(plugin_mgr.plugin_infos()),
                    )));
                    let _ = event_tx.try_send(AppEvent::Action(ActionEnvelope::internal(
                        Action::ShowToast {
                            message: format!("Extension '{}' {}", id, status),
                            duration_ms: 2000,
                        },
                    )));
                }
            }
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
