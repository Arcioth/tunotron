use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use mlua::{HookTriggers, Lua, LuaSerdeExt, Table, Value, VmState};
use crate::action::{Action, Capability};
use super::manifest::PluginManifest;
use super::traits::{Plugin, PluginEvent};

/// Maximum VM instructions allowed per callback (~1-2ms of CPU time).
/// Prevents rogue or infinite-looping scripts from starving the Tokio reactor or freezing the UI.
const MAX_INSTRUCTION_BUDGET: u32 = 250_000;
const INSTRUCTION_CHECK_INTERVAL: u32 = 2_500;

pub struct LuaPlugin {
    pub manifest: PluginManifest,
    lua: Lua,
    instruction_counter: Arc<AtomicU32>,
}

impl LuaPlugin {
    pub fn from_file(path: &Path, music_root: Option<&Path>) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read Lua plugin at {}: {}", path.display(), e))?;
        Self::from_script_with_root(&content, path.to_string_lossy().as_ref(), music_root)
    }

    pub fn from_script(script: &str, chunk_name: &str) -> Result<Self, String> {
        Self::from_script_with_root(script, chunk_name, None)
    }

    pub fn from_script_with_root(
        script: &str,
        chunk_name: &str,
        music_root: Option<&Path>,
    ) -> Result<Self, String> {
        let lua = Lua::new();

        // 1. Sandbox setup: strip dangerous host process controls and cap memory (16MB)
        let _ = lua.set_memory_limit(16 * 1024 * 1024);
        setup_sandbox(&lua)?;

        // 2. CPU instruction budget hook: abort any callback exceeding MAX_INSTRUCTION_BUDGET
        let instruction_counter = Arc::new(AtomicU32::new(0));
        let counter_for_hook = Arc::clone(&instruction_counter);

        lua.set_hook(
            HookTriggers::new().every_nth_instruction(INSTRUCTION_CHECK_INTERVAL),
            move |_lua, _debug| {
                if counter_for_hook.fetch_add(INSTRUCTION_CHECK_INTERVAL, Ordering::Relaxed) >= MAX_INSTRUCTION_BUDGET {
                    return Err(mlua::Error::RuntimeError(
                        "Script instruction budget exceeded: terminated to prevent CPU lockup".to_string(),
                    ));
                }
                Ok(VmState::Continue)
            },
        );

        // 3. Inject `tunotron` global host module
        setup_host_globals(&lua)?;

        // 4. Load script chunk (reset budget counter first)
        instruction_counter.store(0, Ordering::Relaxed);
        let plugin_tbl: Table = lua
            .load(script)
            .set_name(chunk_name)
            .eval()
            .map_err(|e| format!("Error executing Lua script '{}': {}", chunk_name, e))?;

        // 5. Extract manifest table
        let manifest_tbl: Table = plugin_tbl
            .get("manifest")
            .map_err(|e| format!("Plugin '{}' must export a 'manifest' table: {}", chunk_name, e))?;

        let id: String = manifest_tbl
            .get("id")
            .map_err(|_| format!("Plugin '{}' manifest missing required 'id' field", chunk_name))?;

        let name: String = manifest_tbl.get("name").unwrap_or_else(|_| id.clone());
        let version: String = manifest_tbl.get("version").unwrap_or_else(|_| "0.1.0".to_string());
        let description: String = manifest_tbl.get("description").unwrap_or_default();
        let api_version: Option<String> = manifest_tbl.get("api_version").ok();

        let mut capabilities = Vec::new();
        if let Ok(caps_tbl) = manifest_tbl.get::<Table>("capabilities") {
            for s in caps_tbl.sequence_values::<String>().flatten() {
                if let Ok(cap) = s.parse::<Capability>() {
                    capabilities.push(cap);
                } else {
                    tracing::warn!("Plugin '{}' requested unrecognized capability '{}'", id, s);
                }
            }
        }

        let mut keybinds = std::collections::HashMap::new();
        if let Ok(keybinds_tbl) = manifest_tbl.get::<Table>("keybinds") {
            for pair in keybinds_tbl.pairs::<String, String>().flatten() {
                keybinds.insert(pair.0, pair.1);
            }
        }

        let mut manifest = PluginManifest::new(id, name, version, description, capabilities).with_keybinds(keybinds);
        if let Some(av) = api_version {
            manifest = manifest.with_api_version(av);
        }

        // 6. Attach capability-gated host APIs (e.g. jailed filesystem reader, desktop notifications)
        attach_jailed_fs_api(&lua, &manifest, music_root)?;
        attach_notify_api(&lua, &manifest)?;

        // Store plugin table in Lua registry so we can retrieve its callbacks
        let _ = lua.set_named_registry_value("__tunotron_plugin_table", plugin_tbl);

        Ok(Self {
            manifest,
            lua,
            instruction_counter,
        })
    }

    fn plugin_table(&self) -> Result<Table, String> {
        self.lua
            .named_registry_value("__tunotron_plugin_table")
            .map_err(|e| format!("Failed to get plugin table from registry: {}", e))
    }
}

