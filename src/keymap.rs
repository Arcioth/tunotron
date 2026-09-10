use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::action::Action;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyChord {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Empty key chord string".to_string());
        }

        // Single character edge cases (e.g. "+", "-", " ", "?")
        if s.chars().count() == 1 {
            let c = s.chars().next().unwrap();
            return Ok(KeyChord {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
            });
        }

        let mut modifiers = KeyModifiers::NONE;
        let mut key_part = s;

        // Extract modifiers iteratively
        loop {
            let lower = key_part.to_ascii_lowercase();
            if lower.starts_with("ctrl+") || lower.starts_with("control+") {
                modifiers.insert(KeyModifiers::CONTROL);
                key_part = &key_part[key_part.find('+').unwrap() + 1..];
            } else if lower.starts_with("ctrl-") || lower.starts_with("control-") {
                modifiers.insert(KeyModifiers::CONTROL);
                key_part = &key_part[key_part.find('-').unwrap() + 1..];
            } else if lower.starts_with("alt+") || lower.starts_with("alt-") {
                modifiers.insert(KeyModifiers::ALT);
                key_part = &key_part[4..];
            } else if lower.starts_with("shift+") || lower.starts_with("shift-") {
                modifiers.insert(KeyModifiers::SHIFT);
                key_part = &key_part[6..];
            } else if lower.starts_with("super+") || lower.starts_with("super-") {
                modifiers.insert(KeyModifiers::SUPER);
                key_part = &key_part[6..];
            } else {
                break;
            }
        }

        let mut code = match key_part.to_ascii_lowercase().as_str() {
            "enter" | "return" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "backspace" => KeyCode::Backspace,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "space" => KeyCode::Char(' '),
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "page_up" => KeyCode::PageUp,
            "pagedown" | "page_down" => KeyCode::PageDown,
            "delete" | "del" => KeyCode::Delete,
            "insert" | "ins" => KeyCode::Insert,
            "f1" => KeyCode::F(1),
            "f2" => KeyCode::F(2),
            "f3" => KeyCode::F(3),
            "f4" => KeyCode::F(4),
            "f5" => KeyCode::F(5),
            "f6" => KeyCode::F(6),
            "f7" => KeyCode::F(7),
            "f8" => KeyCode::F(8),
            "f9" => KeyCode::F(9),
            "f10" => KeyCode::F(10),
            "f11" => KeyCode::F(11),
            "f12" => KeyCode::F(12),
            other => {
                let mut chars = other.chars();
                if let (Some(_c), None) = (chars.next(), chars.next()) {
                    let original_c = key_part.chars().next().unwrap();
                    KeyCode::Char(original_c)
                } else {
                    return Err(format!("Unrecognized key code: '{}'", key_part));
                }
            }
        };

        if let KeyCode::Char(c) = code {
            if modifiers.contains(KeyModifiers::SHIFT) && c.is_ascii_lowercase() {
                code = KeyCode::Char(c.to_ascii_uppercase());
            }
            modifiers.remove(KeyModifiers::SHIFT);
        }

        Ok(KeyChord { code, modifiers })
    }

    pub fn parse_sequence(s: &str) -> Result<Vec<Self>, String> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Empty key sequence".to_string());
        }

        if s == "," || s == " " {
            return Ok(vec![Self::parse(s)?]);
        }

        let delimiter = if s.contains(',') {
            ','
        } else if s.contains(' ') {
            ' '
        } else {
            return Ok(vec![Self::parse(s)?]);
        };

        let mut chords = Vec::new();
        for token in s.split(delimiter) {
            let token = token.trim();
            if !token.is_empty() {
                chords.push(Self::parse(token)?);
            }
        }

        if chords.is_empty() {
            Err(format!("Invalid key sequence: '{}'", s))
        } else {
            Ok(chords)
        }
    }
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

    pub fn is_reserved(chords: &[KeyChord]) -> bool {
        if chords.is_empty() {
            return false;
        }

        // Quit: 'q' or 'Q' (no modifiers) or Ctrl+C
        if chords.len() == 1 {
            let c = &chords[0];
            if c.modifiers.is_empty() && matches!(c.code, KeyCode::Char('q') | KeyCode::Char('Q')) {
                return true;
            }
            if c.modifiers == KeyModifiers::CONTROL && matches!(c.code, KeyCode::Char('c') | KeyCode::Char('C')) {
                return true;
            }
            // Window close: Esc
            if c.modifiers.is_empty() && matches!(c.code, KeyCode::Esc) {
                return true;
            }
            // Core table navigation: j, k, Up, Down, G, Enter, Backspace
            if c.modifiers.is_empty() {
                match c.code {
                    KeyCode::Char('j')
                    | KeyCode::Char('k')
                    | KeyCode::Down
                    | KeyCode::Up
                    | KeyCode::Char('G')
                    | KeyCode::Enter
                    | KeyCode::Backspace => return true,
                    _ => {}
                }
            }
        }

        // Vim top motion: gg
        if chords.len() == 2
            && chords[0].modifiers.is_empty()
            && chords[1].modifiers.is_empty()
            && chords[0].code == KeyCode::Char('g')
            && chords[1].code == KeyCode::Char('g')
        {
            return true;
        }

        false
    }

    pub fn register_plugin_chord(
        &mut self,
        plugin_id: &str,
        key_str: &str,
        action_name: &str,
        has_keybind_capability: bool,
    ) -> Result<(), String> {
        if !has_keybind_capability {
            return Err(format!(
                "Plugin '{}' cannot bind key '{}': missing Capability::KeyBind",
                plugin_id, key_str
            ));
        }

        let chords = KeyChord::parse_sequence(key_str)?;
        if chords.is_empty() {
            return Err(format!("Empty key sequence: '{}'", key_str));
        }

        if Self::is_reserved(&chords) {
            return Err(format!(
                "Plugin '{}' attempted to bind reserved key sequence '{}'",
                plugin_id, key_str
            ));
        }

        if self.mappings.contains_key(&chords) {
            return Err(format!(
                "Key sequence '{}' is already bound; skipping duplicate registration for plugin '{}'",
                key_str, plugin_id
            ));
        }

        let action = Action::Plugin {
            plugin_id: plugin_id.to_string(),
            name: action_name.to_string(),
            payload: serde_json::Value::Null,
        };

        self.mappings.insert(chords, action);
        Ok(())
    }

    pub fn register_plugin_bindings(&mut self, plugin_mgr: &crate::plugin::PluginManager) {
        for manifest in plugin_mgr.manifests() {
            let has_keybind_cap = manifest.capabilities.contains(&crate::action::Capability::KeyBind);
            for (key_str, action_name) in &manifest.keybinds {
                match self.register_plugin_chord(&manifest.id, key_str, action_name, has_keybind_cap) {
                    Ok(()) => {
                        tracing::info!(
                            "Bound key '{}' to plugin '{}' action '{}'",
                            key_str,
                            manifest.id,
                            action_name
                        );
                    }
                    Err(err) => {
                        tracing::warn!("{}", err);
                    }
                }
            }
        }
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

    #[test]
    fn test_key_chord_parse_simple_and_modifiers() {
        // Simple keys
        let y = KeyChord::parse("y").unwrap();
        assert_eq!(y.code, KeyCode::Char('y'));
        assert_eq!(y.modifiers, KeyModifiers::NONE);

        let space = KeyChord::parse("space").unwrap();
        assert_eq!(space.code, KeyCode::Char(' '));

        let enter = KeyChord::parse("enter").unwrap();
        assert_eq!(enter.code, KeyCode::Enter);

        let f5 = KeyChord::parse("F5").unwrap();
        assert_eq!(f5.code, KeyCode::F(5));

        // Modifiers
        let ctrl_y = KeyChord::parse("ctrl+y").unwrap();
        assert_eq!(ctrl_y.code, KeyCode::Char('y'));
        assert_eq!(ctrl_y.modifiers, KeyModifiers::CONTROL);

        let alt_enter = KeyChord::parse("alt+enter").unwrap();
        assert_eq!(alt_enter.code, KeyCode::Enter);
        assert_eq!(alt_enter.modifiers, KeyModifiers::ALT);

        let ctrl_shift_tab = KeyChord::parse("ctrl+shift+tab").unwrap();
        assert_eq!(ctrl_shift_tab.code, KeyCode::Tab);
        assert_eq!(ctrl_shift_tab.modifiers, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    }

    #[test]
    fn test_key_chord_parse_sequence() {
        let chords = KeyChord::parse_sequence("g g").unwrap();
        assert_eq!(chords.len(), 2);
        assert_eq!(chords[0].code, KeyCode::Char('g'));
        assert_eq!(chords[1].code, KeyCode::Char('g'));

        let comma_seq = KeyChord::parse_sequence("ctrl+x,ctrl+s").unwrap();
        assert_eq!(comma_seq.len(), 2);
        assert_eq!(comma_seq[0].modifiers, KeyModifiers::CONTROL);
        assert_eq!(comma_seq[1].modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn test_plugin_keybinding_registration_and_dispatch() {
        let mut keymap = KeyMap::default();

        // Register custom key 'y' with KeyBind capability
        let res = keymap.register_plugin_chord("com.test.lyrics", "y", "toggle_lyrics", true);
        assert!(res.is_ok());

        let y_chord = KeyChord::parse("y").unwrap();
        let looked_up = keymap.lookup(&[y_chord]);
        assert_eq!(
            looked_up,
            Some(Action::Plugin {
                plugin_id: "com.test.lyrics".to_string(),
                name: "toggle_lyrics".to_string(),
                payload: serde_json::Value::Null,
            })
        );
    }

    #[test]
    fn test_plugin_keybinding_reserved_rejection() {
        let mut keymap = KeyMap::default();

        // 'q' is strictly reserved for host quit
        let res_q = keymap.register_plugin_chord("com.test.rogue", "q", "rogue_quit", true);
        assert!(res_q.is_err());

        // 'Esc' is strictly reserved for host window close
        let res_esc = keymap.register_plugin_chord("com.test.rogue", "esc", "rogue_esc", true);
        assert!(res_esc.is_err());

        // 'j' is strictly reserved for navigation
        let res_j = keymap.register_plugin_chord("com.test.rogue", "j", "rogue_down", true);
        assert!(res_j.is_err());

        // 'gg' is strictly reserved for vim top motion
        let res_gg = keymap.register_plugin_chord("com.test.rogue", "g g", "rogue_top", true);
        assert!(res_gg.is_err());
    }

    #[test]
    fn test_plugin_keybinding_missing_capability_rejection() {
        let mut keymap = KeyMap::default();

        // Attempting to register keybind without KeyBind capability is blocked
        let res = keymap.register_plugin_chord("com.test.unauthorized", "y", "action", false);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("missing Capability::KeyBind"));
    }
}

