use std::path::Path;
use mlua::{Lua, LuaSerdeExt, Table, Value};
use crate::action::{Action, Capability};
use super::manifest::PluginManifest;
use super::traits::{Plugin, PluginEvent};

pub struct LuaPlugin {
    manifest: PluginManifest,
    lua: Lua,
}

impl LuaPlugin {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read Lua plugin at {}: {}", path.display(), e))?;
        Self::from_script(&content, path.to_string_lossy().as_ref())
    }

    pub fn from_script(script: &str, chunk_name: &str) -> Result<Self, String> {
        let lua = Lua::new();

        // 1. Sandbox setup: strip dangerous host process controls and cap memory (16MB)
        let _ = lua.set_memory_limit(16 * 1024 * 1024);
        setup_sandbox(&lua)?;

        // 2. Inject `tunotron` global host module
        setup_host_globals(&lua)?;

        // 3. Load script chunk
        let plugin_tbl: Table = lua
            .load(script)
            .set_name(chunk_name)
            .eval()
            .map_err(|e| format!("Error executing Lua script '{}': {}", chunk_name, e))?;

        // 4. Extract manifest table
        let manifest_tbl: Table = plugin_tbl
            .get("manifest")
            .map_err(|e| format!("Plugin '{}' must export a 'manifest' table: {}", chunk_name, e))?;

        let id: String = manifest_tbl
            .get("id")
            .map_err(|_| format!("Plugin '{}' manifest missing required 'id' field", chunk_name))?;

        let name: String = manifest_tbl.get("name").unwrap_or_else(|_| id.clone());
        let version: String = manifest_tbl.get("version").unwrap_or_else(|_| "0.1.0".to_string());
        let description: String = manifest_tbl.get("description").unwrap_or_default();

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

        let manifest = PluginManifest::new(id, name, version, description, capabilities);

        // Store plugin table in Lua registry so we can retrieve its callbacks
        let _ = lua.set_named_registry_value("__tunotron_plugin_table", plugin_tbl);

        Ok(Self { manifest, lua })
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
        let plugin_tbl = self.plugin_table()?;
        if let Ok(on_load_fn) = plugin_tbl.get::<mlua::Function>("on_load") {
            on_load_fn
                .call::<()>(())
                .map_err(|e| format!("Error in on_load callback for plugin '{}': {}", self.manifest.id, e))?;
        }
        Ok(())
    }

    fn on_event(&mut self, event: &PluginEvent) -> Vec<Action> {
        let Ok(plugin_tbl) = self.plugin_table() else { return Vec::new(); };
        let Ok(on_event_fn) = plugin_tbl.get::<mlua::Function>("on_event") else { return Vec::new(); };

        let lua_event: Value = match self.lua.to_value(event) {
            Ok(val) => val,
            Err(e) => {
                tracing::error!("Failed to serialize event for plugin '{}': {}", self.manifest.id, e);
                return Vec::new();
            }
        };

        match on_event_fn.call::<Value>(lua_event) {
            Ok(ret_val) => parse_actions_from_value(ret_val),
            Err(e) => {
                tracing::error!("Error in on_event for plugin '{}': {}", self.manifest.id, e);
                Vec::new()
            }
        }
    }

    fn on_action(&mut self, name: &str, payload: &serde_json::Value) -> Vec<Action> {
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
}
