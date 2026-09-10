use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use serde::{Deserialize, Serialize};

use crate::action::{Action, Effect, LoopMode, ShuffleMode, WindowId};
use crate::audio::{MpvCommand, MpvEvent};
use crate::library::{read_directory, resolve_in_jail, BrowserEntry, LruCache, Track};
use crate::ui::UiGeom;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone)]
pub struct PlaybackClock {
    anchor_pos: f64,
    anchor_at: Instant,
    playing: bool,
}

impl PlaybackClock {
    pub fn new() -> Self {
        Self {
            anchor_pos: 0.0,
            anchor_at: Instant::now(),
            playing: false,
        }
    }

    pub fn now(&self) -> f64 {
        if self.playing {
            self.anchor_pos + self.anchor_at.elapsed().as_secs_f64()
        } else {
            self.anchor_pos
        }
    }

    pub fn sync(&mut self, pos: f64) {
        self.anchor_pos = pos.max(0.0);
        self.anchor_at = Instant::now();
    }

    pub fn set_playing(&mut self, playing: bool) {
        let cur = self.now();
        self.anchor_pos = cur;
        self.anchor_at = Instant::now();
        self.playing = playing;
    }
}

impl Default for PlaybackClock {
    fn default() -> Self {
        Self::new()
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
    pub state: PlayState,
    pub current_time_sec: f64,
    pub duration_sec: f64,
    pub volume: f64,
    pub loop_mode: LoopMode,
    pub shuffle_mode: ShuffleMode,
    pub current_track: Option<Arc<Track>>,
    // Precomputed strings to guarantee 0 heap allocations during per-frame rendering
    pub now_playing_label: String,
    pub vol_label: String,
    pub duration_label: String,
    pub time_label: String,
}

impl PlaybackState {
    pub fn is_playing(&self) -> bool {
        self.state == PlayState::Playing
    }

    pub fn is_paused(&self) -> bool {
        self.state == PlayState::Paused
    }

    #[allow(dead_code)]
    pub fn is_stopped(&self) -> bool {
        self.state == PlayState::Stopped
    }

    pub fn update_now_playing(&mut self) {
        if let Some(track) = &self.current_track {
            self.now_playing_label = format!("{} — {}", track.artist, track.title);
        } else {
            self.now_playing_label = "No track playing. Select an audio file and press Enter.".to_string();
        }
    }

    pub fn update_vol_label(&mut self) {
        self.vol_label = format!("Vol: {:>3.0}%", self.volume);
    }

    pub fn update_duration_label(&mut self) {
        self.duration_label = crate::library::track::format_mmss(self.duration_sec);
    }

    pub fn update_time_label(&mut self, elapsed_sec: f64) {
        let elapsed_fmt = crate::library::track::format_mmss(elapsed_sec);
        self.time_label = format!("{} / {}", elapsed_fmt, self.duration_label);
    }
}

impl Default for PlaybackState {
    fn default() -> Self {
        let mut s = Self {
            state: PlayState::Stopped,
            current_time_sec: 0.0,
            duration_sec: 0.0,
            volume: 100.0,
            loop_mode: LoopMode::All,
            shuffle_mode: ShuffleMode::Off,
            current_track: None,
            now_playing_label: String::new(),
            vol_label: String::new(),
            duration_label: "00:00".to_string(),
            time_label: "00:00 / 00:00".to_string(),
        };
        s.update_now_playing();
        s.update_vol_label();
        s
    }
}

pub struct AppState {
    pub is_running: bool,
    pub music_root: PathBuf,
    pub current_dir: PathBuf,
    pub browser_items: Vec<BrowserEntry>,
    pub active_playlist: Vec<Arc<Track>>,
    pub active_playlist_index: Option<usize>,
    pub active_playlist_dir: Option<PathBuf>,
    pub shuffle_deck: Vec<usize>,
    pub shuffle_history: Vec<usize>,
    pub rng: XorShift64,
    pub playback: PlaybackState,
    pub clock: PlaybackClock,
    pub last_rendered_sec: u64,
    pub browser_title: String,
    pub metadata_cache: LruCache<PathBuf, crate::event::MetadataPatch>,
    pub density: ViewDensity,
}

impl AppState {
    pub const METADATA_CACHE_CAP: usize = 10_000;

