#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::app::{AppState, PlayState};
use crate::event::AppEvent;

pub fn default_socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("tunotron.sock")
    } else {
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/tunotron-{}.sock", uid))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TunotronStatus {
    pub state: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub position: f64,
    pub duration: f64,
    pub volume: f64,
    pub loop_mode: String,
    pub shuffle: bool,

    // Waybar-compatible JSON fields
    pub text: String,
    pub alt: String,
    pub tooltip: String,
    pub class: String,
    pub percentage: u8,
}

impl TunotronStatus {
    pub fn from_app(app: &AppState) -> Self {
        let state_str = match app.playback.state {
            PlayState::Playing => "playing",
            PlayState::Paused => "paused",
            PlayState::Stopped => "stopped",
        };

        let (title, artist, album) = if let Some(track) = &app.playback.current_track {
            (track.title.clone(), track.artist.clone(), track.album.clone())
        } else {
            ("No Track".to_string(), "".to_string(), "".to_string())
        };

        let pos = app.clock.now();
        let dur = app.playback.duration_sec;
        let percentage = if dur > 0.0 {
            ((pos / dur) * 100.0).clamp(0.0, 100.0) as u8
        } else {
            0
        };

        let text = if app.playback.current_track.is_some() {
            if artist.is_empty() {
                title.clone()
            } else {
                format!("{} — {}", artist, title)
            }
        } else {
            "Tunotron Stopped".to_string()
        };

        let tooltip = if app.playback.current_track.is_some() {
            format!(
                "{}\nAlbum: {}\n[{}]",
                text,
                if album.is_empty() { "Unknown" } else { &album },
                app.playback.time_label
            )
        } else {
            "No active track".to_string()
        };

        Self {
            state: state_str.to_string(),
            title,
            artist,
            album,
            position: (pos * 10.0).round() / 10.0,
            duration: (dur * 10.0).round() / 10.0,
            volume: app.playback.volume,
            loop_mode: app.playback.loop_mode.display_str().to_string(),
            shuffle: app.playback.shuffle_mode == crate::action::ShuffleMode::On,
            text,
            alt: state_str.to_string(),
            tooltip,
            class: state_str.to_string(),
            percentage,
        }
    }
}

impl Default for TunotronStatus {
    fn default() -> Self {
        Self {
            state: "stopped".to_string(),
            title: "No Track".to_string(),
            artist: "".to_string(),
            album: "".to_string(),
            position: 0.0,
            duration: 0.0,
            volume: 100.0,
            loop_mode: "All".to_string(),
            shuffle: false,
            text: "Tunotron Stopped".to_string(),
            alt: "stopped".to_string(),
            tooltip: "No active track".to_string(),
            class: "stopped".to_string(),
            percentage: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd")]
pub enum IpcRequest {
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "action")]
    Action {
        name: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    #[serde(rename = "subscribe")]
    Subscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TunotronStatus>,
}

pub struct IpcHandle {
    pub status_state: Arc<RwLock<TunotronStatus>>,
    pub broadcast_tx: broadcast::Sender<TunotronStatus>,
    pub socket_path: PathBuf,
}

impl IpcHandle {
    pub fn update_status(&self, new_status: TunotronStatus) {
        if let Ok(mut w) = self.status_state.write() {
            *w = new_status.clone();
        }
        let _ = self.broadcast_tx.send(new_status);
    }
}

impl Drop for IpcHandle {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
    }
}

/// Spawns the background Unix socket listener for external tools (Waybar, CLI, Tauri).
pub fn spawn_ipc_server(
    event_tx: mpsc::Sender<AppEvent>,
    initial_status: TunotronStatus,
) -> Result<IpcHandle, std::io::Error> {
    let socket_path = default_socket_path();
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path)?;
    let status_state = Arc::new(RwLock::new(initial_status));
    let (broadcast_tx, _) = broadcast::channel::<TunotronStatus>(16);

    let status_clone = Arc::clone(&status_state);
    let broadcast_clone = broadcast_tx.clone();
    let socket_path_clone = socket_path.clone();

    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let status_ref = Arc::clone(&status_clone);
                    let broadcast_rx = broadcast_clone.subscribe();
                    let ev_tx = event_tx.clone();

