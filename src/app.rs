use std::path::PathBuf;
use ratatui::widgets::TableState;
use tokio::sync::mpsc;

use crate::action::{Action, LoopMode, ShuffleMode};
use crate::audio::{MpvCommand, MpvEvent};
use crate::library::{read_directory, BrowserEntry, Track};

#[derive(Debug, Clone)]
pub struct PlaybackState {
    pub is_playing: bool,
    pub is_paused: bool,
    pub current_time_sec: f64,
    pub duration_sec: f64,
    pub volume: f64,
    pub loop_mode: LoopMode,
    pub shuffle_mode: ShuffleMode,
    pub current_track: Option<Track>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            is_playing: false,
            is_paused: false,
            current_time_sec: 0.0,
            duration_sec: 0.0,
            volume: 100.0,
            loop_mode: LoopMode::All,
            shuffle_mode: ShuffleMode::Off,
            current_track: None,
        }
    }
}

pub struct AppState {
    pub is_running: bool,
    pub current_dir: PathBuf,
    pub browser_items: Vec<BrowserEntry>,
    pub table_state: TableState,
    pub active_playlist: Vec<Track>,
    pub active_playlist_index: Option<usize>,
    pub active_playlist_dir: Option<PathBuf>,
    pub playback: PlaybackState,
    pub cmd_tx: mpsc::UnboundedSender<MpvCommand>,
    pub show_help: bool,
}

impl AppState {
    pub fn new(music_dir: PathBuf, cmd_tx: mpsc::UnboundedSender<MpvCommand>) -> Self {
        let mut state = Self {
            is_running: true,
            current_dir: music_dir.clone(),
            browser_items: Vec::new(),
            table_state: TableState::default(),
            active_playlist: Vec::new(),
            active_playlist_index: None,
            active_playlist_dir: None,
            playback: PlaybackState::default(),
            cmd_tx,
            show_help: false,
        };

        state.reload_current_directory();
        if !state.browser_items.is_empty() {
            state.table_state.select(Some(0));
        }

        state
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_playing
    }

    pub fn reload_current_directory(&mut self) {
        let items = read_directory(&self.current_dir);
        let old_selected = self.table_state.selected().unwrap_or(0);
        self.browser_items = items;

        if self.browser_items.is_empty() {
            self.table_state.select(None);
        } else {
            let next_idx = old_selected.min(self.browser_items.len() - 1);
            self.table_state.select(Some(next_idx));
        }
    }

    pub fn handle_action(&mut self, action: Action) {
        // If modal window is open, handle window dismissal
        if self.show_help {
            match action {
                Action::CloseTopWindow | Action::ToggleHelp => {
                    self.show_help = false;
                    return;
                }
                Action::Quit => {
                    self.is_running = false;
                    let _ = self.cmd_tx.send(MpvCommand::Quit);
                    return;
                }
                _ => return,
            }
        }

        match action {
            Action::Quit => {
                self.is_running = false;
                let _ = self.cmd_tx.send(MpvCommand::Quit);
            }
            Action::ToggleHelp => {
                self.show_help = true;
            }
            Action::CloseTopWindow => {
                self.show_help = false;
            }
            Action::ReloadDirectory => {
                self.reload_current_directory();
            }
            Action::GoToParentDirectory => {
                if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
                    self.current_dir = parent;
                    self.reload_current_directory();
                    self.table_state.select(Some(0));
                }
            }
            Action::EnterDirectory | Action::PlaySelected => {
                self.enter_selected();
            }
            Action::PlayTrackIndex(idx) => {
                if idx < self.active_playlist.len() {
                    self.play_playlist_index(idx);
                }
            }

            // --- Robust Pause & Resume (Fixes Spacebar Bug) ---
            Action::TogglePause => {
                if self.playback.is_playing {
                    self.playback.is_playing = false;
                    self.playback.is_paused = true;
                    let _ = self.cmd_tx.send(MpvCommand::SetPause(true));
                } else if self.playback.is_paused {
                    self.playback.is_playing = true;
                    self.playback.is_paused = false;
                    let _ = self.cmd_tx.send(MpvCommand::SetPause(false));
                } else if let Some(track) = &self.playback.current_track {
                    self.play_track(track.clone());
                } else {
                    // Try to play first track in view
                    self.play_first_audio_in_current_folder();
                }
            }
            Action::Stop => {
                let _ = self.cmd_tx.send(MpvCommand::Stop);
                self.playback.is_playing = false;
                self.playback.is_paused = false;
                self.playback.current_time_sec = 0.0;
            }
            Action::NextTrack => {
                self.advance_track(false);
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
            Action::CycleLoopMode => {
                self.playback.loop_mode = self.playback.loop_mode.next();
            }
            Action::ToggleShuffle => {
                self.playback.shuffle_mode = self.playback.shuffle_mode.toggle();
            }

            // Motions
            Action::MoveDown(count) => {
                let total = self.browser_items.len();
                if total > 0 {
                    let current = self.table_state.selected().unwrap_or(0);
                    let next = (current + count).min(total - 1);
                    self.table_state.select(Some(next));
                }
            }
            Action::MoveUp(count) => {
                let current = self.table_state.selected().unwrap_or(0);
                let prev = current.saturating_sub(count);
                self.table_state.select(Some(prev));
            }
            Action::MoveToTop => {
                if !self.browser_items.is_empty() {
                    self.table_state.select(Some(0));
                }
            }
            Action::MoveToBottom => {
                let total = self.browser_items.len();
                if total > 0 {
                    self.table_state.select(Some(total - 1));
                }
            }
            Action::HalfPageDown => {
                let total = self.browser_items.len();
                if total > 0 {
                    let current = self.table_state.selected().unwrap_or(0);
                    let next = (current + 15).min(total - 1);
                    self.table_state.select(Some(next));
                }
            }
            Action::HalfPageUp => {
                let current = self.table_state.selected().unwrap_or(0);
                let prev = current.saturating_sub(15);
                self.table_state.select(Some(prev));
            }
        }
    }