    pub fn new(music_dir: PathBuf) -> (Self, Vec<Effect>) {
        let canonical_root = music_dir.canonicalize().unwrap_or(music_dir);
        let mut state = Self {
            is_running: true,
            music_root: canonical_root.clone(),
            current_dir: canonical_root.clone(),
            browser_items: Vec::new(),
            active_playlist: Vec::new(),
            active_playlist_index: None,
            active_playlist_dir: None,
            shuffle_deck: Vec::new(),
            shuffle_history: Vec::new(),
            rng: XorShift64::from_entropy(),
            playback: PlaybackState::default(),
            clock: PlaybackClock::new(),
            last_rendered_sec: u64::MAX,
            browser_title: " Music Browser (0 items) ".to_string(),
            metadata_cache: LruCache::new(Self::METADATA_CACHE_CAP),
            density: ViewDensity::Comfortable,
        };

        let initial_items = read_directory(&canonical_root, &canonical_root);
        let effects = state.set_browser_items(initial_items);

        (state, effects)
    }

    pub fn is_playing(&self) -> bool {
        self.playback.is_playing()
    }

    /// Returns an effect to reload current directory asynchronously
    pub fn reload_current_directory(&self) -> Effect {
        Effect::LoadDirectory {
            dir: self.current_dir.clone(),
            root: self.music_root.clone(),
        }
    }

    /// Sets new browser items, applying cached metadata immediately and returning
    /// a ScanMetadata effect for uncached tracks.
    pub fn set_browser_items(&mut self, mut items: Vec<BrowserEntry>) -> Vec<Effect> {
        let mut missing_paths = Vec::new();

        // 1. Immediately apply cached metadata to tracks
        for entry in &mut items {
            if let BrowserEntry::AudioTrack(track) = entry {
                if let Some(patch) = self.metadata_cache.get(&track.path) {
                    let mut updated = (**track).clone();
                    if let Some(title) = &patch.title {
                        updated.title = title.clone();
                    }
                    if let Some(artist) = &patch.artist {
                        updated.artist = artist.clone();
                    }
                    if let Some(album) = &patch.album {
                        updated.album = album.clone();
                    }
                    if patch.duration_sec > 0.0 {
                        updated.set_duration(patch.duration_sec);
                    }
                    if patch.track_number.is_some() {
                        updated.track_number = patch.track_number;
                    }
                    *track = Arc::new(updated);
                } else {
                    missing_paths.push(track.path.clone());
                }
            }
        }

        self.browser_items = items;
        self.browser_title = format!(" Music Browser ({} items) ", self.browser_items.len());

        // 2. Return ScanMetadata effect for paths not in the cache
        if !missing_paths.is_empty() {
            vec![Effect::ScanMetadata(missing_paths)]
        } else {
            Vec::new()
        }
    }

