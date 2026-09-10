use std::collections::HashMap;
use std::path::PathBuf;
use ratatui::layout::Rect;
use ratatui::widgets::TableState;
use tokio::sync::mpsc;

use crate::action::{Action, LoopMode, ShuffleMode};
use crate::audio::{MpvCommand, MpvEvent};
use crate::library::{read_directory, resolve_in_jail, BrowserEntry, Track};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewDensity {
    Comfortable,
    Compact,
}

impl ViewDensity {
    pub fn toggle(&self) -> Self {
        match self {
            ViewDensity::Comfortable => ViewDensity::Compact,
            ViewDensity::Compact => ViewDensity::Comfortable,
        }
    }
}

#[derive(Debug, Clone)]
pub struct XorShift64(pub u64);

impl XorShift64 {
    pub fn from_entropy() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        Self(nanos | 1)
    }

    pub fn next_bound(&mut self, max: usize) -> usize {
        if max <= 1 {
            return 0;
        }
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x as usize) % max
    }
}

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
    pub music_root: PathBuf,
    pub current_dir: PathBuf,
    pub browser_items: Vec<BrowserEntry>,
    pub table_state: TableState,
    pub active_playlist: Vec<Track>,
    pub active_playlist_index: Option<usize>,
    pub active_playlist_dir: Option<PathBuf>,
    pub shuffle_deck: Vec<usize>,
    pub shuffle_history: Vec<usize>,
    pub rng: XorShift64,
    pub playback: PlaybackState,
    pub metadata_cache: HashMap<PathBuf, crate::event::MetadataPatch>,
    pub cmd_tx: mpsc::Sender<MpvCommand>,
    pub event_tx: mpsc::Sender<crate::event::AppEvent>,
    pub show_help: bool,
    pub density: ViewDensity,
    pub browser_rect: Rect,
    pub browser_rows_rect: Rect,
    pub progress_rect: Rect,
}

impl AppState {
    pub fn new(
        music_dir: PathBuf,
        cmd_tx: mpsc::Sender<MpvCommand>,
        event_tx: mpsc::Sender<crate::event::AppEvent>,
    ) -> Self {
        let canonical_root = music_dir.canonicalize().unwrap_or(music_dir);
        let mut state = Self {
            is_running: true,
            music_root: canonical_root.clone(),
            current_dir: canonical_root.clone(),
            browser_items: Vec::new(),
            table_state: TableState::default(),
            active_playlist: Vec::new(),
            active_playlist_index: None,
            active_playlist_dir: None,
            shuffle_deck: Vec::new(),
            shuffle_history: Vec::new(),
            rng: XorShift64::from_entropy(),
            playback: PlaybackState::default(),
            metadata_cache: HashMap::new(),
            cmd_tx,
            event_tx,
            show_help: false,
            density: ViewDensity::Comfortable,
            browser_rect: Rect::default(),
            browser_rows_rect: Rect::default(),
            progress_rect: Rect::default(),
        };

        let initial_items = read_directory(&canonical_root, &canonical_root);
        state.set_browser_items(initial_items);

        state
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_playing
    }

    /// Non-blocking folder reload: reads directory in background thread pool to prevent UI stutters
    pub fn reload_current_directory(&mut self) {
        let dir = self.current_dir.clone();
        let root = self.music_root.clone();
        let event_tx = self.event_tx.clone();

        tokio::task::spawn_blocking(move || {
            let items = read_directory(&dir, &root);
            let _ = event_tx.blocking_send(crate::event::AppEvent::DirectoryLoaded { dir, items });
        });
    }

    /// Sets new browser items, applying cached metadata immediately and only scanning uncached tracks
    pub fn set_browser_items(&mut self, mut items: Vec<BrowserEntry>) {
        let old_selected = self.table_state.selected().unwrap_or(0);
        let mut missing_paths = Vec::new();

        // 1. Immediately apply cached metadata to tracks
        for entry in &mut items {
            if let BrowserEntry::AudioTrack(track) = entry {
                if let Some(patch) = self.metadata_cache.get(&track.path) {
                    if let Some(title) = &patch.title {
                        track.title = title.clone();
                    }
                    if let Some(artist) = &patch.artist {
                        track.artist = artist.clone();
                    }
                    if let Some(album) = &patch.album {
                        track.album = album.clone();
                    }
                    if patch.duration_sec > 0.0 {
                        track.set_duration(patch.duration_sec);
                    }
                    if patch.track_number.is_some() {
                        track.track_number = patch.track_number;
                    }
                } else {
                    missing_paths.push(track.path.clone());
                }
            }
        }

        self.browser_items = items;

        if self.browser_items.is_empty() {
            self.table_state.select(None);
        } else {
            let next_idx = old_selected.min(self.browser_items.len() - 1);
            self.table_state.select(Some(next_idx));
        }

        // 2. Only spawn background probe for paths that are NOT in the cache
        if !missing_paths.is_empty() {
            crate::library::Scanner::scan_paths_in_background(missing_paths, self.event_tx.clone());
        }
    }

