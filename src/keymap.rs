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
        let mut modifiers = evt.modifiers;
        if matches!(evt.code, KeyCode::Char(_)) {
            modifiers.remove(KeyModifiers::SHIFT);
        }
        Self {
            code: evt.code,
            modifiers,
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
        self.pending.push(chord.clone());

        // Check if current chord sequence is an exact match
        if let Some(action) = keymap.lookup(&self.pending) {
            self.pending.clear();
            return Some(action);
        }

        // Check if current chord sequence is a prefix of a longer chord
        if keymap.is_prefix(&self.pending) {
            return None;
        }

        // Mismatch: retry the last key as a fresh sequence (avoids swallowing key after prefix attempt)
        self.pending.clear();
        if let Some(action) = keymap.lookup(std::slice::from_ref(&chord)) {
            return Some(action);
        }
        if keymap.is_prefix(std::slice::from_ref(&chord)) {
            self.pending.push(chord);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shift_normalization() {
        let key_with_shift = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT);
        let chord = KeyChord::from(key_with_shift);
        assert_eq!(chord.modifiers, KeyModifiers::NONE);
        assert_eq!(chord.code, KeyCode::Char('?'));

        let key_cap_g = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        let chord_g = KeyChord::from(key_cap_g);
        assert_eq!(chord_g.modifiers, KeyModifiers::NONE);
        assert_eq!(chord_g.code, KeyCode::Char('G'));
    }

    #[test]
    fn test_key_chords_and_prefix_retry() {
        let keymap = KeyMap::default();
        let mut sm = KeySequenceStateMachine::new();

        // Feed 'g' -> prefix of 'gg', should return None
        let g_chord = KeyChord {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(sm.feed(g_chord.clone(), &keymap), None);

        // Feed another 'g' -> exact match for Action::MoveToTop
        assert_eq!(sm.feed(g_chord.clone(), &keymap), Some(Action::MoveToTop));

        // Feed 'g' then 'j' -> mismatch should NOT swallow 'j', it should execute MoveDown(1)
        assert_eq!(sm.feed(g_chord.clone(), &keymap), None);
        let j_chord = KeyChord {
            code: KeyCode::Char('j'),
            modifiers: KeyModifiers::NONE,
        };
        assert_eq!(sm.feed(j_chord, &keymap), Some(Action::MoveDown(1)));
    }
}

