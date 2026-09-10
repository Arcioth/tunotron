use std::path::PathBuf;
use crossterm::event::KeyEvent;
use crate::audio::protocol::MpvEvent;

#[derive(Debug, Clone)]
pub struct MetadataPatch {
    pub path: PathBuf,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_sec: f64,
    pub track_number: Option<u32>,
}

#[derive(Debug)]
pub enum ScannerEvent {
    Batch(Vec<MetadataPatch>),
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Mpv(MpvEvent),
    TimePos(f64),
    DirectoryLoaded {
        dir: PathBuf,
        items: Vec<crate::library::BrowserEntry>,
    },
    Scanner(ScannerEvent),
    Action(crate::action::ActionEnvelope),
    Tick,
}