    pub fn apply_metadata_patches(&mut self, patches: Vec<crate::event::MetadataPatch>) {
        for patch in patches {
            // Save in session cache to eliminate re-reads upon revisiting
            self.metadata_cache.insert(patch.path.clone(), patch.clone());

            for item in &mut self.browser_items {
                if let BrowserEntry::AudioTrack(t) = item {
                    if t.path == patch.path {
                        if let Some(title) = &patch.title {
                            t.title = title.clone();
                        }
                        if let Some(artist) = &patch.artist {
                            t.artist = artist.clone();
                        }
                        if let Some(album) = &patch.album {
                            t.album = album.clone();
                        }
                        if patch.duration_sec > 0.0 {
                            t.set_duration(patch.duration_sec);
                        }
                        if patch.track_number.is_some() {
                            t.track_number = patch.track_number;
                        }
                    }
                }
            }

            for t in &mut self.active_playlist {
                if t.path == patch.path {
                    if let Some(title) = &patch.title {
                        t.title = title.clone();
                    }
                    if let Some(artist) = &patch.artist {
                        t.artist = artist.clone();
                    }
                    if let Some(album) = &patch.album {
                        t.album = album.clone();
                    }
                    if patch.duration_sec > 0.0 {
                        t.set_duration(patch.duration_sec);
                    }
                    if patch.track_number.is_some() {
                        t.track_number = patch.track_number;
                    }
                }
            }

            if let Some(ref mut curr) = self.playback.current_track {
                if curr.path == patch.path {
                    if let Some(title) = &patch.title {
                        curr.title = title.clone();
                    }
                    if let Some(artist) = &patch.artist {
                        curr.artist = artist.clone();
                    }
                    if let Some(album) = &patch.album {
                        curr.album = album.clone();
                    }
                    if patch.duration_sec > 0.0 {
                        curr.set_duration(patch.duration_sec);
                        if self.playback.duration_sec == 0.0 {
                            self.playback.duration_sec = patch.duration_sec;
                        }
                    }
                    if patch.track_number.is_some() {
                        curr.track_number = patch.track_number;
                    }
                }
            }
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
                    let _ = self.cmd_tx.try_send(MpvCommand::Quit);
                    return;
                }
                _ => return,
            }
        }

        match action {
            Action::Quit => {
                self.is_running = false;
                let _ = self.cmd_tx.try_send(MpvCommand::Quit);
            }
            Action::ToggleHelp => {
                self.show_help = true;
            }
            Action::CloseTopWindow => {
                self.show_help = false;
            }
            Action::ToggleDensity => {
                self.density = self.density.toggle();
            }
            Action::ReloadDirectory => {
                self.reload_current_directory();
            }

            // Jailed navigation: cannot navigate higher than music_root
            Action::GoToParentDirectory => {
                if self.current_dir != self.music_root && self.current_dir.starts_with(&self.music_root) {
                    if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
                        if resolve_in_jail(&self.music_root, &parent).is_some() {
                            self.current_dir = parent;
                            self.reload_current_directory();
                            self.table_state.select(Some(0));
                        }
                    }
                }
            }

            // Locate currently playing song
            Action::LocatePlayingTrack => {
                self.locate_playing_track();
            }

            Action::SelectIndex(idx) => {
                if idx < self.browser_items.len() {
                    self.table_state.select(Some(idx));
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

            // Robust Pause & Resume
            Action::TogglePause => {
                if self.playback.is_playing {
                    self.playback.is_playing = false;
                    self.playback.is_paused = true;
                    let _ = self.cmd_tx.try_send(MpvCommand::SetPause(true));
                } else if self.playback.is_paused {
                    self.playback.is_playing = true;
                    self.playback.is_paused = false;
                    let _ = self.cmd_tx.try_send(MpvCommand::SetPause(false));
                } else if let Some(track) = &self.playback.current_track {
                    self.play_track(track.clone());
                } else {
                    self.play_first_audio_in_current_folder();
                }
            }
            Action::Stop => {
                let _ = self.cmd_tx.try_send(MpvCommand::Stop);
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
                let _ = self.cmd_tx.try_send(MpvCommand::Seek {
                    seconds: delta_sec as f64,
                    relative: true,
                });
            }
            Action::SeekRatio(ratio) => {
                let clamped = ratio.clamp(0.0, 1.0);
                let target_sec = clamped * self.playback.duration_sec;
                let _ = self.cmd_tx.try_send(MpvCommand::Seek {
                    seconds: target_sec,
                    relative: false,
                });
                self.playback.current_time_sec = target_sec;
            }
            Action::VolumeDelta(delta_pct) => {
                let new_vol = (self.playback.volume + delta_pct as f64).clamp(0.0, 100.0);
                self.playback.volume = new_vol;
                let _ = self.cmd_tx.try_send(MpvCommand::SetVolume(new_vol));
            }
            Action::SetVolume(vol) => {
                let clamped = vol.clamp(0.0, 100.0);
                self.playback.volume = clamped;
                let _ = self.cmd_tx.try_send(MpvCommand::SetVolume(clamped));
            }
            Action::CycleLoopMode => {
                self.playback.loop_mode = self.playback.loop_mode.next();
            }
            Action::ToggleShuffle => {
                self.playback.shuffle_mode = self.playback.shuffle_mode.toggle();
                if self.playback.shuffle_mode == ShuffleMode::On {
                    self.reset_shuffle_deck();
                }
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
                let step = (self.browser_rows_rect.height / 2).max(1) as usize;
                let total = self.browser_items.len();
                if total > 0 {
                    let current = self.table_state.selected().unwrap_or(0);
                    let next = (current + step).min(total - 1);
                    self.table_state.select(Some(next));
                }
            }
            Action::HalfPageUp => {
                let step = (self.browser_rows_rect.height / 2).max(1) as usize;
                let current = self.table_state.selected().unwrap_or(0);
                let prev = current.saturating_sub(step);
                self.table_state.select(Some(prev));
            }

            // Extension custom hook
            Action::Custom(_) => {}
        }
    }

    pub fn enter_selected(&mut self) {
        let Some(idx) = self.table_state.selected() else { return };
        match self.browser_items.get(idx) {
            Some(BrowserEntry::ParentDir(parent_path)) => {
                let parent_path = parent_path.clone();
                if resolve_in_jail(&self.music_root, &parent_path).is_some() {
                    self.current_dir = parent_path;
                    self.reload_current_directory();
                    self.table_state.select(Some(0));
                }
            }
            Some(BrowserEntry::Directory { path, .. }) => {
                let Some(jailed_path) = resolve_in_jail(&self.music_root, path) else { return };
                self.current_dir = jailed_path;
                self.reload_current_directory();
                self.table_state.select(Some(0));
            }
            Some(BrowserEntry::AudioTrack(track)) => {
                let track = track.clone();
                self.set_folder_as_active_playlist(&track);
            }
            None => {}
        }
    }

    pub fn locate_playing_track(&mut self) {
        if let Some(track) = &self.playback.current_track {
            let track_path = track.path.clone();
            if let Some(folder) = track_path.parent() {
                // If we are in a different folder, jump to the track's folder first
                if self.current_dir != folder {
                    self.current_dir = folder.to_path_buf();
                    self.reload_current_directory();
                }

                // Locate and highlight the track row
                if let Some(pos) = self.browser_items.iter().position(|e| e.path() == track_path) {
                    self.table_state.select(Some(pos));
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
        self.shuffle_history.clear();

        if self.playback.shuffle_mode == ShuffleMode::On {
            self.reset_shuffle_deck();
        }

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
            if let Some(curr) = self.active_playlist_index {
                if curr != idx {
                    self.shuffle_history.push(curr);
                }
            }
            self.active_playlist_index = Some(idx);
            self.play_track(track);
        }
    }

    pub fn play_track(&mut self, track: Track) {
        self.playback.is_playing = true;
        self.playback.is_paused = false;
        self.playback.current_time_sec = 0.0;
        self.playback.duration_sec = track.duration_sec;

        let _ = self.cmd_tx.try_send(MpvCommand::LoadFile {
            path: track.path.to_string_lossy().to_string(),
            replace: true,
        });

        self.playback.current_track = Some(track);
    }

    /// Generates a non-repeating Fisher-Yates shuffle deck
    fn reset_shuffle_deck(&mut self) {
        let total = self.active_playlist.len();
        if total <= 1 {
            self.shuffle_deck.clear();
            return;
        }

        let current = self.active_playlist_index.unwrap_or(0);
        let mut deck: Vec<usize> = (0..total).filter(|&i| i != current).collect();

        // Fisher-Yates Shuffle
        for i in (1..deck.len()).rev() {
            let j = self.rng.next_bound(i + 1);
            deck.swap(i, j);
        }

        self.shuffle_deck = deck;
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

        // Shuffle Mode (Fisher-Yates Deck: never plays the same ending song, stops at deck end if Loop: Off)
        if self.playback.shuffle_mode == ShuffleMode::On {
            if self.shuffle_deck.is_empty() {
                if is_auto_eof && self.playback.loop_mode != LoopMode::All {
                    self.playback.is_playing = false;
                    self.playback.is_paused = false;
                    self.playback.current_time_sec = 0.0;
                    return;
                }
                self.reset_shuffle_deck();
            }

            if let Some(next_idx) = self.shuffle_deck.pop() {
                self.play_playlist_index(next_idx);
                return;
            }
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

        // If more than 3 seconds into track, restart from 0:00
        if self.playback.current_time_sec > 3.0 {
            let _ = self.cmd_tx.try_send(MpvCommand::Seek {
                seconds: 0.0,
                relative: false,
            });
            self.playback.current_time_sec = 0.0;
            return;
        }

        // Under shuffle, step back through shuffle history
        if self.playback.shuffle_mode == ShuffleMode::On {
            if let Some(prev) = self.shuffle_history.pop() {
                if let Some(track) = self.active_playlist.get(prev).cloned() {
                    self.active_playlist_index = Some(prev);
                    self.play_track(track);
                    return;
                }
            }
        }

        let current_idx = self.active_playlist_index.unwrap_or(0);
        let prev_idx = if current_idx == 0 {
            self.active_playlist.len().saturating_sub(1)
        } else {
            current_idx - 1
        };

        self.play_playlist_index(prev_idx);
    }

    /// Handles an mpv event and returns a boolean `dirty` flag.
    /// Returns `false` for high-frequency `time-pos` ticks to prevent CPU render storms,
    /// and `true` for discrete state changes that warrant immediate screen redraw.
    pub fn handle_mpv_event(&mut self, event: MpvEvent) -> bool {
        match event {
            MpvEvent::PropertyChange { name, data: Some(val), .. } => {
                match name.as_str() {
                    "time-pos" => {
                        if let Some(num) = val.as_f64() {
                            self.playback.current_time_sec = num;
                        }
                        false
                    }
                    "duration" => {
                        if let Some(num) = val.as_f64() {
                            self.playback.duration_sec = num;
                        }
                        true
                    }
                    "pause" => {
                        if let Some(paused) = val.as_bool() {
                            self.playback.is_paused = paused;
                            self.playback.is_playing = !paused;
                        }
                        true
                    }
                    "volume" => {
                        if let Some(v) = val.as_f64() {
                            self.playback.volume = v;
                        }
                        true
                    }
                    _ => false,
                }
            }
            MpvEvent::PropertyChange { .. } => false,
            MpvEvent::FileLoaded => {
                self.playback.is_playing = true;
                self.playback.is_paused = false;
                true
            }
            MpvEvent::EndFile { reason, .. } => {
                if reason.as_deref() == Some("eof") {
                    self.advance_track(true);
                }
                true
            }
            MpvEvent::Idle => {
                self.playback.is_playing = false;
                self.playback.is_paused = false;
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xorshift64_deterministic() {
        let mut rng1 = XorShift64(123456789);
        let mut rng2 = XorShift64(123456789);

        for _ in 0..100 {
            assert_eq!(rng1.next_bound(10), rng2.next_bound(10));
        }
    }

    #[test]
    fn test_xorshift64_bounds() {
        let mut rng = XorShift64(987654321);
        for max in [1, 2, 5, 10, 100] {
            for _ in 0..50 {
                let val = rng.next_bound(max);
                assert!(val < max);
            }
        }
    }

    #[test]
    fn test_advance_track_loop_modes() {
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);
        let (event_tx, _event_rx) = mpsc::channel(16);
        let tmp = std::env::temp_dir();
        let mut app = AppState::new(tmp.clone(), cmd_tx, event_tx);

        let t1 = Track::new(1, tmp.join("1.mp3"));
        let t2 = Track::new(2, tmp.join("2.mp3"));
        app.active_playlist = vec![t1, t2];
        app.active_playlist_index = Some(0);
        app.playback.is_playing = true;

        // Linear advance to index 1
        app.advance_track(false);
        assert_eq!(app.active_playlist_index, Some(1));

        // LoopMode::All wraps around to index 0
        app.playback.loop_mode = LoopMode::All;
        app.advance_track(true);
        assert_eq!(app.active_playlist_index, Some(0));

        // LoopMode::Off stops playback at EOF
        app.active_playlist_index = Some(1);
        app.playback.loop_mode = LoopMode::Off;
        app.advance_track(true);
        assert_eq!(app.playback.is_playing, false);

        // LoopMode::Track repeats track
        app.active_playlist_index = Some(0);
        app.playback.is_playing = true;
        app.playback.loop_mode = LoopMode::Track;
        app.advance_track(true);
        assert_eq!(app.active_playlist_index, Some(0));
        assert_eq!(app.playback.is_playing, true);
    }

    #[test]
    fn test_advance_track_shuffle_loop_off() {
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);
        let (event_tx, _event_rx) = mpsc::channel(16);
        let tmp = std::env::temp_dir();
        let mut app = AppState::new(tmp.clone(), cmd_tx, event_tx);

        let t1 = Track::new(1, tmp.join("1.mp3"));
        let t2 = Track::new(2, tmp.join("2.mp3"));
        app.active_playlist = vec![t1, t2];
        app.active_playlist_index = Some(0);
        app.playback.is_playing = true;
        app.playback.shuffle_mode = ShuffleMode::On;
        app.playback.loop_mode = LoopMode::Off;
        app.shuffle_deck = vec![1]; // 1 item left in deck

        // Advance plays item 1, emptying deck
        app.advance_track(false);
        assert_eq!(app.active_playlist_index, Some(1));
        assert!(app.shuffle_deck.is_empty());

        // Auto EOF with empty deck and LoopMode::Off halts playback
        app.advance_track(true);
        assert_eq!(app.playback.is_playing, false);
    }

    #[test]
    fn test_apply_metadata_patches() {
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);
        let (event_tx, _event_rx) = mpsc::channel(16);
        let tmp = std::env::temp_dir();
        let mut app = AppState::new(tmp.clone(), cmd_tx, event_tx);

        let p1 = tmp.join("song.mp3");
        let t1 = Track::new(1, p1.clone());
        app.browser_items = vec![BrowserEntry::AudioTrack(t1)];

        let patch = crate::event::MetadataPatch {
            path: p1,
            title: Some("New Title".to_string()),
            artist: Some("New Artist".to_string()),
            album: Some("New Album".to_string()),
            duration_sec: 185.0,
            track_number: Some(4),
        };

        app.apply_metadata_patches(vec![patch]);

        if let BrowserEntry::AudioTrack(t) = &app.browser_items[0] {
            assert_eq!(t.title, "New Title");
            assert_eq!(t.artist, "New Artist");
            assert_eq!(t.album, "New Album");
            assert_eq!(t.duration_label, "03:05");
            assert_eq!(t.track_number, Some(4));
        } else {
            panic!("Expected AudioTrack");
        }
    }

    #[test]
    fn test_metadata_cache_instant_load() {
        let (cmd_tx, _cmd_rx) = mpsc::channel(16);
        let (event_tx, mut event_rx) = mpsc::channel(16);
        let tmp = std::env::temp_dir();
        let mut app = AppState::new(tmp.clone(), cmd_tx, event_tx);

        let p1 = tmp.join("cached_song.mp3");

        // Pre-populate metadata cache
        app.metadata_cache.insert(
            p1.clone(),
            crate::event::MetadataPatch {
                path: p1.clone(),
                title: Some("Cached Title".to_string()),
                artist: Some("Cached Artist".to_string()),
                album: Some("Cached Album".to_string()),
                duration_sec: 120.0,
                track_number: Some(1),
            },
        );

        // Setting browser items with a known cached track populates it synchronously
        let fresh_track = Track::new(1, p1.clone());
        app.set_browser_items(vec![BrowserEntry::AudioTrack(fresh_track)]);

        if let BrowserEntry::AudioTrack(t) = &app.browser_items[0] {
            assert_eq!(t.title, "Cached Title");
            assert_eq!(t.artist, "Cached Artist");
            assert_eq!(t.duration_label, "02:00");
        } else {
            panic!("Expected AudioTrack");
        }

        // Verify that NO background scanner events were triggered since all tracks were cached
        assert!(event_rx.try_recv().is_err());
    }
}

