use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ViewId {
    Library = 1,
    FileBrowser = 2,
    Queue = 3,
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
    TogglePane,

    // Views
    SwitchView(ViewId),
    PreviousView,

    // Playback
    PlaySelected,
    PlayTrackIndex(usize),
    TogglePause,
    Stop,
    NextTrack,
    PrevTrack,
    Seek(i64),         // delta in seconds (+5, -5)
    VolumeDelta(i8),   // delta in percent (+5, -5)
    SetVolume(f64),
    EnqueueSelected,
    RemoveFromQueue(usize),
    ClearQueue,

    // Application
    RefreshLibrary,
    Quit,
}