                    tokio::spawn(async move {
                        let (reader, mut writer) = stream.into_split();
                        let mut lines = BufReader::new(reader).lines();

                        while let Ok(Some(line)) = lines.next_line().await {
                            let line = line.trim();
                            if line.is_empty() {
                                continue;
                            }

                            let req: Result<IpcRequest, _> = serde_json::from_str(line);
                            match req {
                                Ok(IpcRequest::Status) => {
                                    let current = status_ref.read().map(|r| r.clone()).unwrap_or_default();
                                    let resp = IpcResponse {
                                        ok: true,
                                        error: None,
                                        status: Some(current),
                                    };
                                    if let Ok(json) = serde_json::to_string(&resp) {
                                        let _ = writer.write_all(json.as_bytes()).await;
                                        let _ = writer.write_all(b"\n").await;
                                    }
                                }
                                Ok(IpcRequest::Action { name, args }) => {
                                    let action_opt = parse_ipc_action(&name, &args);
                                    if let Some(action) = action_opt {
                                        let env = crate::action::ActionEnvelope::user(action);
                                        let _ = ev_tx.try_send(AppEvent::Action(env));
                                        let resp = IpcResponse {
                                            ok: true,
                                            error: None,
                                            status: None,
                                        };
                                        if let Ok(json) = serde_json::to_string(&resp) {
                                            let _ = writer.write_all(json.as_bytes()).await;
                                            let _ = writer.write_all(b"\n").await;
                                        }
                                    } else {
                                        let resp = IpcResponse {
                                            ok: false,
                                            error: Some(format!("Unknown or invalid action: {}", name)),
                                            status: None,
                                        };
                                        if let Ok(json) = serde_json::to_string(&resp) {
                                            let _ = writer.write_all(json.as_bytes()).await;
                                            let _ = writer.write_all(b"\n").await;
                                        }
                                    }
                                }
                                Ok(IpcRequest::Subscribe) => {
                                    let mut rx = broadcast_rx;
                                    let current = status_ref.read().map(|r| r.clone()).unwrap_or_default();
                                    if let Ok(json) = serde_json::to_string(&current) {
                                        let _ = writer.write_all(json.as_bytes()).await;
                                        let _ = writer.write_all(b"\n").await;
                                    }
                                    while let Ok(updated) = rx.recv().await {
                                        if let Ok(json) = serde_json::to_string(&updated) {
                                            if writer.write_all(json.as_bytes()).await.is_err()
                                                || writer.write_all(b"\n").await.is_err()
                                            {
                                                break;
                                            }
                                        }
                                    }
                                    break;
                                }
                                Err(e) => {
                                    let resp = IpcResponse {
                                        ok: false,
                                        error: Some(format!("Invalid JSON request: {}", e)),
                                        status: None,
                                    };
                                    if let Ok(json) = serde_json::to_string(&resp) {
                                        let _ = writer.write_all(json.as_bytes()).await;
                                        let _ = writer.write_all(b"\n").await;
                                    }
                                }
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("IPC accept error: {}", e);
                    break;
                }
            }
        }
    });

    Ok(IpcHandle {
        status_state,
        broadcast_tx,
        socket_path: socket_path_clone,
    })
}

pub fn parse_ipc_action(name: &str, args: &serde_json::Value) -> Option<Action> {
    match name {
        "toggle" | "play_pause" | "TogglePause" => Some(Action::TogglePause),
        "stop" | "Stop" => Some(Action::Stop),
        "next" | "NextTrack" => Some(Action::NextTrack),
        "prev" | "PrevTrack" => Some(Action::PrevTrack),
        "loop" | "CycleLoopMode" => Some(Action::CycleLoopMode),
        "shuffle" | "ToggleShuffle" => Some(Action::ToggleShuffle),
        "density" | "ToggleDensity" => Some(Action::ToggleDensity),
        "seek" => {
            if let Some(delta) = args.as_i64() {
                Some(Action::Seek(delta))
            } else if let Some(delta) = args.get("delta").and_then(|v| v.as_i64()) {
                Some(Action::Seek(delta))
            } else {
                args.get("seconds").and_then(|v| v.as_f64()).map(Action::SeekAbsolute)
            }
        }
        "seek_to" => {
            let pos = args.as_f64().or_else(|| args.get("seconds").and_then(|v| v.as_f64()))?;
            Some(Action::SeekAbsolute(pos))
        }
        "volume" => {
            if let Some(vol) = args.as_f64() {
                Some(Action::SetVolume(vol))
            } else if let Some(delta) = args.get("delta").and_then(|v| v.as_i64()) {
                Some(Action::VolumeDelta(delta as i8))
            } else {
                args.get("volume").and_then(|v| v.as_f64()).map(Action::SetVolume)
            }
        }
        "toast" => {
            let msg = if let Some(s) = args.as_str() {
                s.to_string()
            } else {
                args.get("message")?.as_str()?.to_string()
            };
            let dur = args.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(2500);
            Some(Action::ShowToast { message: msg, duration_ms: dur })
        }
        "notify" => {
            let summary = args.get("summary")?.as_str()?.to_string();
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            Some(Action::Notify { summary, body })
        }
        _ => None,
    }
}

