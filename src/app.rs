use std::path::PathBuf;
use tokio::sync::mpsc;
use tracing::info;

use crate::action::{Action, ViewId};
use crate::audio::{MpvCommand, MpvEvent};
use crate::event::ScannerEvent;
use crate::library::Track;
use crate::ui::views::{FileBrowserView, LibraryView, QueueView, View};

#[derive(Debug, Clone, Default)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub is_paused: bool,
    pub current_time_sec: f64,
    pub duration_sec: f64,
    pub volume: f64,
    pub current_track: Option<Track>,
}

pub struct AppViews {
    pub library: LibraryView,
    pub browser: FileBrowserView,
    pub queue: QueueView,
}

pub struct AppState {
    pub is_running: bool,
    pub active_view: ViewId,
    pub previous_view: Option<ViewId>,
    pub playback: PlaybackState,
    pub views: AppViews,
    #[allow(dead_code)]
    pub music_dir: PathBuf,
    pub cmd_tx: mpsc::UnboundedSender<MpvCommand>,
    track_generation: u64,
}

impl AppState {
    pub fn new(music_dir: PathBuf, cmd_tx: mpsc::UnboundedSender<MpvCommand>) -> Self {
        let views = AppViews {
            library: LibraryView::new(),
            browser: FileBrowserView::new(music_dir.clone()),
            queue: QueueView::new(),
        };

        Self {
            is_running: true,
            active_view: ViewId::Library,
            previous_view: None,
            playback: PlaybackState {
                volume: 100.0,
                ..Default::default()
            },
            views,
            music_dir,
            cmd_tx,
            track_generation: 0,
        }
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_playing
    }

    pub fn handle_action(&mut self, action: Action) {
        match action {
            Action::Quit => {
                self.is_running = false;
                let _ = self.cmd_tx.send(MpvCommand::Quit);
            }
            Action::SwitchView(view_id) => {
                if self.active_view != view_id {
                    self.previous_view = Some(self.active_view);
                    self.active_view = view_id;
                }
            }
            Action::PreviousView => {
                if let Some(prev) = self.previous_view {
                    self.active_view = prev;
                }
            }

            // Playback controls
            Action::PlaySelected => match self.active_view {
                ViewId::Library => {
                    if let Some(track) = self.views.library.selected_track().cloned() {
                        self.play_track(track);
                    }
                }
                ViewId::FileBrowser => {
                    if let Some(path) = self.views.browser.enter_selected() {
                        self.play_path(path);
                    }
                }
                ViewId::Queue => {
                    if let Some(idx) = self.views.queue.selected_index() {
                        if let Some(track) = self.views.queue.queue.get(idx).cloned() {
                            self.play_track(track);
                        }
                    }
                }
            },
            Action::PlayTrackIndex(idx) => {
                if let Some(track) = self.views.library.tracks.get(idx).cloned() {
                    self.play_track(track);
                }
            }
            Action::TogglePause => {
                if self.playback.is_playing {
                    let _ = self.cmd_tx.send(MpvCommand::TogglePause);
                } else if let Some(track) = &self.playback.current_track {
                    self.play_track(track.clone());
                } else if let Some(track) = self.views.library.tracks.first().cloned() {
                    self.play_track(track);
                }
            }
            Action::Stop => {
                let _ = self.cmd_tx.send(MpvCommand::Stop);
                self.playback.is_playing = false;
                self.playback.is_paused = false;
                self.playback.current_time_sec = 0.0;
            }
            Action::NextTrack => {
                self.advance_queue();
            }
            Action::PrevTrack => {
                self.previous_track();
            }
            Action::Seek(delta_sec) => {
                let _ = self.cmd_tx.send(MpvCommand::Seek {
                    seconds: delta_sec as f64,
                    relative: true,
                });
            }
            Action::VolumeDelta(delta_pct) => {
                let new_vol = (self.playback.volume + delta_pct as f64).clamp(0.0, 100.0);
                self.playback.volume = new_vol;
                let _ = self.cmd_tx.send(MpvCommand::SetVolume(new_vol));
            }
            Action::SetVolume(vol) => {
                let clamped = vol.clamp(0.0, 100.0);
                self.playback.volume = clamped;
                let _ = self.cmd_tx.send(MpvCommand::SetVolume(clamped));
            }
            Action::EnqueueSelected => {
                if let Some(track) = self.views.library.selected_track().cloned() {
                    self.views.queue.push(track);
                }
            }
            Action::RemoveFromQueue(_) => {
                self.views.queue.remove_selected();
            }
            Action::ClearQueue => {
                self.views.queue.set_queue(Vec::new());
            }

            // View navigation actions (j/k, gg, G, etc.)
            other => match self.active_view {
                ViewId::Library => {
                    self.views.library.handle_action(&other);
                }
                ViewId::FileBrowser => {
                    self.views.browser.handle_action(&other);
                }
                ViewId::Queue => {
                    self.views.queue.handle_action(&other);
                }
            },
        }
    }

