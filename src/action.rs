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

    pub fn badge_str(&self) -> &'static str {
        match self {
            LoopMode::Off => "[Loop: Off]",
            LoopMode::Track => "[Loop: Track]",
            LoopMode::All => "[Loop: All]",
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

    pub fn badge_str(&self) -> &'static str {
        match self {
            ShuffleMode::Off => "[Shuffle: Off]",
            ShuffleMode::On => "[Shuffle: On]",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WindowId {
    Help,
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
    OpenWindow(WindowId),
    CloseTopWindow,
    ToggleDensity,

    // Dynamic Extension Actions
    Plugin {
        plugin_id: String,
        name: String,
        payload: serde_json::Value,
    },

    // Application
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    PlaybackControl,
    PlaybackQueue,
    UiOverlay,
    FsJailRead,
    FsJailWrite,
    KeyBind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionPermission {
    Public,
    Capability(Capability),
    HostOnly,
}

impl Action {
    pub fn permission(&self) -> ActionPermission {
        match self {
            Action::TogglePause
            | Action::Stop
            | Action::NextTrack
            | Action::PrevTrack
            | Action::Seek(_)
            | Action::SeekRatio(_)
            | Action::VolumeDelta(_)
            | Action::SetVolume(_)
            | Action::CycleLoopMode
            | Action::ToggleShuffle => ActionPermission::Capability(Capability::PlaybackControl),

            Action::PlaySelected
            | Action::PlayTrackIndex(_) => ActionPermission::Capability(Capability::PlaybackQueue),

            Action::EnterDirectory
            | Action::GoToParentDirectory
            | Action::ReloadDirectory
            | Action::LocatePlayingTrack => ActionPermission::Capability(Capability::FsJailRead),

            Action::ToggleHelp
            | Action::OpenWindow(_)
            | Action::CloseTopWindow
            | Action::ToggleDensity => ActionPermission::Capability(Capability::UiOverlay),

            Action::MoveDown(_)
            | Action::MoveUp(_)
            | Action::MoveToTop
            | Action::MoveToBottom
            | Action::HalfPageDown
            | Action::HalfPageUp
            | Action::SelectIndex(_) => ActionPermission::Public,

            Action::Plugin { .. } => ActionPermission::Public,

            Action::Quit => ActionPermission::HostOnly,
        }
    }

    pub fn required_capability(&self) -> Option<Capability> {
        match self.permission() {
            ActionPermission::Capability(cap) => Some(cap),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionSource {
    User,
    Internal,
    Plugin(String),
}

impl ActionSource {
    /// Evaluates if the action is permitted under this origin's granted capabilities.
    /// Uses a strict DEFAULT-DENY policy for untrusted sources (plugins).
    pub fn is_permitted(&self, action: &Action, granted_caps: &[Capability]) -> bool {
        match self {
            ActionSource::User | ActionSource::Internal => true,
            ActionSource::Plugin(_) => {
                match action.permission() {
                    ActionPermission::Public => true,
                    ActionPermission::Capability(required) => granted_caps.contains(&required),
                    ActionPermission::HostOnly => false,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionEnvelope {
    pub action: Action,
    pub source: ActionSource,
}

impl ActionEnvelope {
    pub fn user(action: Action) -> Self {
        Self {
            action,
            source: ActionSource::User,
        }
    }

    pub fn internal(action: Action) -> Self {
        Self {
            action,
            source: ActionSource::Internal,
        }
    }

    pub fn plugin(plugin_id: impl Into<String>, action: Action) -> Self {
        Self {
            action,
            source: ActionSource::Plugin(plugin_id.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_source_permissions() {
        let user = ActionSource::User;
        let plugin = ActionSource::Plugin("com.example.lyrics".into());

        // User can do anything
        assert!(user.is_permitted(&Action::Quit, &[]));
        assert!(user.is_permitted(&Action::TogglePause, &[]));

        // Plugin cannot Quit under any capability (HostOnly)
        assert!(!plugin.is_permitted(&Action::Quit, &[Capability::PlaybackControl]));

        // Plugin without capability cannot control playback
        assert!(!plugin.is_permitted(&Action::TogglePause, &[]));

        // Plugin with capability can control playback
        assert!(plugin.is_permitted(&Action::TogglePause, &[Capability::PlaybackControl]));

        // Plugin can perform inert UI motions without capabilities (Public)
        assert!(plugin.is_permitted(&Action::MoveDown(1), &[]));
    }

    #[test]
    fn test_action_permissions_default_deny() {
        assert_eq!(Action::Quit.permission(), ActionPermission::HostOnly);
        assert_eq!(Action::TogglePause.permission(), ActionPermission::Capability(Capability::PlaybackControl));
        assert_eq!(Action::MoveDown(1).permission(), ActionPermission::Public);
        assert_eq!(Action::TogglePause.required_capability(), Some(Capability::PlaybackControl));
        assert_eq!(Action::PlaySelected.required_capability(), Some(Capability::PlaybackQueue));
        assert_eq!(Action::EnterDirectory.required_capability(), Some(Capability::FsJailRead));
        assert_eq!(Action::ToggleHelp.required_capability(), Some(Capability::UiOverlay));
        assert_eq!(Action::MoveDown(1).required_capability(), None);
    }
}