/// Executes a CLI command against a running Tunotron instance.
pub async fn run_cli_command(cmd: &str, args: &[String]) -> Result<(), String> {
    let socket_path = default_socket_path();
    if !socket_path.exists() {
        return Err(format!(
            "Tunotron is not running (socket not found at {})",
            socket_path.display()
        ));
    }

    let stream = UnixStream::connect(&socket_path)
        .await
        .map_err(|e| format!("Failed to connect to Tunotron socket: {}", e))?;

    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    match cmd {
        "status" => {
            let is_json = args.iter().any(|a| a == "--json");
            let is_follow = args.iter().any(|a| a == "--follow" || a == "-f");

            if is_follow {
                let req = serde_json::json!({ "cmd": "subscribe" });
                writer
                    .write_all(format!("{}\n", req).as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;

                while let Ok(Some(line)) = lines.next_line().await {
                    if is_json {
                        println!("{}", line);
                    } else if let Ok(status) = serde_json::from_str::<TunotronStatus>(&line) {
                        print_human_status(&status);
                    }
                }
            } else {
                let req = serde_json::json!({ "cmd": "status" });
                writer
                    .write_all(format!("{}\n", req).as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;

                if let Ok(Some(line)) = lines.next_line().await {
                    if is_json {
                        if let Ok(resp) = serde_json::from_str::<IpcResponse>(&line) {
                            if let Some(st) = resp.status {
                                println!("{}", serde_json::to_string(&st).unwrap_or(line));
                            } else {
                                println!("{}", line);
                            }
                        } else {
                            println!("{}", line);
                        }
                    } else if let Ok(resp) = serde_json::from_str::<IpcResponse>(&line) {
                        if let Some(status) = resp.status {
                            print_human_status(&status);
                        } else {
                            println!("No status returned");
                        }
                    }
                }
            }
        }
        "play" | "pause" | "toggle" | "next" | "prev" | "stop" | "loop" | "shuffle" => {
            let req = serde_json::json!({
                "cmd": "action",
                "name": cmd,
            });
            writer
                .write_all(format!("{}\n", req).as_bytes())
                .await
                .map_err(|e| e.to_string())?;

            if let Ok(Some(line)) = lines.next_line().await {
                if let Ok(resp) = serde_json::from_str::<IpcResponse>(&line) {
                    if resp.ok {
                        println!("tunotron: {} ok", cmd);
                    } else {
                        return Err(resp.error.unwrap_or_else(|| "Command failed".into()));
                    }
                }
            }
        }
        "seek" => {
            let arg = args.first().ok_or("Usage: tunotron seek <[+|-]seconds>")?;
            let req = if arg.starts_with('+') || arg.starts_with('-') {
                let delta: i64 = arg.parse().map_err(|_| "Invalid seek delta")?;
                serde_json::json!({ "cmd": "action", "name": "seek", "args": delta })
            } else {
                let sec: f64 = arg.parse().map_err(|_| "Invalid seek seconds")?;
                serde_json::json!({ "cmd": "action", "name": "seek_to", "args": sec })
            };

            writer
                .write_all(format!("{}\n", req).as_bytes())
                .await
                .map_err(|e| e.to_string())?;

            if let Ok(Some(line)) = lines.next_line().await {
                if let Ok(resp) = serde_json::from_str::<IpcResponse>(&line) {
                    if resp.ok {
                        println!("tunotron: seek ok");
                    } else {
                        return Err(resp.error.unwrap_or_else(|| "Seek failed".into()));
                    }
                }
            }
        }
        "volume" => {
            let arg = args.first().ok_or("Usage: tunotron volume <[+|-]percent>")?;
            let req = if arg.starts_with('+') || arg.starts_with('-') {
                let delta: i64 = arg.parse().map_err(|_| "Invalid volume delta")?;
                serde_json::json!({ "cmd": "action", "name": "volume", "args": { "delta": delta } })
            } else {
                let vol: f64 = arg.parse().map_err(|_| "Invalid volume percent")?;
                serde_json::json!({ "cmd": "action", "name": "volume", "args": vol })
            };

            writer
                .write_all(format!("{}\n", req).as_bytes())
                .await
                .map_err(|e| e.to_string())?;

            if let Ok(Some(line)) = lines.next_line().await {
                if let Ok(resp) = serde_json::from_str::<IpcResponse>(&line) {
                    if resp.ok {
                        println!("tunotron: volume ok");
                    } else {
                        return Err(resp.error.unwrap_or_else(|| "Volume failed".into()));
                    }
                }
            }
        }
        "toast" => {
            let msg = args.join(" ");
            if msg.is_empty() {
                return Err("Usage: tunotron toast <message>".into());
            }
            let req = serde_json::json!({
                "cmd": "action",
                "name": "toast",
                "args": msg
            });
            writer
                .write_all(format!("{}\n", req).as_bytes())
                .await
                .map_err(|e| e.to_string())?;

            if let Ok(Some(line)) = lines.next_line().await {
                if let Ok(resp) = serde_json::from_str::<IpcResponse>(&line) {
                    if resp.ok {
                        println!("tunotron: toast ok");
                    } else {
                        return Err(resp.error.unwrap_or_else(|| "Toast failed".into()));
                    }
                }
            }
        }
        other => {
            return Err(format!("Unknown command '{}'. Available: status, play, pause, toggle, stop, next, prev, seek, volume, loop, shuffle, toast", other));
        }
    }

    Ok(())
}

fn print_human_status(status: &TunotronStatus) {
    let icon = match status.state.as_str() {
        "playing" => "▶",
        "paused" => "⏸",
        _ => "⏹",
    };

    let pos_m = (status.position as u64) / 60;
    let pos_s = (status.position as u64) % 60;
    let dur_m = (status.duration as u64) / 60;
    let dur_s = (status.duration as u64) % 60;

    println!(
        "{} {} [{:02}:{:02} / {:02}:{:02}] (Vol: {:.0}%, Loop: {}, Shuffle: {})",
        icon,
        status.text,
        pos_m,
        pos_s,
        dur_m,
        dur_s,
        status.volume,
        status.loop_mode,
        if status.shuffle { "On" } else { "Off" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ipc_action_parser() {
        assert_eq!(parse_ipc_action("toggle", &serde_json::Value::Null), Some(Action::TogglePause));
        assert_eq!(parse_ipc_action("next", &serde_json::Value::Null), Some(Action::NextTrack));
        assert_eq!(parse_ipc_action("prev", &serde_json::Value::Null), Some(Action::PrevTrack));
        assert_eq!(parse_ipc_action("stop", &serde_json::Value::Null), Some(Action::Stop));
        assert_eq!(
            parse_ipc_action("seek", &serde_json::json!(10)),
            Some(Action::Seek(10))
        );
        assert_eq!(
            parse_ipc_action("seek_to", &serde_json::json!(45.5)),
            Some(Action::SeekAbsolute(45.5))
        );
        assert_eq!(
            parse_ipc_action("volume", &serde_json::json!(80.0)),
            Some(Action::SetVolume(80.0))
        );
        assert_eq!(
            parse_ipc_action("volume", &serde_json::json!({ "delta": 5 })),
            Some(Action::VolumeDelta(5))
        );
        assert_eq!(
            parse_ipc_action("toast", &serde_json::json!("Hello Tunotron")),
            Some(Action::ShowToast {
                message: "Hello Tunotron".to_string(),
                duration_ms: 2500,
            })
        );
    }

    #[test]
    fn test_tunotron_status_default() {
        let status = TunotronStatus::default();
        assert_eq!(status.state, "stopped");
        assert_eq!(status.class, "stopped");
        assert_eq!(status.percentage, 0);
    }
}