    pub fn handle_mpv_event(&mut self, event: MpvEvent) {
        match event {
            MpvEvent::PropertyChange { name, data, .. } => {
                if let Some(val) = data {
                    match name.as_str() {
                        "time-pos" => {
                            if let Some(num) = val.as_f64() {
                                self.playback.current_time_sec = num;
                            }
                        }
                        "duration" => {
                            if let Some(num) = val.as_f64() {
                                self.playback.duration_sec = num;
                            }
                        }
                        "pause" => {
                            if let Some(paused) = val.as_bool() {
                                self.playback.is_paused = paused;
                                self.playback.is_playing = !paused;
                            }
                        }
                        "volume" => {
                            if let Some(v) = val.as_f64() {
                                self.playback.volume = v;
                            }
                        }
                        _ => {}
                    }
                }
            }
            MpvEvent::FileLoaded => {
                self.playback.is_playing = true;
                self.playback.is_paused = false;
            }
            MpvEvent::EndFile { reason, .. } => {
                // Strict EOF Guard: Only trigger automatic next track when reason == "eof"
                // Manual skips emit "stop", which must not trigger double advances!
                if reason.as_deref() == Some("eof") {
                    self.advance_queue();
                }
            }
            MpvEvent::Idle => {
                self.playback.is_playing = false;
                self.playback.is_paused = false;
            }
            _ => {}
        }
    }

    pub fn handle_scanner_event(&mut self, event: ScannerEvent) {
        match event {
            ScannerEvent::Batch(tracks) => {
                self.views.library.append_tracks(tracks);
            }
            ScannerEvent::Finished { total_tracks } => {
                info!("Library scanning complete: {} tracks found", total_tracks);
            }
        }
    }

    pub fn play_track(&mut self, track: Track) {
        self.track_generation += 1;
        self.playback.current_track = Some(track.clone());
        self.playback.is_playing = true;
        self.playback.is_paused = false;
        self.playback.current_time_sec = 0.0;
        self.playback.duration_sec = track.duration_sec;

        let _ = self.cmd_tx.send(MpvCommand::LoadFile {
            path: track.path.to_string_lossy().to_string(),
            replace: true,
        });
    }

    pub fn play_path(&mut self, path: PathBuf) {
        let track = Track::new(0, path);
        self.play_track(track);
    }

    pub fn advance_queue(&mut self) {
        if !self.views.queue.queue.is_empty() {
            let next_track = self.views.queue.queue.remove(0);
            self.play_track(next_track);
        } else if let Some(current) = &self.playback.current_track {
            // If queue is empty, play next song in library
            if let Some(pos) = self.views.library.tracks.iter().position(|t| t.id == current.id) {
                let next_idx = (pos + 1) % self.views.library.tracks.len();
                if let Some(track) = self.views.library.tracks.get(next_idx).cloned() {
                    self.play_track(track);
                }
            }
        }
    }

    pub fn previous_track(&mut self) {
        if let Some(current) = &self.playback.current_track {
            if let Some(pos) = self.views.library.tracks.iter().position(|t| t.id == current.id) {
                let prev_idx = if pos == 0 {
                    self.views.library.tracks.len().saturating_sub(1)
                } else {
                    pos - 1
                };
                if let Some(track) = self.views.library.tracks.get(prev_idx).cloned() {
                    self.play_track(track);
                }
            }
        }
    }
}
