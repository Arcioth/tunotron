#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LoopMode {
    Off,
    Track,
    All,
}

impl LoopMode {
    pub fn next(&self) -> Self {
        match self {
            LoopMode::Off => LoopMode::Track,
            LoopMode::Track => LoopMode::All,
            LoopMode::All => LoopMode::Off,
        }
    }

    pub fn display_str(&self) -> &'static str {
        match self {
            LoopMode::Off => "Off",
            LoopMode::Track => "Track",
            LoopMode::All => "All",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ShuffleMode {
    Off,
    On,
}

impl ShuffleMode {
    pub fn toggle(&self) -> Self {
        match self {
            ShuffleMode::Off => ShuffleMode::On,
            ShuffleMode::On => ShuffleMode::Off,
        }
    }

    pub fn display_str(&self) -> &'static str {
        match self {
            ShuffleMode::Off => "Off",
            ShuffleMode::On => "On",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    // Navigation / Motions
    MoveDown(usize),
    MoveUp(usize),
    MoveToTop,
    MoveToBottom,
    HalfPageDown,
    HalfPageUp,

    // Playback
    PlaySelected,
    PlayTrackIndex(usize),
    TogglePause,
    Stop,
    NextTrack,
    PrevTrack,
    Seek(i64),         // delta in seconds (+5, -5)
    SeekRatio(f64),    // 0.0 to 1.0 (from mouse click on seekbar)
    VolumeDelta(i8),   // delta in percent (+5, -5)
    SetVolume(f64),
    CycleLoopMode,
    ToggleShuffle,

    // Directory & Filesystem
    EnterDirectory,
    GoToParentDirectory,
    ReloadDirectory,
    LocatePlayingTrack,
    SelectIndex(usize),

    // Windows & UI
    ToggleHelp,
    CloseTopWindow,
    ToggleDensity,

    // Dynamic Extension Actions
    Custom(String),

    // Application
    Quit,
}