    pub fn apply_metadata_patches(&mut self, patches: Vec<crate::event::MetadataPatch>) {
        for patch in patches {
            self.metadata_cache.insert(patch.path.clone(), patch.clone());

            for item in &mut self.browser_items {
                if let BrowserEntry::AudioTrack(t) = item {
                    if t.path == patch.path {
                        let mut updated = (**t).clone();
                        if let Some(title) = &patch.title {
                            updated.title = title.clone();
                        }
                        if let Some(artist) = &patch.artist {
                            updated.artist = artist.clone();
                        }
                        if let Some(album) = &patch.album {
                            updated.album = album.clone();
                        }
                        if patch.duration_sec > 0.0 {
                            updated.set_duration(patch.duration_sec);
                        }
                        if patch.track_number.is_some() {
                            updated.track_number = patch.track_number;
                        }
                        *t = Arc::new(updated);
                        break;
                    }
                }
            }

            for t in &mut self.active_playlist {
                if t.path == patch.path {
                    let mut updated = (**t).clone();
                    if let Some(title) = &patch.title {
                        updated.title = title.clone();
                    }
                    if let Some(artist) = &patch.artist {
                        updated.artist = artist.clone();
                    }
                    if let Some(album) = &patch.album {
                        updated.album = album.clone();
                    }
                    if patch.duration_sec > 0.0 {
                        updated.set_duration(patch.duration_sec);
                    }
                    if patch.track_number.is_some() {
                        updated.track_number = patch.track_number;
                    }
                    *t = Arc::new(updated);
                }
            }

            if let Some(curr) = &self.playback.current_track {
                if curr.path == patch.path {
                    let mut updated = (**curr).clone();
                    if let Some(title) = &patch.title {
                        updated.title = title.clone();
                    }
                    if let Some(artist) = &patch.artist {
                        updated.artist = artist.clone();
                    }
                    if let Some(album) = &patch.album {
                        updated.album = album.clone();
                    }
                    if patch.duration_sec > 0.0 {
                        updated.set_duration(patch.duration_sec);
                        if self.playback.duration_sec == 0.0 {
                            self.playback.duration_sec = patch.duration_sec;
                        }
                    }
                    if patch.track_number.is_some() {
                        updated.track_number = patch.track_number;
                    }
                    self.playback.current_track = Some(Arc::new(updated));
                    self.playback.update_now_playing();
                    self.playback.update_duration_label();
                    self.playback.update_time_label(self.clock.now());
                }
            }
        }
    }

