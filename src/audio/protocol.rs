#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct MpvRequest {
    pub command: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum MpvIncoming {
    Event(MpvEvent),
    Response(MpvResponse),
}

#[derive(Debug, Deserialize)]
pub struct MpvResponse {
    pub error: String,
    pub request_id: Option<u64>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "event")]
pub enum MpvEvent {
    #[serde(rename = "property-change")]
    PropertyChange {
        id: u64,
        name: String,
        data: Option<serde_json::Value>,
    },
    #[serde(rename = "end-file")]
    EndFile {
        reason: Option<String>,
        file_error: Option<String>,
        playlist_entry_id: Option<i64>,
    },
    #[serde(rename = "file-loaded")]
    FileLoaded,
    #[serde(rename = "playback-restart")]
    PlaybackRestart,
    #[serde(rename = "start-file")]
    StartFile { playlist_entry_id: Option<i64> },
    #[serde(rename = "idle")]
    Idle,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone)]
pub enum MpvCommand {
    LoadFile { path: String, replace: bool },
    TogglePause,
    SetPause(bool),
    Seek { seconds: f64, relative: bool },
    SetVolume(f64),
    Stop,
    Quit,
}

impl MpvCommand {
    pub fn to_request(&self, req_id: Option<u64>) -> MpvRequest {
        match self {
            MpvCommand::LoadFile { path, replace } => MpvRequest {
                command: vec![
                    "loadfile".into(),
                    path.clone().into(),
                    if *replace { "replace".into() } else { "append".into() },
                ],
                request_id: req_id,
            },
            MpvCommand::TogglePause => MpvRequest {
                command: vec!["cycle".into(), "pause".into()],
                request_id: req_id,
            },
            MpvCommand::SetPause(paused) => MpvRequest {
                command: vec!["set_property".into(), "pause".into(), (*paused).into()],
                request_id: req_id,
            },
            MpvCommand::Seek { seconds, relative } => MpvRequest {
                command: vec![
                    "seek".into(),
                    (*seconds).into(),
                    if *relative {
                        "relative".into()
                    } else {
                        "absolute+exact".into()
                    },
                ],
                request_id: req_id,
            },
            MpvCommand::SetVolume(vol) => MpvRequest {
                command: vec!["set_property".into(), "volume".into(), (*vol).into()],
                request_id: req_id,
            },
            MpvCommand::Stop => MpvRequest {
                command: vec!["stop".into()],
                request_id: req_id,
            },
            MpvCommand::Quit => MpvRequest {
                command: vec!["quit".into()],
                request_id: req_id,
            },
        }
    }
}
