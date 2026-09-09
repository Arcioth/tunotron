use std::path::{Path, PathBuf};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use tokio::sync::mpsc;
use walkdir::WalkDir;
use tracing::debug;

use super::track::Track;
use crate::event::{AppEvent, ScannerEvent};

pub struct Scanner;

#[allow(dead_code)]
impl Scanner {
    pub fn scan_directory_in_background(
        dir: PathBuf,
        event_tx: mpsc::UnboundedSender<AppEvent>,
    ) {
        tokio::task::spawn_blocking(move || {
            let mut tracks = Vec::new();
            let mut batch = Vec::new();
            let mut current_id = 0;

            for entry in WalkDir::new(&dir)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() && Track::is_audio_file(path) {
                    current_id += 1;
                    let track = Self::read_metadata_sync(current_id, path);
                    batch.push(track.clone());
                    tracks.push(track);

                    if batch.len() >= 20 {
                        let _ = event_tx.send(AppEvent::Scanner(ScannerEvent::Batch(std::mem::take(&mut batch))));
                    }
                }
            }

            if !batch.is_empty() {
                let _ = event_tx.send(AppEvent::Scanner(ScannerEvent::Batch(batch)));
            }

            let _ = event_tx.send(AppEvent::Scanner(ScannerEvent::Finished {
                total_tracks: tracks.len(),
            }));
        });
    }

    pub fn read_metadata_sync(id: usize, path: &Path) -> Track {
        let mut track = Track::new(id, path.to_path_buf());

        if let Ok(tagged_file) = Probe::open(path).and_then(|p| p.read()) {
            let properties = tagged_file.properties();
            track.duration_sec = properties.duration().as_secs_f64();

            if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
                if let Some(title) = tag.title() {
                    let s = title.trim();
                    if !s.is_empty() {
                        track.title = s.to_string();
                    }
                }
                if let Some(artist) = tag.artist() {
                    let s = artist.trim();
                    if !s.is_empty() {
                        track.artist = s.to_string();
                    }
                }
                if let Some(album) = tag.album() {
                    let s = album.trim();
                    if !s.is_empty() {
                        track.album = s.to_string();
                    }
                }
                track.track_number = tag.track();
            }
        } else {
            debug!("Could not read audio tags for: {}", path.display());
        }

        track
    }
}
