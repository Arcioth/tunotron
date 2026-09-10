#![allow(dead_code)]

pub mod builtin;
pub mod loader;
pub mod lua;
pub mod manifest;
pub mod manager;
pub mod traits;

pub use builtin::TrackLoggerPlugin;
#[allow(unused_imports)]
pub use loader::{default_plugin_dir, load_plugins_from_dir};
#[allow(unused_imports)]
pub use lua::LuaPlugin;
#[allow(unused_imports)]
pub use manifest::PluginManifest;
pub use manager::PluginManager;
#[allow(unused_imports)]
pub use traits::{Plugin, PluginEvent};
