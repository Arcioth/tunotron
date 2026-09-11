#![allow(dead_code)]

use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::audio::MpvCommand;

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Mpv(MpvCommand),
    LoadDirectory { dir: PathBuf, root: PathBuf },
    ScanMetadata(Vec<PathBuf>),
    PluginAction {
        plugin_id: String,
        name: String,
        payload: serde_json::Value,
    },
    Notify {
        summary: String,
        body: String,
    },
    Broadcast {
        event: String,
        payload: serde_json::Value,
    },
}

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
    PluginModal,
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
    SeekAbsolute(f64), // absolute position in seconds
    VolumeDelta(i8),   // delta in percent (+5, -5)
    SetVolume(f64),
    SetAudioFilter(String), // mpv audio filter (e.g. lavfi, equalizer)
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
    ShowModal {
        title: String,
        content: String,
    },
    ShowToast {
        message: String,
        duration_ms: u64,
    },
    SetSlot {
        slot: String,
        content: String,
    },
    ClearSlot {
        slot: String,
    },

    // Tabs & Extension View Navigation
    SwitchTab(usize),
    NextTab,
    PrevTab,
    RegisterTab {
        id: String,
        title: String,
        shortcut: Option<String>,
    },
    UnregisterTab {
        id: String,
    },
    SetExtensionPage(Box<crate::ui::ExtensionPage>),
    UpdateExtensionPageField {
        page_id: String,
        field_id: String,
        value: serde_json::Value,
    },
    UpdateRadar {
        page_id: String,
        radar: crate::ui::RadarState,
    },
    FormNavUp,
    FormNavDown,
    FormAdjustLeft,
    FormAdjustRight,
    FormActivate,

    // Dynamic Extension Actions
    Plugin {
        plugin_id: String,
        name: String,
        payload: serde_json::Value,
    },

    // Host & Integration
    Notify {
        summary: String,
        body: String,
    },
    Broadcast {
        event: String,
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
    Notify,
    PersistentStorage,
}

impl std::str::FromStr for Capability {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "PlaybackControl" => Ok(Capability::PlaybackControl),
            "PlaybackQueue" => Ok(Capability::PlaybackQueue),
            "UiOverlay" => Ok(Capability::UiOverlay),
            "FsJailRead" => Ok(Capability::FsJailRead),
            "FsJailWrite" => Ok(Capability::FsJailWrite),
            "KeyBind" => Ok(Capability::KeyBind),
            "Notify" => Ok(Capability::Notify),
            "PersistentStorage" => Ok(Capability::PersistentStorage),
            other => Err(format!("Unknown capability: {}", other)),
        }
    }
}

impl Capability {
    pub fn as_str(&self) -> &'static str {
        match self {
            Capability::PlaybackControl => "PlaybackControl",
            Capability::PlaybackQueue => "PlaybackQueue",
            Capability::UiOverlay => "UiOverlay",
            Capability::FsJailRead => "FsJailRead",
            Capability::FsJailWrite => "FsJailWrite",
            Capability::KeyBind => "KeyBind",
            Capability::Notify => "Notify",
            Capability::PersistentStorage => "PersistentStorage",
        }
    }
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
            | Action::SeekAbsolute(_)
            | Action::VolumeDelta(_)
            | Action::SetVolume(_)
            | Action::SetAudioFilter(_)
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
            | Action::ToggleDensity
            | Action::ShowModal { .. }
            | Action::ShowToast { .. }
            | Action::SetSlot { .. }
            | Action::ClearSlot { .. }
            | Action::RegisterTab { .. }
            | Action::UnregisterTab { .. }
            | Action::SetExtensionPage(_)
            | Action::UpdateExtensionPageField { .. }
            | Action::UpdateRadar { .. } => ActionPermission::Capability(Capability::UiOverlay),

            Action::Notify { .. } => ActionPermission::Capability(Capability::Notify),

            Action::MoveDown(_)
            | Action::MoveUp(_)
            | Action::MoveToTop
            | Action::MoveToBottom
            | Action::HalfPageDown
            | Action::HalfPageUp
            | Action::SelectIndex(_)
            | Action::SwitchTab(_)
            | Action::NextTab
            | Action::PrevTab
            | Action::FormNavUp
            | Action::FormNavDown
            | Action::FormAdjustLeft
            | Action::FormAdjustRight
            | Action::FormActivate => ActionPermission::Public,

            Action::Plugin { .. } => ActionPermission::Public,

            Action::Broadcast { .. } => ActionPermission::Public,

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
            ActionSource::Plugin(caller_id) => {
                // Cross-plugin spoofing guard: a plugin cannot invoke another plugin's internal action surface!
                // Real user keybindings (ActionSource::User) can invoke any plugin action, but a plugin can only target itself.
                if let Action::Plugin { plugin_id, .. } = action {
                    if caller_id != plugin_id {
                        return false;
                    }
                }

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

    #[test]
    fn test_show_modal_capability_permission() {
        let action = Action::ShowModal {
            title: "Lyrics".to_string(),
            content: "Hello world".to_string(),
        };
        assert_eq!(action.permission(), ActionPermission::Capability(Capability::UiOverlay));
        assert_eq!(action.required_capability(), Some(Capability::UiOverlay));

        let plugin = ActionSource::Plugin("com.test.lyrics".into());
        assert!(!plugin.is_permitted(&action, &[]));
        assert!(!plugin.is_permitted(&action, &[Capability::PlaybackControl]));
        assert!(plugin.is_permitted(&action, &[Capability::UiOverlay]));
    }

    #[test]
    fn test_notify_capability_permission() {
        let action = Action::Notify {
            summary: "Now Playing".to_string(),
            body: "Artist - Title".to_string(),
        };
        assert_eq!(action.permission(), ActionPermission::Capability(Capability::Notify));
        assert_eq!(action.required_capability(), Some(Capability::Notify));

        let plugin = ActionSource::Plugin("org.tunotron.notifier".into());
        assert!(!plugin.is_permitted(&action, &[]));
        assert!(!plugin.is_permitted(&action, &[Capability::PlaybackControl, Capability::UiOverlay]));
        assert!(plugin.is_permitted(&action, &[Capability::Notify]));
    }

    #[test]
    fn test_cross_plugin_action_forgery_denied() {
        let target_action = Action::Plugin {
            plugin_id: "org.tunotron.target".to_string(),
            name: "privileged_hook".to_string(),
            payload: serde_json::json!({}),
        };

        // User keybinding or internal event can invoke any plugin's action
        assert!(ActionSource::User.is_permitted(&target_action, &[]));
        assert!(ActionSource::Internal.is_permitted(&target_action, &[]));

        // Plugin targeting itself is permitted
        let self_source = ActionSource::Plugin("org.tunotron.target".to_string());
        assert!(self_source.is_permitted(&target_action, &[]));

        // Rogue or sibling plugin targeting another plugin's namespace is strictly DENIED
        let rogue_source = ActionSource::Plugin("org.tunotron.rogue".to_string());
        assert!(!rogue_source.is_permitted(&target_action, &[]));
        assert!(!rogue_source.is_permitted(&target_action, &[Capability::PlaybackControl, Capability::UiOverlay, Capability::Notify]));
    }
}
