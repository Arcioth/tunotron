use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: usize,
    pub path: PathBuf,
    pub filename: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_sec: f64,
    pub track_number: Option<u32>,
}

impl Track {
    pub fn new(id: usize, path: PathBuf) -> Self {
        let filename = path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        let fallback_title = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| filename.clone());

        Self {
            id,
            path,
            filename,
            title: fallback_title,
            artist: "Unknown Artist".to_string(),
            album: "Unknown Album".to_string(),
            duration_sec: 0.0,
            track_number: None,
        }
    }

    pub fn formatted_duration(&self) -> String {
        let total_sec = self.duration_sec.round() as u64;
        let mins = total_sec / 60;
        let secs = total_sec % 60;
        format!("{:02}:{:02}", mins, secs)
    }

    pub fn is_audio_file(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(
                ext.to_lowercase().as_str(),
                "mp3" | "flac" | "ogg" | "opus" | "m4a" | "aac" | "wav" | "wma" | "alac" | "aiff"
            )
        } else {
            false
        }
    }
}
