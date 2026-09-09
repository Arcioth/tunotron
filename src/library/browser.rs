use std::fs;
use std::path::{Path, PathBuf};
use crate::library::scanner::Scanner;
use crate::library::track::Track;

#[derive(Debug, Clone)]
pub enum BrowserEntry {
    ParentDir(PathBuf),
    Directory {
        name: String,
        path: PathBuf,
    },
    AudioTrack(Track),
}

#[allow(dead_code)]
impl BrowserEntry {
    pub fn is_dir(&self) -> bool {
        matches!(self, BrowserEntry::ParentDir(_) | BrowserEntry::Directory { .. })
    }

    pub fn path(&self) -> &Path {
        match self {
            BrowserEntry::ParentDir(p) => p,
            BrowserEntry::Directory { path, .. } => path,
            BrowserEntry::AudioTrack(t) => &t.path,
        }
    }
}

pub fn read_directory(dir: &Path) -> Vec<BrowserEntry> {
    let mut items = Vec::new();

    if let Some(parent) = dir.parent() {
        items.push(BrowserEntry::ParentDir(parent.to_path_buf()));
    }

    if let Ok(entries) = fs::read_dir(dir) {
        let mut dirs = Vec::new();
        let mut tracks = Vec::new();
        let mut track_id = 0;

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                dirs.push(BrowserEntry::Directory { name, path });
            } else if Track::is_audio_file(&path) {
                track_id += 1;
                let track = Scanner::read_metadata_sync(track_id, &path);
                tracks.push(track);
            }
        }

        // Sort directories alphabetically
        dirs.sort_by(|a, b| match (a, b) {
            (BrowserEntry::Directory { name: a_name, .. }, BrowserEntry::Directory { name: b_name, .. }) => {
                a_name.to_lowercase().cmp(&b_name.to_lowercase())
            }
            _ => std::cmp::Ordering::Equal,
        });

        // Sort tracks by track number if available, then by title / filename
        tracks.sort_by(|a, b| {
            match (a.track_number, b.track_number) {
                (Some(an), Some(bn)) if an != bn => an.cmp(&bn),
                _ => a.filename.to_lowercase().cmp(&b.filename.to_lowercase()),
            }
        });

        items.extend(dirs);
        for track in tracks {
            items.push(BrowserEntry::AudioTrack(track));
        }
    }

    items
}
