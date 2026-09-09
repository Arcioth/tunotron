use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::action::Action;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl From<KeyEvent> for KeyChord {
    fn from(evt: KeyEvent) -> Self {
        Self {
            code: evt.code,
            modifiers: evt.modifiers,
        }
    }
}

pub struct KeyMap {
    mappings: HashMap<Vec<KeyChord>, Action>,
}

impl Default for KeyMap {
    fn default() -> Self {
        Self::default_bindings()
    }
}

impl KeyMap {
    pub fn default_bindings() -> Self {
        let mut map = Self {
            mappings: HashMap::new(),
        };

        // Navigation (Vim motions)
        map.bind_simple(KeyCode::Char('j'), Action::MoveDown(1));
        map.bind_simple(KeyCode::Down, Action::MoveDown(1));
        map.bind_simple(KeyCode::Char('k'), Action::MoveUp(1));
        map.bind_simple(KeyCode::Up, Action::MoveUp(1));
        map.bind_chord(vec![KeyCode::Char('g'), KeyCode::Char('g')], Action::MoveToTop);
        map.bind_simple(KeyCode::Char('G'), Action::MoveToBottom);

        // Page navigation
        map.bind_simple(KeyCode::Char('d'), Action::HalfPageDown);
        map.bind_simple(KeyCode::Char('u'), Action::HalfPageUp);

        // Directory Navigation
        map.bind_simple(KeyCode::Enter, Action::PlaySelected);
        map.bind_simple(KeyCode::Backspace, Action::GoToParentDirectory);

        // Playback controls
        map.bind_simple(KeyCode::Char('c'), Action::TogglePause);
        map.bind_simple(KeyCode::Char(' '), Action::TogglePause);
        map.bind_simple(KeyCode::Char('v'), Action::Stop);
        map.bind_simple(KeyCode::Char('b'), Action::NextTrack);
        map.bind_simple(KeyCode::Char('z'), Action::PrevTrack);

        // Seeking (Right / Left arrow)
        map.bind_simple(KeyCode::Right, Action::Seek(5));
        map.bind_simple(KeyCode::Left, Action::Seek(-5));

        // Volume (+ / -)
        map.bind_simple(KeyCode::Char('+'), Action::VolumeDelta(5));
        map.bind_simple(KeyCode::Char('='), Action::VolumeDelta(5));
        map.bind_simple(KeyCode::Char('-'), Action::VolumeDelta(-5));

        // Playback Modes: Loop & Shuffle
        map.bind_simple(KeyCode::Char('m'), Action::CycleLoopMode);
        map.bind_simple(KeyCode::Char('s'), Action::ToggleShuffle);

        // Directory & UI
        map.bind_simple(KeyCode::Char('.'), Action::LocatePlayingTrack);
        map.bind_simple(KeyCode::Char('Z'), Action::ToggleDensity);
        map.bind_simple(KeyCode::Char('r'), Action::ReloadDirectory);
        map.bind_simple(KeyCode::Char('?'), Action::ToggleHelp);
        map.bind_simple(KeyCode::Esc, Action::CloseTopWindow);
        map.bind_simple(KeyCode::Char('q'), Action::Quit);

        map
    }

    pub fn bind_simple(&mut self, code: KeyCode, action: Action) {
        self.mappings.insert(
            vec![KeyChord {
                code,
                modifiers: KeyModifiers::NONE,
            }],
            action,
        );
    }

    pub fn bind_chord(&mut self, codes: Vec<KeyCode>, action: Action) {
        let chords = codes
            .into_iter()
            .map(|code| KeyChord {
                code,
                modifiers: KeyModifiers::NONE,
            })
            .collect();
        self.mappings.insert(chords, action);
    }

    pub fn lookup(&self, chords: &[KeyChord]) -> Option<Action> {
        self.mappings.get(chords).cloned()
    }

    pub fn is_prefix(&self, chords: &[KeyChord]) -> bool {
        self.mappings
            .keys()
            .any(|k| k.starts_with(chords) && k.len() > chords.len())
    }
}

pub struct KeySequenceStateMachine {
    pending: Vec<KeyChord>,
    last_key_time: Instant,
    timeout: Duration,
}

impl Default for KeySequenceStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl KeySequenceStateMachine {
    pub fn new() -> Self {
        Self {
            pending: Vec::new(),
            last_key_time: Instant::now(),
            timeout: Duration::from_millis(600),
        }
    }

    pub fn feed(&mut self, chord: KeyChord, keymap: &KeyMap) -> Option<Action> {
        if self.last_key_time.elapsed() > self.timeout {
            self.pending.clear();
        }
        self.last_key_time = Instant::now();
        self.pending.push(chord);

        // Check if current chord sequence is an exact match
        if let Some(action) = keymap.lookup(&self.pending) {
            self.pending.clear();
            return Some(action);
        }

        // Check if current chord sequence is a prefix of a longer chord
        if keymap.is_prefix(&self.pending) {
            None
        } else {
            self.pending.clear();
            None
        }
    }
}
