use crossterm::event::KeyEvent;
use crate::audio::protocol::MpvEvent;
use crate::library::track::Track;

#[derive(Debug)]
#[allow(dead_code)]
pub enum ScannerEvent {
    Batch(Vec<Track>),
    Finished { total_tracks: usize },
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Mpv(MpvEvent),
    Scanner(ScannerEvent),
    Tick,
}