    pub fn reduce(&mut self, action: Action, geom: &mut UiGeom) -> Vec<Effect> {
        // If modal window is open, handle window dismissal and consume input
        if geom.has_window() {
            match action {
                Action::CloseTopWindow => {
                    geom.pop_window();
                    return Vec::new();
                }
                Action::ToggleHelp if geom.top_window() == Some(WindowId::Help) => {
                    geom.pop_window();
                    return Vec::new();
                }
                Action::ShowModal { title, content } => {
                    geom.open_modal(title, content);
                    return Vec::new();
                }
                Action::Notify { summary, body } => {
                    return vec![Effect::Notify { summary, body }];
                }
                Action::Quit => {
                    self.is_running = false;
                    return vec![Effect::Mpv(MpvCommand::Quit)];
                }
                _ => return Vec::new(),
            }
        }

        match action {
            Action::Quit => {
                self.is_running = false;
                vec![Effect::Mpv(MpvCommand::Quit)]
            }
            Action::ToggleHelp => {
                geom.toggle_window(WindowId::Help);
                Vec::new()
            }
            Action::OpenWindow(id) => {
                geom.push_window(id);
                Vec::new()
            }
            Action::ShowModal { title, content } => {
                geom.open_modal(title, content);
                Vec::new()
            }
            Action::CloseTopWindow => {
                geom.pop_window();
                Vec::new()
            }
            Action::ToggleDensity => {
                self.density = self.density.toggle();
                Vec::new()
            }
            Action::ReloadDirectory => {
                vec![self.reload_current_directory()]
            }

            // Jailed navigation: cannot navigate higher than music_root
            Action::GoToParentDirectory => {
                if self.current_dir != self.music_root && self.current_dir.starts_with(&self.music_root) {
                    if let Some(parent) = self.current_dir.parent().map(|p| p.to_path_buf()) {
                        if resolve_in_jail(&self.music_root, &parent).is_some() {
                            self.current_dir = parent;
                            geom.select(Some(0));
                            return vec![self.reload_current_directory()];
                        }
                    }
                }
                Vec::new()
            }

            // Locate currently playing song
            Action::LocatePlayingTrack => {
                self.locate_playing_track(geom)
            }

            Action::SelectIndex(idx) => {
                geom.select_index(idx, self.browser_items.len());
                Vec::new()
            }

            Action::EnterDirectory | Action::PlaySelected => {
                self.enter_selected(geom)
            }
            Action::PlayTrackIndex(idx) => {
                if idx < self.active_playlist.len() {
                    self.play_playlist_index(idx)
                } else {
                    Vec::new()
                }
            }

            // Robust Pause & Resume with PlayState and PlaybackClock
            Action::TogglePause => {
                if self.playback.is_playing() {
                    self.playback.state = PlayState::Paused;
                    self.clock.set_playing(false);
                    vec![Effect::Mpv(MpvCommand::SetPause(true))]
                } else if self.playback.is_paused() {
                    self.playback.state = PlayState::Playing;
                    self.clock.set_playing(true);
                    vec![Effect::Mpv(MpvCommand::SetPause(false))]
                } else if let Some(track) = &self.playback.current_track {
                    self.play_track(Arc::clone(track))
                } else {
                    self.play_first_audio_in_current_folder()
                }
            }
            Action::Stop => {
                self.playback.state = PlayState::Stopped;
                self.clock.sync(0.0);
                self.clock.set_playing(false);
                self.playback.current_time_sec = 0.0;
                self.playback.update_time_label(0.0);
                vec![Effect::Mpv(MpvCommand::Stop)]
            }
            Action::NextTrack => {
                self.advance_track(false)
            }
            Action::PrevTrack => {
                self.previous_track()
            }
            Action::Seek(delta_sec) => {
                let new_pos = (self.clock.now() + delta_sec as f64).clamp(0.0, self.playback.duration_sec.max(0.0));
                self.clock.sync(new_pos);
                self.playback.current_time_sec = new_pos;
                self.playback.update_time_label(new_pos);
                vec![Effect::Mpv(MpvCommand::Seek {
                    seconds: delta_sec as f64,
                    relative: true,
                })]
            }
            Action::SeekRatio(ratio) => {
                let clamped = ratio.clamp(0.0, 1.0);
                let target_sec = clamped * self.playback.duration_sec;
                self.clock.sync(target_sec);
                self.playback.current_time_sec = target_sec;
                self.playback.update_time_label(target_sec);
                vec![Effect::Mpv(MpvCommand::Seek {
                    seconds: target_sec,
                    relative: false,
                })]
            }
            Action::VolumeDelta(delta_pct) => {
                let new_vol = (self.playback.volume + delta_pct as f64).clamp(0.0, 100.0);
                self.playback.volume = new_vol;
                self.playback.update_vol_label();
                vec![Effect::Mpv(MpvCommand::SetVolume(new_vol))]
            }
            Action::SetVolume(vol) => {
                let clamped = vol.clamp(0.0, 100.0);
                self.playback.volume = clamped;
                self.playback.update_vol_label();
                vec![Effect::Mpv(MpvCommand::SetVolume(clamped))]
            }
            Action::CycleLoopMode => {
                self.playback.loop_mode = self.playback.loop_mode.next();
                Vec::new()
            }
            Action::ToggleShuffle => {
                self.playback.shuffle_mode = self.playback.shuffle_mode.toggle();
                if self.playback.shuffle_mode == ShuffleMode::On {
                    self.reset_shuffle_deck();
                }
                Vec::new()
            }

            // Motions
            Action::MoveDown(count) => {
                geom.move_down(count, self.browser_items.len());
                Vec::new()
            }
            Action::MoveUp(count) => {
                geom.move_up(count);
                Vec::new()
            }
            Action::MoveToTop => {
                geom.move_to_top();
                Vec::new()
            }
            Action::MoveToBottom => {
                geom.move_to_bottom(self.browser_items.len());
                Vec::new()
            }
            Action::HalfPageDown => {
                let step = (geom.browser_rows_rect.height / 2).max(1) as usize;
                geom.move_down(step, self.browser_items.len());
                Vec::new()
            }
            Action::HalfPageUp => {
                let step = (geom.browser_rows_rect.height / 2).max(1) as usize;
                geom.move_up(step);
                Vec::new()
            }

            // Extension Action Seam
            Action::Plugin { plugin_id, name, payload } => {
                tracing::debug!("Received plugin action: [{}] {} {:?}", plugin_id, name, payload);
                vec![Effect::PluginAction { plugin_id, name, payload }]
            }

            Action::Notify { summary, body } => {
                vec![Effect::Notify { summary, body }]
            }
        }
    }

