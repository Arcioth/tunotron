use std::path::{Path, PathBuf};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use tokio::sync::mpsc;
use tracing::debug;

use crate::event::{AppEvent, MetadataPatch, ScannerEvent};

pub struct Scanner;

impl Scanner {
    pub fn scan_paths_in_background(
        paths: Vec<PathBuf>,
        event_tx: mpsc::Sender<AppEvent>,
    ) {
        if paths.is_empty() {
            return;
        }

        tokio::task::spawn_blocking(move || {
            let mut batch = Vec::new();

            for path in paths {
                if let Some(patch) = Self::read_tags_blocking(&path) {
                    batch.push(patch);
                    if batch.len() >= 20 {
                        let _ = event_tx.blocking_send(AppEvent::Scanner(ScannerEvent::Batch(
                            std::mem::take(&mut batch),
                        )));
                    }
                }
            }

            if !batch.is_empty() {
                let _ = event_tx.blocking_send(AppEvent::Scanner(ScannerEvent::Batch(batch)));
            }
        });
    }

    pub fn read_tags_blocking(path: &Path) -> Option<MetadataPatch> {
        let tagged_file = Probe::open(path).ok()?.read().ok()?;
        let properties = tagged_file.properties();
        let duration_sec = properties.duration().as_secs_f64();

        let mut title = None;
        let mut artist = None;
        let mut album = None;
        let mut track_number = None;

        if let Some(tag) = tagged_file.primary_tag().or_else(|| tagged_file.first_tag()) {
            if let Some(t) = tag.title() {
                let s = t.trim();
                if !s.is_empty() {
                    title = Some(s.to_string());
                }
            }
            if let Some(a) = tag.artist() {
                let s = a.trim();
                if !s.is_empty() {
                    artist = Some(s.to_string());
                }
            }
            if let Some(al) = tag.album() {
                let s = al.trim();
                if !s.is_empty() {
                    album = Some(s.to_string());
                }
            }
            track_number = tag.track();
        } else {
            debug!("Could not read audio tags for: {}", path.display());
        }

        Some(MetadataPatch {
            path: path.to_path_buf(),
            title,
            artist,
            album,
            duration_sec,
            track_number,
        })
    }
}
