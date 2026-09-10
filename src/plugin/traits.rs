use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use crate::action::Action;
use crate::app::PlayState;
use crate::library::TrackId;
use super::manifest::PluginManifest;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PluginEvent {
    TrackChanged {
        track_id: TrackId,
        title: String,
        artist: String,
        album: String,
        duration_sec: f64,
        path: PathBuf,
    },
    PlaybackStopped,
    PlayStateChanged(PlayState),
    TimePos(f64),
}

pub trait Plugin: Send {
    fn manifest(&self) -> &PluginManifest;

    fn on_load(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn on_event(&mut self, _event: &PluginEvent) -> Vec<Action> {
        Vec::new()
    }

    fn on_action(&mut self, _name: &str, _payload: &serde_json::Value) -> Vec<Action> {
        Vec::new()
    }

    fn on_unload(&mut self) {}
}