    pub fn enter_selected(&mut self) {
        let selected_idx = self.table_state.selected();
        if let Some(idx) = selected_idx {
            if let Some(entry) = self.browser_items.get(idx).cloned() {
                match entry {
                    BrowserEntry::ParentDir(parent_path) => {
                        self.current_dir = parent_path;
                        self.reload_current_directory();
                        self.table_state.select(Some(0));
                    }
                    BrowserEntry::Directory { path, .. } => {
                        self.current_dir = path;
                        self.reload_current_directory();
                        self.table_state.select(Some(0));
                    }
                    BrowserEntry::AudioTrack(track) => {
                        // The entire folder becomes the Active Playlist!
                        self.set_folder_as_active_playlist(&track);
                    }
                }
            }
        }
    }

    fn set_folder_as_active_playlist(&mut self, starting_track: &Track) {
        let mut tracks = Vec::new();
        for item in &self.browser_items {
            if let BrowserEntry::AudioTrack(t) = item {
                tracks.push(t.clone());
            }
        }

        let start_idx = tracks.iter().position(|t| t.path == starting_track.path).unwrap_or(0);
        self.active_playlist = tracks;
        self.active_playlist_index = Some(start_idx);
        self.active_playlist_dir = Some(self.current_dir.clone());

        self.play_track(starting_track.clone());
    }

    fn play_first_audio_in_current_folder(&mut self) {
        let first_track = self.browser_items.iter().find_map(|item| {
            if let BrowserEntry::AudioTrack(track) = item {
                Some(track.clone())
            } else {
                None
            }
        });

        if let Some(track) = first_track {
            self.set_folder_as_active_playlist(&track);
        }
    }

    pub fn play_playlist_index(&mut self, idx: usize) {
        if let Some(track) = self.active_playlist.get(idx).cloned() {
            self.active_playlist_index = Some(idx);
            self.play_track(track);
        }
    }

    pub fn play_track(&mut self, track: Track) {
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

    pub fn advance_track(&mut self, is_auto_eof: bool) {
        if self.active_playlist.is_empty() {
            return;
        }

        // Loop: Track
        if is_auto_eof && self.playback.loop_mode == LoopMode::Track {
            if let Some(track) = self.playback.current_track.clone() {
                self.play_track(track);
                return;
            }
        }

        // Shuffle Mode
        if self.playback.shuffle_mode == ShuffleMode::On {
            let total = self.active_playlist.len();
            if total > 1 {
                let current = self.active_playlist_index.unwrap_or(0);
                let mut next = pseudo_random(total);
                if next == current {
                    next = (next + 1) % total;
                }
                self.play_playlist_index(next);
            } else {
                self.play_playlist_index(0);
            }
            return;
        }

        // Linear advance
        let current_idx = self.active_playlist_index.unwrap_or(0);
        let next_idx = current_idx + 1;

        if next_idx < self.active_playlist.len() {
            self.play_playlist_index(next_idx);
        } else {
            // Reached end of folder playlist
            match self.playback.loop_mode {
                LoopMode::All => {
                    self.play_playlist_index(0);
                }
                LoopMode::Off | LoopMode::Track => {
                    if is_auto_eof {
                        self.playback.is_playing = false;
                        self.playback.is_paused = false;
                        self.playback.current_time_sec = 0.0;
                    } else {
                        self.play_playlist_index(0);
                    }
                }
            }
        }
    }

    pub fn previous_track(&mut self) {
        if self.active_playlist.is_empty() {
            return;
        }

        // If more than 3 seconds into track, restart from beginning
        if self.playback.current_time_sec > 3.0 {
            let _ = self.cmd_tx.send(MpvCommand::Seek {
                seconds: 0.0,
                relative: false,
            });
            self.playback.current_time_sec = 0.0;
            return;
        }

        let current_idx = self.active_playlist_index.unwrap_or(0);
        let prev_idx = if current_idx == 0 {
            self.active_playlist.len().saturating_sub(1)
        } else {
            current_idx - 1
        };

        self.play_playlist_index(prev_idx);
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
                if reason.as_deref() == Some("eof") {
                    self.advance_track(true);
                }
            }
            MpvEvent::Idle => {
                self.playback.is_playing = false;
                self.playback.is_paused = false;
            }
            _ => {}
        }
    }
}

fn pseudo_random(max: usize) -> usize {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(1);
    nanos % max
}
