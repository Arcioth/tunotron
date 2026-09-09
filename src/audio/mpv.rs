use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};
use tracing::{error, info, warn};

use super::protocol::{MpvCommand, MpvIncoming};
use crate::event::AppEvent;

pub struct MpvProcessGuard {
    child: Option<Child>,
    socket_path: PathBuf,
}

impl Drop for MpvProcessGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            // Send SIGTERM to mpv
            if let Some(id) = child.id() {
                unsafe {
                    libc::kill(id as libc::pid_t, libc::SIGTERM);
                }
            }
            let _ = child.start_kill();
        }
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

pub struct MpvSupervisor {
    #[allow(dead_code)]
    pub guard: MpvProcessGuard,
    pub stream: UnixStream,
}

impl MpvSupervisor {
    pub async fn spawn() -> Result<Self> {
        let uid = unsafe { libc::getuid() };
        let pid = std::process::id();
        let socket_dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);

        let socket_path = socket_dir.join(format!("tunotron-{}-{}.sock", uid, pid));
        let _ = std::fs::remove_file(&socket_path);

        let parent_pid = unsafe { libc::getpid() };
        let mut cmd = Command::new("mpv");

        cmd.args([
            "--no-video",
            "--audio-display=no",
            "--vo=null",
            "--no-terminal",
            "--input-terminal=no",
            "--idle=yes",
            "--keep-open=no",
            "--no-osc",
            "--no-osd-bar",
            "--load-scripts=no",
            "--no-config",
            "--msg-level=all=no",
        ]);
        cmd.arg(format!("--input-ipc-server={}", socket_path.display()));
        cmd.kill_on_drop(true);

        unsafe {
            cmd.pre_exec(move || {
                // Ask Linux kernel to send SIGTERM if parent dies (prevents orphan processes)
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent_pid {
                    libc::_exit(1);
                }
                Ok(())
            });
        }

        let mut child = cmd.spawn().context("Failed to spawn headless mpv binary")?;

        // Connect to socket with retry
        let stream = Self::connect_with_retry(&socket_path, &mut child, Duration::from_secs(3))
            .await
            .context("Failed to connect to mpv IPC socket")?;

        let guard = MpvProcessGuard {
            child: Some(child),
            socket_path,
        };

        Ok(Self { guard, stream })
    }

    async fn connect_with_retry(
        socket_path: &Path,
        child: &mut Child,
        timeout: Duration,
    ) -> Result<UnixStream> {
        let start = Instant::now();
        let poll_interval = Duration::from_millis(30);

        loop {
            if let Some(status) = child.try_wait()? {
                anyhow::bail!("mpv process exited prematurely with status: {}", status);
            }

            match UnixStream::connect(socket_path).await {
                Ok(stream) => return Ok(stream),
                Err(_) if start.elapsed() < timeout => {
                    tokio::time::sleep(poll_interval).await;
                }
                Err(e) => anyhow::bail!("Timeout connecting to mpv socket: {}", e),
            }
        }
    }
}

pub async fn run_mpv_actor(
    stream: UnixStream,
    mut cmd_rx: mpsc::UnboundedReceiver<MpvCommand>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let (read_half, write_half) = stream.into_split();
    let mut reader = FramedRead::new(read_half, LinesCodec::new());
    let mut writer = FramedWrite::new(write_half, LinesCodec::new());

    // Register property observations
    let init_cmds = [
        r#"{"command":["observe_property",1,"time-pos"]}"#,
        r#"{"command":["observe_property",2,"pause"]}"#,
        r#"{"command":["observe_property",3,"duration"]}"#,
        r#"{"command":["observe_property",4,"media-title"]}"#,
        r#"{"command":["observe_property",5,"volume"]}"#,
    ];
    for cmd in init_cmds {
        writer.send(cmd.to_string()).await?;
    }

    let mut next_req_id = 100u64;

    loop {
        tokio::select! {
            // Incoming messages from mpv over Unix socket
            maybe_line = reader.next() => {
                match maybe_line {
                    Some(Ok(line)) => {
                        if let Ok(incoming) = serde_json::from_str::<MpvIncoming>(&line) {
                            if let MpvIncoming::Event(event) = incoming {
                                let _ = event_tx.send(AppEvent::Mpv(event));
                            }
                        }
                    }
                    Some(Err(e)) => {
                        warn!("mpv IPC line read error: {}", e);
                    }
                    None => {
                        info!("mpv IPC stream closed");
                        break;
                    }
                }
            }

            // Commands from application loop
            Some(cmd) = cmd_rx.recv() => {
                next_req_id += 1;
                let req = cmd.to_request(Some(next_req_id));
                if let Ok(json_str) = serde_json::to_string(&req) {
                    if let Err(e) = writer.send(json_str).await {
                        error!("Failed to write command to mpv: {}", e);
                        break;
                    }
                }
            }

            else => break,
        }
    }

    Ok(())
}