fn setup_sandbox(lua: &Lua) -> Result<(), String> {
    let globals = lua.globals();

    // Restrict `os` module: keep only safe math/time functions
    if let Ok(orig_os) = globals.get::<Table>("os") {
        let safe_os = lua.create_table().map_err(|e| e.to_string())?;
        if let Ok(clock) = orig_os.get::<mlua::Function>("clock") {
            let _ = safe_os.set("clock", clock);
        }
        if let Ok(time) = orig_os.get::<mlua::Function>("time") {
            let _ = safe_os.set("time", time);
        }
        if let Ok(difftime) = orig_os.get::<mlua::Function>("difftime") {
            let _ = safe_os.set("difftime", difftime);
        }
        let _ = globals.set("os", safe_os);
    }

    // Disable package.loadlib to prevent loading unsafe external native C libraries
    if let Ok(pkg) = globals.get::<Table>("package") {
        let _ = pkg.set("loadlib", Value::Nil);
    }

    Ok(())
}

fn setup_host_globals(lua: &Lua) -> Result<(), String> {
    let globals = lua.globals();
    let tunotron_tbl = lua.create_table().map_err(|e| e.to_string())?;

    let log_fn = lua
        .create_function(|_, msg: String| {
            tracing::info!("[Lua] {}", msg);
            Ok(())
        })
        .map_err(|e| e.to_string())?;

    let warn_fn = lua
        .create_function(|_, msg: String| {
            tracing::warn!("[Lua] {}", msg);
            Ok(())
        })
        .map_err(|e| e.to_string())?;

    let err_fn = lua
        .create_function(|_, msg: String| {
            tracing::error!("[Lua] {}", msg);
            Ok(())
        })
        .map_err(|e| e.to_string())?;

    let _ = tunotron_tbl.set("log", log_fn);
    let _ = tunotron_tbl.set("warn", warn_fn);
    let _ = tunotron_tbl.set("error", err_fn);
    let _ = tunotron_tbl.set("version", env!("CARGO_PKG_VERSION"));

    globals
        .set("tunotron", tunotron_tbl)
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn attach_jailed_fs_api(
    lua: &Lua,
    manifest: &PluginManifest,
    music_root: Option<&Path>,
) -> Result<(), String> {
    let globals = lua.globals();
    let tunotron_tbl: Table = globals.get("tunotron").map_err(|e| e.to_string())?;

    let has_read_cap = manifest.capabilities.contains(&Capability::FsJailRead);
    let root_buf = music_root.map(|p| {
        std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    });

    let root_for_read = root_buf.clone();
    let read_fn = lua
        .create_function(move |lua, path_str: String| {
            if !has_read_cap {
                return Ok((Value::Nil, Some("Permission denied: missing Capability::FsJailRead".to_string())));
            }
            let Some(root) = &root_for_read else {
                return Ok((Value::Nil, Some("Music directory not configured".to_string())));
            };

            let target_path = Path::new(&path_str);
            let resolved = if target_path.is_relative() {
                root.join(target_path)
            } else {
                target_path.to_path_buf()
            };

            let Some(canonical) = crate::library::browser::resolve_in_jail(root, &resolved) else {
                let is_escape = if target_path.is_absolute() && !target_path.starts_with(root) {
                    true
                } else if let Ok(canon_outside) = resolved.canonicalize() {
                    !canon_outside.starts_with(root)
                } else {
                    let mut depth: isize = 0;
                    let mut escaped = false;
                    for comp in target_path.components() {
                        match comp {
                            std::path::Component::ParentDir => {
                                depth -= 1;
                                if depth < 0 {
                                    escaped = true;
                                    break;
                                }
                            }
                            std::path::Component::Normal(_) => {
                                depth += 1;
                            }
                            _ => {}
                        }
                    }
                    escaped
                };

                if is_escape {
                    return Ok((Value::Nil, Some(format!("Access denied: path '{}' escapes music jail", path_str))));
                }

                return Ok((Value::Nil, Some(format!("File not found or unreadable: '{}'", path_str))));
            };

            if !canonical.is_file() {
                return Ok((Value::Nil, Some(format!("Path '{}' is not a regular file", path_str))));
            }

            let meta = match std::fs::metadata(&canonical) {
                Ok(m) => m,
                Err(e) => return Ok((Value::Nil, Some(format!("Failed to read file metadata: {}", e)))),
            };

            const MAX_READ_BYTES: u64 = 2 * 1024 * 1024; // 2 MB
            if meta.len() > MAX_READ_BYTES {
                return Ok((Value::Nil, Some(format!("File size ({} bytes) exceeds 2 MB limit", meta.len()))));
            }

            match std::fs::read_to_string(&canonical) {
                Ok(content) => Ok((Value::String(lua.create_string(&content)?), None)),
                Err(e) => Ok((Value::Nil, Some(format!("Failed to read file as UTF-8 text: {}", e)))),
            }
        })
        .map_err(|e| e.to_string())?;

    let root_for_exists = root_buf;
    let exists_fn = lua
        .create_function(move |_, path_str: String| {
            if !has_read_cap {
                return Ok(false);
            }
            let Some(root) = &root_for_exists else {
                return Ok(false);
            };

            let target_path = Path::new(&path_str);
            let resolved = if target_path.is_relative() {
                root.join(target_path)
            } else {
                target_path.to_path_buf()
            };

            if let Some(canonical) = crate::library::browser::resolve_in_jail(root, &resolved) {
                Ok(canonical.is_file())
            } else {
                Ok(false)
            }
        })
        .map_err(|e| e.to_string())?;

    tunotron_tbl.set("read_file", read_fn).map_err(|e| e.to_string())?;
    tunotron_tbl.set("file_exists", exists_fn).map_err(|e| e.to_string())?;

    Ok(())
}

fn attach_notify_api(lua: &Lua, manifest: &PluginManifest) -> Result<(), String> {
    let globals = lua.globals();
    let tunotron_tbl: Table = globals.get("tunotron").map_err(|e| e.to_string())?;

    let has_notify_cap = manifest.capabilities.contains(&Capability::Notify);
    let notify_fn = lua
        .create_function(move |_, (summary, body): (String, Option<String>)| {
            if !has_notify_cap {
                return Ok((false, Some("Permission denied: missing Capability::Notify".to_string())));
            }
            let body_str = body.unwrap_or_default();
            let res = std::process::Command::new("notify-send")
                .arg("--app-name=Tunotron")
                .arg(&summary)
                .arg(&body_str)
                .spawn();
            match res {
                Ok(_) => Ok((true, None::<String>)),
                Err(e) => Ok((false, Some(format!("Failed to spawn notify-send: {}", e)))),
            }
        })
        .map_err(|e| e.to_string())?;

    tunotron_tbl.set("notify", notify_fn).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_actions_from_value(val: Value) -> Vec<Action> {
    match val {
        Value::Table(ref tbl) => {
            // Check if table is a single action dictionary: { action = "..." }
            if tbl.contains_key("action").unwrap_or(false) {
                if let Some(act) = parse_table_action(tbl) {
                    return vec![act];
                }
            }

            // Otherwise treat as an array of actions
            let mut actions = Vec::new();
            for v in tbl.sequence_values::<Value>().flatten() {
                if let Some(act) = parse_single_action(&v) {
                    actions.push(act);
                }
            }
            actions
        }
        Value::String(_) => {
            if let Some(act) = parse_single_action(&val) {
                vec![act]
            } else {
                Vec::new()
            }
        }
        _ => Vec::new(),
    }
}

fn parse_single_action(val: &Value) -> Option<Action> {
    match val {
        Value::String(s) => match s.to_str().ok()?.as_ref() {
            "TogglePause" => Some(Action::TogglePause),
            "Stop" => Some(Action::Stop),
            "NextTrack" => Some(Action::NextTrack),
            "PrevTrack" => Some(Action::PrevTrack),
            "CycleLoopMode" => Some(Action::CycleLoopMode),
            "ToggleShuffle" => Some(Action::ToggleShuffle),
            "ToggleHelp" => Some(Action::ToggleHelp),
            "ToggleDensity" => Some(Action::ToggleDensity),
            "ReloadDirectory" => Some(Action::ReloadDirectory),
            "LocatePlayingTrack" => Some(Action::LocatePlayingTrack),
            "PlaySelected" => Some(Action::PlaySelected),
            "EnterDirectory" => Some(Action::EnterDirectory),
            "CloseModal" | "CloseTopWindow" => Some(Action::CloseTopWindow),
            "Quit" => Some(Action::Quit),
            _ => None,
        },
        Value::Table(tbl) => parse_table_action(tbl),
        _ => None,
    }
}

fn parse_table_action(tbl: &Table) -> Option<Action> {
    let action_name: String = tbl.get("action").ok()?;
    match action_name.as_str() {
        "TogglePause" => Some(Action::TogglePause),
        "Stop" => Some(Action::Stop),
        "NextTrack" => Some(Action::NextTrack),
        "PrevTrack" => Some(Action::PrevTrack),
        "CycleLoopMode" => Some(Action::CycleLoopMode),
        "ToggleShuffle" => Some(Action::ToggleShuffle),
        "ToggleHelp" => Some(Action::ToggleHelp),
        "ToggleDensity" => Some(Action::ToggleDensity),
        "ReloadDirectory" => Some(Action::ReloadDirectory),
        "LocatePlayingTrack" => Some(Action::LocatePlayingTrack),
        "PlaySelected" => Some(Action::PlaySelected),
        "EnterDirectory" => Some(Action::EnterDirectory),
        "CloseModal" | "CloseTopWindow" => Some(Action::CloseTopWindow),
        "ShowModal" => {
            let title: String = tbl.get("title").unwrap_or_else(|_| "Notification".to_string());
            let content: String = tbl.get("content").unwrap_or_default();
            Some(Action::ShowModal { title, content })
        }
        "Notify" => {
            let summary: String = tbl.get("summary").unwrap_or_else(|_| "Tunotron".to_string());
            let body: String = tbl.get("body").unwrap_or_default();
            Some(Action::Notify { summary, body })
        }
        "Quit" => Some(Action::Quit),
        "Seek" => {
            let seconds: i64 = tbl.get("seconds").unwrap_or(0);
            Some(Action::Seek(seconds))
        }
        "SeekRatio" => {
            let ratio: f64 = tbl.get("ratio").unwrap_or(0.0);
            Some(Action::SeekRatio(ratio))
        }
        "VolumeDelta" => {
            let delta: i8 = tbl.get("delta").unwrap_or(0);
            Some(Action::VolumeDelta(delta))
        }
        "SetVolume" => {
            let volume: f64 = tbl.get("volume").unwrap_or(100.0);
            Some(Action::SetVolume(volume))
        }
        "PlayTrackIndex" => {
            let index: usize = tbl.get("index").unwrap_or(0);
            Some(Action::PlayTrackIndex(index))
        }
        "MoveDown" => {
            let count: usize = tbl.get("count").unwrap_or(1);
            Some(Action::MoveDown(count))
        }
        "MoveUp" => {
            let count: usize = tbl.get("count").unwrap_or(1);
            Some(Action::MoveUp(count))
        }
        _ => None,
    }
}

impl Plugin for LuaPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn on_load(&mut self) -> Result<(), String> {
        self.instruction_counter.store(0, Ordering::Relaxed);
        let plugin_tbl = self.plugin_table()?;
        if let Ok(on_load_fn) = plugin_tbl.get::<mlua::Function>("on_load") {
            on_load_fn
                .call::<()>(())
                .map_err(|e| format!("Error in on_load callback for plugin '{}': {}", self.manifest.id, e))?;
        }
        Ok(())
    }

    fn on_event(&mut self, event: &PluginEvent) -> Vec<Action> {
        self.instruction_counter.store(0, Ordering::Relaxed);
        let Ok(plugin_tbl) = self.plugin_table() else { return Vec::new(); };

        let mut actions = Vec::new();
        let mut tick_handled = false;

        // 1. If this is a Tick event, check for dedicated `on_tick(position, duration)` hook
        if let PluginEvent::Tick { position, duration } = event {
            if let Ok(on_tick_fn) = plugin_tbl.get::<mlua::Function>("on_tick") {
                tick_handled = true;
                match on_tick_fn.call::<Value>((*position, *duration)) {
                    Ok(ret_val) => {
                        actions.extend(parse_actions_from_value(ret_val));
                    }
                    Err(e) => {
                        tracing::error!("Error in on_tick for plugin '{}': {}", self.manifest.id, e);
                    }
                }
            }
        }

        // 2. Call standard `on_event(event)` hook if defined (and not already handled by on_tick)
        if !tick_handled {
            if let Ok(on_event_fn) = plugin_tbl.get::<mlua::Function>("on_event") {
                let lua_event: Value = match self.lua.to_value(event) {
                    Ok(val) => val,
                    Err(e) => {
                        tracing::error!("Failed to serialize event for plugin '{}': {}", self.manifest.id, e);
                        return actions;
                    }
                };

                match on_event_fn.call::<Value>(lua_event) {
                    Ok(ret_val) => {
                        actions.extend(parse_actions_from_value(ret_val));
                    }
                    Err(e) => {
                        tracing::error!("Error in on_event for plugin '{}': {}", self.manifest.id, e);
                    }
                }
            }
        }

        actions
    }

    fn on_action(&mut self, name: &str, payload: &serde_json::Value) -> Vec<Action> {
        self.instruction_counter.store(0, Ordering::Relaxed);
        let Ok(plugin_tbl) = self.plugin_table() else { return Vec::new(); };
        let Ok(on_action_fn) = plugin_tbl.get::<mlua::Function>("on_action") else { return Vec::new(); };

        let lua_payload: Value = self.lua.to_value(payload).unwrap_or(Value::Nil);

        match on_action_fn.call::<Value>((name, lua_payload)) {
            Ok(ret_val) => parse_actions_from_value(ret_val),
            Err(e) => {
                tracing::error!("Error in on_action for plugin '{}': {}", self.manifest.id, e);
                Vec::new()
            }
        }
    }

    fn on_unload(&mut self) {
        self.instruction_counter.store(0, Ordering::Relaxed);
        if let Ok(plugin_tbl) = self.plugin_table() {
            if let Ok(on_unload_fn) = plugin_tbl.get::<mlua::Function>("on_unload") {
                let _ = on_unload_fn.call::<()>(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::library::TrackId;

    #[test]
    fn test_lua_plugin_load_and_manifest() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "com.test.sample",
                name = "Sample Plugin",
                version = "1.2.3",
                description = "A sample Lua plugin for testing",
                capabilities = { "PlaybackControl", "UiOverlay" }
            }
            function p.on_load()
                tunotron.log("Sample loaded")
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "sample.lua").unwrap();
        assert_eq!(plugin.manifest().id, "com.test.sample");
        assert_eq!(plugin.manifest().name, "Sample Plugin");
        assert_eq!(plugin.manifest().version, "1.2.3");
        assert_eq!(
            plugin.manifest().capabilities,
            vec![Capability::PlaybackControl, Capability::UiOverlay]
        );
        assert!(plugin.on_load().is_ok());
    }

    #[test]
    fn test_lua_plugin_on_event_and_actions() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "com.test.actions",
                capabilities = { "PlaybackControl" }
            }
            function p.on_event(event)
                return {
                    "TogglePause",
                    { action = "Seek", seconds = 15 }
                }
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "actions.lua").unwrap();
        let ev = PluginEvent::TrackChanged {
            track_id: TrackId(42),
            title: "Test Song".into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_sec: 180.0,
            path: PathBuf::from("/music/song.mp3"),
        };

        let actions = plugin.on_event(&ev);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], Action::TogglePause);
        assert_eq!(actions[1], Action::Seek(15));
    }

    #[test]
    fn test_lua_plugin_sandbox_restricts_os_exit() {
        let script = r#"
            local p = {}
            p.manifest = { id = "com.test.jailbreak" }
            function p.on_load()
                if os.exit then
                    os.exit(1)
                end
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "jailbreak.lua").unwrap();
        // os.exit does not exist in sandbox, so on_load completes without crashing the host process!
        assert!(plugin.on_load().is_ok());
    }

    #[test]
    fn test_lua_plugin_custom_action_dispatch() {
        let script = r#"
            local p = {}
            p.manifest = { id = "com.test.custom", capabilities = { "UiOverlay" } }
            function p.on_action(name, payload)
                if name == "ping" then
                    return "ToggleHelp"
                end
                return nil
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "custom.lua").unwrap();
        let actions = plugin.on_action("ping", &serde_json::json!({}));
        assert_eq!(actions, vec![Action::ToggleHelp]);
    }

    #[test]
    fn test_lua_plugin_memory_limit() {
        let script = r#"
            local p = {}
            p.manifest = { id = "com.test.oom" }
            function p.on_load()
                local big = {}
                for i = 1, 1000000 do
                    big[i] = string.rep("A", 1024)
                end
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "oom.lua").unwrap();
        // Memory limit should prevent allocating ~1GB and safely fail without crashing host
        assert!(plugin.on_load().is_err());
    }

    #[test]
    fn test_lua_plugin_keybinds_declaration() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "com.test.keybinds",
                capabilities = { "KeyBind" },
                keybinds = {
                    ["y"] = "toggle_lyrics",
                    ["ctrl+l"] = "show_log"
                }
            }
            return p
        "#;

        let plugin = LuaPlugin::from_script(script, "keybinds.lua").unwrap();
        assert_eq!(plugin.manifest().keybinds.get("y"), Some(&"toggle_lyrics".to_string()));
        assert_eq!(plugin.manifest().keybinds.get("ctrl+l"), Some(&"show_log".to_string()));
    }

    #[test]
    fn test_lua_plugin_emits_show_modal() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "com.test.modal",
                capabilities = { "UiOverlay" }
            }
            function p.on_action(name, payload)
                if name == "show_lyrics" then
                    return {
                        action = "ShowModal",
                        title = "Song Lyrics",
                        content = "Line 1\nLine 2"
                    }
                end
                return nil
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "modal.lua").unwrap();
        let actions = plugin.on_action("show_lyrics", &serde_json::json!({}));
        assert_eq!(
            actions,
            vec![Action::ShowModal {
                title: "Song Lyrics".to_string(),
                content: "Line 1\nLine 2".to_string(),
            }]
        );
    }

    #[test]
    fn test_lua_plugin_jailed_file_read_success() {
        let temp_dir = std::env::temp_dir().join(format!("tunotron_test_jail_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("song.lrc");
        let _ = std::fs::write(&test_file, "[00:15.00]Jailed lyrics content");

        let script = r#"
            local p = {}
            p.manifest = {
                id = "com.test.fs",
                capabilities = { "FsJailRead" }
            }
            function p.on_action(name, payload)
                local exists = tunotron.file_exists("song.lrc")
                local content, err = tunotron.read_file("song.lrc")
                if exists and content then
                    return {
                        action = "ShowModal",
                        title = "Lyrics",
                        content = content
                    }
                end
                return nil
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script_with_root(script, "fs.lua", Some(&temp_dir)).unwrap();
        let actions = plugin.on_action("read", &serde_json::json!({}));
        assert_eq!(
            actions,
            vec![Action::ShowModal {
                title: "Lyrics".to_string(),
                content: "[00:15.00]Jailed lyrics content".to_string(),
            }]
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lua_plugin_jailed_file_read_blocks_path_escape() {
        let temp_dir = std::env::temp_dir().join(format!("tunotron_test_jail_escape_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let script = r#"
            local p = {}
            p.manifest = {
                id = "com.test.escape",
                capabilities = { "FsJailRead" }
            }
            function p.on_action(name, payload)
                local exists = tunotron.file_exists("../../etc/passwd")
                local content, err = tunotron.read_file("../../etc/passwd")
                if not exists and not content and string.find(err, "escapes music jail") then
                    return "ToggleHelp"
                end
                return nil
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script_with_root(script, "escape.lua", Some(&temp_dir)).unwrap();
        let actions = plugin.on_action("read", &serde_json::json!({}));
        assert_eq!(actions, vec![Action::ToggleHelp]);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lua_plugin_jailed_file_read_missing_capability() {
        let temp_dir = std::env::temp_dir().join(format!("tunotron_test_jail_nocap_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("song.lrc");
        let _ = std::fs::write(&test_file, "secret");

        let script = r#"
            local p = {}
            p.manifest = {
                id = "com.test.nocap",
                capabilities = {} -- Missing FsJailRead!
            }
            function p.on_action(name, payload)
                local exists = tunotron.file_exists("song.lrc")
                local content, err = tunotron.read_file("song.lrc")
                if not exists and not content and string.find(err, "missing Capability::FsJailRead") then
                    return "ToggleHelp"
                end
                return nil
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script_with_root(script, "nocap.lua", Some(&temp_dir)).unwrap();
        let actions = plugin.on_action("read", &serde_json::json!({}));
        assert_eq!(actions, vec![Action::ToggleHelp]);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lyrics_viewer_example_plugin() {
        let temp_dir = std::env::temp_dir().join(format!("tunotron_test_jail_lyrics_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let audio_path = temp_dir.join("track.flac");
        let lrc_path = temp_dir.join("track.lrc");
        let _ = std::fs::write(&lrc_path, "[00:10.00]First line of lyrics\n[00:20.00]Second line");

        let plugin_path = Path::new("examples/plugins/lyrics_viewer.lua");
        let mut plugin = LuaPlugin::from_file(plugin_path, Some(&temp_dir)).unwrap();

        // 1. Simulate track changed event
        let ev = PluginEvent::TrackChanged {
            track_id: crate::library::track::TrackId(1),
            title: "Echoes".into(),
            artist: "Pink Floyd".into(),
            album: "Meddle".into(),
            duration_sec: 1400.0,
            path: audio_path,
        };
        let _ = plugin.on_event(&ev);

        // 2. Trigger 'y' keybind action
        let actions = plugin.on_action("toggle_lyrics", &serde_json::json!({}));
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ShowModal { title, content } => {
                assert_eq!(title, "Lyrics: Pink Floyd - Echoes");
                assert!(content.contains("First line of lyrics"));
            }
            other => panic!("Expected Action::ShowModal, got {:?}", other),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lua_plugin_on_tick_hook() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "org.tunotron.ticktest",
                name = "Tick Test",
                version = "0.1.0",
                description = "Tests on_tick hook",
                capabilities = {}
            }
            function p.on_tick(pos, dur)
                if pos >= 10.0 and dur >= 100.0 then
                    return { action = "Seek", seconds = 5 }
                end
                return {}
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "tick_test.lua").unwrap();
        let tick_ev = PluginEvent::Tick {
            position: 12.5,
            duration: 180.0,
        };
        let actions = plugin.on_event(&tick_ev);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], Action::Seek(5));
    }

    #[test]
    fn test_lua_plugin_on_event_tick_fallback() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "org.tunotron.tickfallback",
                name = "Tick Fallback",
                version = "0.1.0",
                description = "Tests on_event handling of Tick",
                capabilities = {}
            }
            function p.on_event(event)
                if event.type == "Tick" and event.position > 50.0 then
                    return { action = "TogglePause" }
                end
                return {}
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "tick_fallback.lua").unwrap();
        let tick_ev = PluginEvent::Tick {
            position: 55.0,
            duration: 200.0,
        };
        let actions = plugin.on_event(&tick_ev);
        assert_eq!(actions, vec![Action::TogglePause]);
    }

    #[test]
    fn test_sleep_timer_example_plugin() {
        let plugin_path = Path::new("examples/plugins/sleep_timer.lua");
        let mut plugin = LuaPlugin::from_file(plugin_path, None).unwrap();

        // 1. Toggle timer on
        let actions = plugin.on_action("toggle_timer", &serde_json::json!({}));
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ShowModal { title, .. } => assert_eq!(title, "Sleep Timer Activated"),
            other => panic!("Expected Action::ShowModal, got {:?}", other),
        }

        // Set remaining_sec to 1 for quick expiration testing
        plugin.plugin_table().unwrap().set("remaining_sec", 1).unwrap();

        // 2. Tick fires and timer expires
        let tick_ev = PluginEvent::Tick {
            position: 100.0,
            duration: 300.0,
        };
        let actions = plugin.on_event(&tick_ev);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], Action::TogglePause);
        match &actions[1] {
            Action::ShowModal { title, content } => {
                assert_eq!(title, "Sleep Timer Expired");
                assert!(content.contains("Playback has been paused"));
            }
            other => panic!("Expected Action::ShowModal, got {:?}", other),
        }
    }

    #[test]
    fn test_lua_plugin_notify_permission_denied() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "org.tunotron.nonotify",
                name = "No Notify",
                version = "0.1.0",
                description = "Missing notify capability",
                capabilities = {}
            }
            function p.on_action(name, payload)
                local ok, err = tunotron.notify("Test", "Should fail")
                if not ok and string.find(err, "missing Capability::Notify") then
                    return { action = "ToggleHelp" }
                end
                return {}
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "nonotify.lua").unwrap();
        let actions = plugin.on_action("test", &serde_json::json!({}));
        assert_eq!(actions, vec![Action::ToggleHelp]);
    }

    #[test]
    fn test_lua_plugin_notify_action_emission() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "org.tunotron.notifytest",
                name = "Notify Test",
                version = "0.1.0",
                description = "Tests notify action emission",
                capabilities = { "Notify" }
            }
            function p.on_action(name, payload)
                return {
                    action = "Notify",
                    summary = "Now Playing",
                    body = "Pink Floyd - Echoes"
                }
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "notify_test.lua").unwrap();
        let actions = plugin.on_action("alert", &serde_json::json!({}));
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0],
            Action::Notify {
                summary: "Now Playing".to_string(),
                body: "Pink Floyd - Echoes".to_string(),
            }
        );
    }

    #[test]
    fn test_now_playing_notify_example_plugin() {
        let plugin_path = Path::new("examples/plugins/now_playing_notify.lua");
        let mut plugin = LuaPlugin::from_file(plugin_path, None).unwrap();

        // 1. Simulate track changed event
        let ev = PluginEvent::TrackChanged {
            track_id: crate::library::track::TrackId(1),
            title: "Time".into(),
            artist: "Pink Floyd".into(),
            album: "The Dark Side of the Moon".into(),
            duration_sec: 425.0,
            path: PathBuf::from("/music/time.flac"),
        };
        let actions = plugin.on_event(&ev);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Notify { summary, body } => {
                assert_eq!(summary, "Time");
                assert!(body.contains("Pink Floyd"));
                assert!(body.contains("The Dark Side of the Moon"));
                assert!(body.contains("[07:05]"));
            }
            other => panic!("Expected Action::Notify, got {:?}", other),
        }

        // 2. Trigger 'N' keybind action
        let actions = plugin.on_action("show_now_playing", &serde_json::json!({}));
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Notify { summary, body } => {
                assert_eq!(summary, "Time");
                assert!(body.contains("Pink Floyd"));
            }
            other => panic!("Expected Action::Notify, got {:?}", other),
        }
    }

    #[test]
    fn test_lua_plugin_instruction_budget_terminates_infinite_loop() {
        let script = r#"
            local p = {}
            p.manifest = {
                id = "org.tunotron.rogueloop",
                name = "Rogue Loop",
                version = "0.1.0",
                description = "Attempts infinite loop",
                capabilities = {}
            }
            function p.on_action(name, payload)
                -- Runaway loop: allocates 0 memory, but spins instructions endlessly
                while true do end
                return { action = "ToggleHelp" }
            end
            return p
        "#;

        let mut plugin = LuaPlugin::from_script(script, "rogue_loop.lua").unwrap();

        let start = std::time::Instant::now();
        let actions = plugin.on_action("hang", &serde_json::json!({}));
        let elapsed = start.elapsed();

        // Must abort without hanging (under 50ms) and return empty actions safely
        assert!(elapsed < std::time::Duration::from_millis(50));
        assert!(actions.is_empty(), "Aborted script must return no actions");
    }
}