    pub fn enter_selected(&mut self, geom: &mut UiGeom) -> Vec<Effect> {
        let Some(idx) = geom.selected() else { return Vec::new(); };
        match self.browser_items.get(idx) {
            Some(BrowserEntry::ParentDir(parent_path)) => {
                let parent_path = parent_path.clone();
                if resolve_in_jail(&self.music_root, &parent_path).is_some() {
                    self.current_dir = parent_path;
                    geom.select(Some(0));
                    vec![self.reload_current_directory()]
                } else {
                    Vec::new()
                }
            }
            Some(BrowserEntry::Directory { path, .. }) => {
                let Some(jailed_path) = resolve_in_jail(&self.music_root, path) else { return Vec::new(); };
                self.current_dir = jailed_path;
                geom.select(Some(0));
                vec![self.reload_current_directory()]
            }
            Some(BrowserEntry::AudioTrack(track)) => {
                let track = track.clone();
                self.set_folder_as_active_playlist(&track)
            }
            None => Vec::new(),
        }
    }

    pub fn locate_playing_track(&mut self, geom: &mut UiGeom) -> Vec<Effect> {
        if let Some(track) = &self.playback.current_track {
            let track_path = track.path.clone();
            if let Some(folder) = track_path.parent() {
                // If we are in a different folder, jump to the track's folder first
                if self.current_dir != folder {
                    self.current_dir = folder.to_path_buf();
                    return vec![self.reload_current_directory()];
                }

                // Locate and highlight the track row
                if let Some(pos) = self.browser_items.iter().position(|e| e.path() == track_path) {
                    geom.select(Some(pos));
                }
            }
        }
        Vec::new()
    }

