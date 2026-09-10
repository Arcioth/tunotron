use super::manifest::PluginManifest;
use super::traits::{Plugin, PluginEvent};
use crate::action::Action;

pub struct TrackLoggerPlugin {
    manifest: PluginManifest,
}

impl TrackLoggerPlugin {
    pub fn new() -> Self {
        Self {
            manifest: PluginManifest::new(
                "builtin.track_logger",
                "Built-in Track Logger",
                env!("CARGO_PKG_VERSION"),
                "Observes track changes and logs them for diagnostics",
                vec![],
            ),
        }
    }
}

impl Default for TrackLoggerPlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Plugin for TrackLoggerPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn on_event(&mut self, event: &PluginEvent) -> Vec<Action> {
        match event {
            PluginEvent::TrackChanged { artist, title, duration_sec, .. } => {
                tracing::info!("Track change event: {} — {} ({:.1}s)", artist, title, duration_sec);
            }
            PluginEvent::PlaybackStopped => {
                tracing::info!("Playback stopped event received by plugin");
            }
            _ => {}
        }
        Vec::new()
    }
}
