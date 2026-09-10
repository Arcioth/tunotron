use std::path::{Path, PathBuf};
use super::lua::LuaPlugin;
use super::manager::PluginManager;
use super::traits::Plugin;

pub fn default_plugin_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs_home().join(".config")
        })
        .join("tunotron/plugins")
}

pub fn load_plugins_from_dir(dir: &Path, music_root: Option<&Path>, mgr: &mut PluginManager) -> usize {
    if !dir.exists() || !dir.is_dir() {
        return 0;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };

    let mut loaded_count = 0;

    let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();

    for path in paths {

        let plugin_file = if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("lua") {
            Some(path)
        } else if path.is_dir() {
            let init_file = path.join("init.lua");
            if init_file.is_file() {
                Some(init_file)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(target) = plugin_file {
            match LuaPlugin::from_file(&target, music_root) {
                Ok(plugin) => {
                    let id = plugin.manifest().id.clone();
                    if let Err(e) = mgr.register(Box::new(plugin)) {
                        tracing::error!("Failed to register Lua plugin from '{}': {}", target.display(), e);
                    } else {
                        tracing::info!("Loaded Lua plugin '{}' from {}", id, target.display());
                        loaded_count += 1;
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to load Lua plugin from '{}': {}", target.display(), e);
                }
            }
        }
    }

    loaded_count
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}