    fn set_folder_as_active_playlist(&mut self, starting_track: &Arc<Track>) -> Vec<Effect> {
        let mut tracks = Vec::new();
        for item in &self.browser_items {
            if let BrowserEntry::AudioTrack(t) = item {
                tracks.push(Arc::clone(t));
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

        self.play_track(Arc::clone(starting_track))
    }

    fn play_first_audio_in_current_folder(&mut self) -> Vec<Effect> {
        let first_track = self.browser_items.iter().find_map(|item| {
            if let BrowserEntry::AudioTrack(track) = item {
                Some(Arc::clone(track))
            } else {
                None
            }
        });

        if let Some(track) = first_track {
            self.set_folder_as_active_playlist(&track)
        } else {
            Vec::new()
        }
    }

    pub fn play_playlist_index(&mut self, idx: usize) -> Vec<Effect> {
        if let Some(track) = self.active_playlist.get(idx).cloned() {
            if let Some(curr) = self.active_playlist_index {
                if curr != idx {
                    self.shuffle_history.push(curr);
                }
            }
            self.active_playlist_index = Some(idx);
            self.play_track(track)
        } else {
            Vec::new()
        }
    }

    pub fn play_track(&mut self, track: Arc<Track>) -> Vec<Effect> {
        self.playback.state = PlayState::Playing;
        self.clock.sync(0.0);
        self.clock.set_playing(true);
        self.playback.current_time_sec = 0.0;
        self.playback.duration_sec = track.duration_sec;

        let cmd = MpvCommand::LoadFile {
            path: track.path.to_string_lossy().to_string(),
            replace: true,
        };

        self.playback.current_track = Some(track);
        self.playback.update_now_playing();
        self.playback.update_duration_label();
        self.playback.update_time_label(0.0);

        vec![Effect::Mpv(cmd)]
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

    pub fn advance_track(&mut self, is_auto_eof: bool) -> Vec<Effect> {
        if self.active_playlist.is_empty() {
            return Vec::new();
        }

        // Loop: Track
        if is_auto_eof && self.playback.loop_mode == LoopMode::Track {
            if let Some(track) = self.playback.current_track.clone() {
                return self.play_track(track);
            }
        }

        // Shuffle Mode (Fisher-Yates Deck: never plays the same ending song, stops at deck end if Loop: Off)
        if self.playback.shuffle_mode == ShuffleMode::On {
            if self.shuffle_deck.is_empty() {
                if is_auto_eof && self.playback.loop_mode != LoopMode::All {
                    self.playback.state = PlayState::Stopped;
                    self.clock.sync(0.0);
                    self.clock.set_playing(false);
                    self.playback.current_time_sec = 0.0;
                    self.playback.update_time_label(0.0);
                    return Vec::new();
                }
                self.reset_shuffle_deck();
            }

            if let Some(next_idx) = self.shuffle_deck.pop() {
                return self.play_playlist_index(next_idx);
            }
        }

        // Linear advance
        let current_idx = self.active_playlist_index.unwrap_or(0);
        let next_idx = current_idx + 1;

        if next_idx < self.active_playlist.len() {
            self.play_playlist_index(next_idx)
        } else {
            // Reached end of folder playlist
            match self.playback.loop_mode {
                LoopMode::All => {
                    self.play_playlist_index(0)
                }
                LoopMode::Off | LoopMode::Track => {
                    if is_auto_eof {
                        self.playback.state = PlayState::Stopped;
                        self.clock.sync(0.0);
                        self.clock.set_playing(false);
                        self.playback.current_time_sec = 0.0;
                        self.playback.update_time_label(0.0);
                        Vec::new()
                    } else {
                        self.play_playlist_index(0)
                    }
                }
            }
        }
    }

    pub fn previous_track(&mut self) -> Vec<Effect> {
        if self.active_playlist.is_empty() {
            return Vec::new();
        }

        // If more than 3 seconds into track, restart from 0:00
        if self.clock.now() > 3.0 {
            self.clock.sync(0.0);
            self.playback.current_time_sec = 0.0;
            self.playback.update_time_label(0.0);
            return vec![Effect::Mpv(MpvCommand::Seek {
                seconds: 0.0,
                relative: false,
            })];
        }

        // Under shuffle, step back through shuffle history
        if self.playback.shuffle_mode == ShuffleMode::On {
            if let Some(prev) = self.shuffle_history.pop() {
                if let Some(track) = self.active_playlist.get(prev).cloned() {
                    self.active_playlist_index = Some(prev);
                    return self.play_track(track);
                }
            }
        }

        let current_idx = self.active_playlist_index.unwrap_or(0);
        let prev_idx = if current_idx == 0 {
            self.active_playlist.len().saturating_sub(1)
        } else {
            current_idx - 1
        };

        self.play_playlist_index(prev_idx)
    }

    /// Handles an mpv event and returns `(dirty, effects)`.
    /// Returns `dirty = false` for high-frequency `time-pos` ticks to prevent CPU render storms,
    /// and `dirty = true` for discrete state changes that warrant immediate screen redraw.
    pub fn handle_mpv_event(&mut self, event: MpvEvent) -> (bool, Vec<Effect>) {
        match event {
            MpvEvent::PropertyChange { name, data: Some(val), .. } => {
                match name.as_str() {
                    "time-pos" => {
                        if let Some(num) = val.as_f64() {
                            self.clock.sync(num);
                            self.playback.current_time_sec = num;
                        }
                        (false, Vec::new())
                    }
                    "duration" => {
                        if let Some(num) = val.as_f64() {
                            self.playback.duration_sec = num;
                            self.playback.update_duration_label();
                            self.playback.update_time_label(self.clock.now());
                        }
                        (true, Vec::new())
                    }
                    "pause" => {
                        if let Some(paused) = val.as_bool() {
                            if paused {
                                self.playback.state = PlayState::Paused;
                                self.clock.set_playing(false);
                            } else {
                                self.playback.state = PlayState::Playing;
                                self.clock.set_playing(true);
                            }
                        }
                        (true, Vec::new())
                    }
                    "volume" => {
                        if let Some(v) = val.as_f64() {
                            self.playback.volume = v;
                            self.playback.update_vol_label();
                        }
                        (true, Vec::new())
                    }
                    _ => (false, Vec::new()),
                }
            }
            MpvEvent::PropertyChange { .. } => (false, Vec::new()),
            MpvEvent::FileLoaded => {
                self.playback.state = PlayState::Playing;
                self.clock.set_playing(true);
                self.last_rendered_sec = u64::MAX;
                (true, Vec::new())
            }
            MpvEvent::EndFile { reason, .. } => {
                if reason.as_deref() == Some("eof") {
                    let effects = self.advance_track(true);
                    (true, effects)
                } else {
                    (true, Vec::new())
                }
            }
            MpvEvent::Idle => {
                self.playback.state = PlayState::Stopped;
                self.clock.sync(0.0);
                self.clock.set_playing(false);
                self.playback.current_time_sec = 0.0;
                self.playback.update_time_label(0.0);
                self.last_rendered_sec = u64::MAX;
                (true, Vec::new())
            }
            _ => (false, Vec::new()),
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
        let tmp = std::env::temp_dir();
        let (mut app, _) = AppState::new(tmp.clone());

        let t1 = Arc::new(Track::new(tmp.join("1.mp3")));
        let t2 = Arc::new(Track::new(tmp.join("2.mp3")));
        app.active_playlist = vec![t1, t2];
        app.active_playlist_index = Some(0);
        app.playback.state = PlayState::Playing;

        // Linear advance to index 1
        let effects = app.advance_track(false);
        assert_eq!(app.active_playlist_index, Some(1));
        assert_eq!(effects.len(), 1);

        // LoopMode::All wraps around to index 0
        app.playback.loop_mode = LoopMode::All;
        let effects = app.advance_track(true);
        assert_eq!(app.active_playlist_index, Some(0));
        assert_eq!(effects.len(), 1);

        // LoopMode::Off stops playback at EOF
        app.active_playlist_index = Some(1);
        app.playback.loop_mode = LoopMode::Off;
        let effects = app.advance_track(true);
        assert!(!app.playback.is_playing());
        assert!(effects.is_empty());

        // LoopMode::Track repeats track
        app.active_playlist_index = Some(0);
        app.playback.state = PlayState::Playing;
        app.playback.loop_mode = LoopMode::Track;
        let effects = app.advance_track(true);
        assert_eq!(app.active_playlist_index, Some(0));
        assert!(app.playback.is_playing());
        assert_eq!(effects.len(), 1);
    }

    #[test]
    fn test_advance_track_shuffle_loop_off() {
        let tmp = std::env::temp_dir();
        let (mut app, _) = AppState::new(tmp.clone());

        let t1 = Arc::new(Track::new(tmp.join("1.mp3")));
        let t2 = Arc::new(Track::new(tmp.join("2.mp3")));
        app.active_playlist = vec![t1, t2];
        app.active_playlist_index = Some(0);
        app.playback.state = PlayState::Playing;
        app.playback.shuffle_mode = ShuffleMode::On;
        app.playback.loop_mode = LoopMode::Off;
        app.shuffle_deck = vec![1]; // 1 item left in deck

        // Advance plays item 1, emptying deck
        let effects = app.advance_track(false);
        assert_eq!(app.active_playlist_index, Some(1));
        assert_eq!(effects.len(), 1);
        assert!(app.shuffle_deck.is_empty());

        // Auto EOF with empty deck and LoopMode::Off halts playback
        let effects = app.advance_track(true);
        assert!(!app.playback.is_playing());
        assert!(effects.is_empty());
    }

    #[test]
    fn test_apply_metadata_patches() {
        let tmp = std::env::temp_dir();
        let (mut app, _) = AppState::new(tmp.clone());

        let p1 = tmp.join("song.mp3");
        let t1 = Arc::new(Track::new(p1.clone()));
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
        let tmp = std::env::temp_dir();
        let (mut app, _) = AppState::new(tmp.clone());

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
        let fresh_track = Arc::new(Track::new(p1.clone()));
        let effects = app.set_browser_items(vec![BrowserEntry::AudioTrack(fresh_track)]);

        if let BrowserEntry::AudioTrack(t) = &app.browser_items[0] {
            assert_eq!(t.title, "Cached Title");
            assert_eq!(t.artist, "Cached Artist");
            assert_eq!(t.duration_label, "02:00");
        } else {
            panic!("Expected AudioTrack");
        }

        // Verify that NO background scanner events were triggered since all tracks were cached
        assert!(effects.is_empty());
    }

    #[test]
    fn test_pure_reducer_effects() {
        let tmp = std::env::temp_dir();
        let (mut app, _) = AppState::new(tmp.clone());
        let mut geom = UiGeom::new();

        // Test Quit effect
        let effects = app.reduce(Action::Quit, &mut geom);
        assert_eq!(effects, vec![Effect::Mpv(MpvCommand::Quit)]);
        assert!(!app.is_running);

        // Test Seek effect
        app.playback.duration_sec = 100.0;
        let effects = app.reduce(Action::Seek(5), &mut geom);
        assert_eq!(effects, vec![Effect::Mpv(MpvCommand::Seek { seconds: 5.0, relative: true })]);
        assert_eq!(app.playback.current_time_sec, 5.0);

        // Test Plugin effect
        let effects = app.reduce(Action::Plugin {
            plugin_id: "test.plugin".into(),
            name: "custom_action".into(),
            payload: serde_json::json!({"foo": "bar"}),
        }, &mut geom);
        assert_eq!(effects, vec![Effect::PluginAction {
            plugin_id: "test.plugin".into(),
            name: "custom_action".into(),
            payload: serde_json::json!({"foo": "bar"}),
        }]);
    }

    #[test]
    fn test_playback_clock_interpolation() {
        let mut clock = PlaybackClock::new();
        assert_eq!(clock.now(), 0.0);

        clock.sync(10.0);
        assert_eq!(clock.now(), 10.0);

        clock.set_playing(true);
        std::thread::sleep(std::time::Duration::from_millis(15));
        assert!(clock.now() > 10.01);

        clock.set_playing(false);
        let paused_time = clock.now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert_eq!(clock.now(), paused_time);
    }

    #[test]
    fn test_show_modal_reducer() {
        let (mut app, _) = AppState::new(PathBuf::from("/music"));
        let mut geom = UiGeom::new();

        assert!(!geom.has_window());
        assert_eq!(geom.modal_content, None);

        // ShowModal pushes PluginModal to window stack
        let effects = app.reduce(
            Action::ShowModal {
                title: "Track Lyrics".to_string(),
                content: "Verse 1\nChorus".to_string(),
            },
            &mut geom,
        );
        assert!(effects.is_empty());
        assert!(geom.has_window());
        assert_eq!(geom.top_window(), Some(WindowId::PluginModal));
        assert_eq!(
            geom.modal_content,
            Some(crate::ui::geom::ModalContent {
                title: "Track Lyrics".to_string(),
                content: "Verse 1\nChorus".to_string(),
            })
        );

        // While modal is open, normal navigation actions are consumed
        let effects = app.reduce(Action::MoveDown(5), &mut geom);
        assert!(effects.is_empty());

        // CloseTopWindow closes the modal and cleans up modal_content
        let effects = app.reduce(Action::CloseTopWindow, &mut geom);
        assert!(effects.is_empty());
        assert!(!geom.has_window());
        assert_eq!(geom.modal_content, None);
    }

    #[test]
    fn test_notify_reducer_effects() {
        let temp_dir = std::env::temp_dir();
        let (mut app, _) = AppState::new(temp_dir);
        let mut geom = UiGeom::default();

        let effects = app.reduce(
            Action::Notify {
                summary: "Title".to_string(),
                body: "Body text".to_string(),
            },
            &mut geom,
        );

        assert_eq!(
            effects,
            vec![Effect::Notify {
                summary: "Title".to_string(),
                body: "Body text".to_string(),
            }]
        );
    }
}

