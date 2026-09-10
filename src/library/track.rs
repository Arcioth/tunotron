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
    pub duration_label: String,
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
            duration_label: "00:00".to_string(),
            track_number: None,
        }
    }

    pub fn set_duration(&mut self, sec: f64) {
        self.duration_sec = sec;
        self.duration_label = format_mmss(sec);
    }

    pub fn is_audio_file(path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            ext.eq_ignore_ascii_case("mp3")
                || ext.eq_ignore_ascii_case("flac")
                || ext.eq_ignore_ascii_case("ogg")
                || ext.eq_ignore_ascii_case("opus")
                || ext.eq_ignore_ascii_case("m4a")
                || ext.eq_ignore_ascii_case("aac")
                || ext.eq_ignore_ascii_case("wav")
                || ext.eq_ignore_ascii_case("wma")
                || ext.eq_ignore_ascii_case("alac")
                || ext.eq_ignore_ascii_case("aiff")
        } else {
            false
        }
    }
}

pub fn format_mmss(seconds: f64) -> String {
    let total_sec = seconds.round() as u64;
    let mins = total_sec / 60;
    let secs = total_sec % 60;
    format!("{:02}:{:02}", mins, secs)
}
