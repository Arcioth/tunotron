use crate::action::ActionEnvelope;
use super::manifest::PluginManifest;
use super::traits::{Plugin, PluginEvent};

const MAX_WALL_CLOCK_TIME: std::time::Duration = std::time::Duration::from_millis(50);
const MAX_VIOLATION_STRIKES: u32 = 3;

struct ManagedPlugin {
    plugin: Box<dyn Plugin>,
    strikes: u32,
    disabled: bool,
}

pub struct PluginManager {
    plugins: Vec<ManagedPlugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<(), String> {
        let id = plugin.manifest().id.clone();
        if self.plugins.iter().any(|p| p.plugin.manifest().id == id) {
            return Err(format!("Plugin with id '{}' is already registered", id));
        }

        // Validate API version compatibility if specified
        if let Some(ref api_ver) = plugin.manifest().api_version {
            let is_compatible = api_ver.starts_with("0.")
                || api_ver.starts_with("1.")
                || api_ver == "0"
                || api_ver == "1";
            if !is_compatible {
                return Err(format!(
                    "Plugin '{}' requires incompatible API version '{}'; host supports '^0.3' or '^1.0'",
                    id, api_ver
                ));
            }
        }

        plugin.on_load()?;
        tracing::info!(
            "Registered plugin '{}' [{}] v{} (granted capabilities: {:?})",
            plugin.manifest().name,
            id,
            plugin.manifest().version,
            plugin.manifest().capabilities
        );
        self.plugins.push(ManagedPlugin {
            plugin,
            strikes: 0,
            disabled: false,
        });
        Ok(())
    }

    pub fn unregister(&mut self, id: &str) -> bool {
        if let Some(pos) = self.plugins.iter().position(|p| p.plugin.manifest().id == id) {
            let mut managed = self.plugins.remove(pos);
            managed.plugin.on_unload();
            tracing::info!("Unregistered plugin: [{}]", id);
            true
        } else {
            false
        }
    }

    pub fn manifests(&self) -> Vec<PluginManifest> {
        self.plugins
            .iter()
            .map(|p| p.plugin.manifest().clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.plugins.iter().filter(|p| !p.disabled).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn is_disabled(&self, id: &str) -> bool {
        self.plugins
            .iter()
            .find(|p| p.plugin.manifest().id == id)
            .map(|p| p.disabled)
            .unwrap_or(false)
    }

    /// Broadcasts a domain event to all plugins and enforces capability permissions
    /// on any returned actions via the strict default-deny matrix.
    pub fn dispatch_event(&mut self, event: &PluginEvent) -> Vec<ActionEnvelope> {
        let mut allowed_envelopes = Vec::new();

        for entry in &mut self.plugins {
            if entry.disabled {
                continue;
            }

            let manifest = entry.plugin.manifest().clone();
            let start = std::time::Instant::now();
            let raw_actions = entry.plugin.on_event(event);
            let elapsed = start.elapsed();

            if elapsed > MAX_WALL_CLOCK_TIME {
                entry.strikes += 1;
                tracing::warn!(
                    "Plugin '{}' exceeded wall-clock budget ({:?} > {:?}) [strike {}/{}]",
                    manifest.id,
                    elapsed,
                    MAX_WALL_CLOCK_TIME,
                    entry.strikes,
                    MAX_VIOLATION_STRIKES
                );
                if entry.strikes >= MAX_VIOLATION_STRIKES {
                    entry.disabled = true;
                    tracing::error!(
                        "Plugin '{}' auto-disabled after {} consecutive safety violations",
                        manifest.id,
                        MAX_VIOLATION_STRIKES
                    );
                    continue;
                }
            }

            for action in raw_actions {
                let envelope = ActionEnvelope::plugin(&manifest.id, action);
                if envelope.source.is_permitted(&envelope.action, &manifest.capabilities) {
                    allowed_envelopes.push(envelope);
                } else {
                    tracing::warn!(
                        "Blocked unauthorized action {:?} from plugin '{}' (missing required capability: {:?})",
                        envelope.action,
                        manifest.id,
                        envelope.action.required_capability()
                    );
                }
            }
        }

        allowed_envelopes
    }

    /// Dispatches an action targeted at a specific plugin by id.
    pub fn dispatch_action(
        &mut self,
        target_plugin_id: &str,
        name: &str,
        payload: &serde_json::Value,
    ) -> Vec<ActionEnvelope> {
        let mut allowed_envelopes = Vec::new();

        for entry in &mut self.plugins {
            if entry.plugin.manifest().id == target_plugin_id {
                if entry.disabled {
                    tracing::warn!("Ignoring action for disabled plugin '{}'", target_plugin_id);
                    return Vec::new();
                }

                let manifest = entry.plugin.manifest().clone();
                let start = std::time::Instant::now();
                let raw_actions = entry.plugin.on_action(name, payload);
                let elapsed = start.elapsed();

                if elapsed > MAX_WALL_CLOCK_TIME {
                    entry.strikes += 1;
                    tracing::warn!(
                        "Plugin '{}' exceeded wall-clock budget ({:?} > {:?}) [strike {}/{}]",
                        manifest.id,
                        elapsed,
                        MAX_WALL_CLOCK_TIME,
                        entry.strikes,
                        MAX_VIOLATION_STRIKES
                    );
                    if entry.strikes >= MAX_VIOLATION_STRIKES {
                        entry.disabled = true;
                        tracing::error!(
                            "Plugin '{}' auto-disabled after {} consecutive safety violations",
                            manifest.id,
                            MAX_VIOLATION_STRIKES
                        );
                        return Vec::new();
                    }
                }

                for action in raw_actions {
                    let envelope = ActionEnvelope::plugin(&manifest.id, action);
                    if envelope.source.is_permitted(&envelope.action, &manifest.capabilities) {
                        allowed_envelopes.push(envelope);
                    } else {
                        tracing::warn!(
                            "Blocked unauthorized action {:?} from plugin '{}' (missing required capability: {:?})",
                            envelope.action,
                            manifest.id,
                            envelope.action.required_capability()
                        );
                    }
                }
            }
        }

        allowed_envelopes
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Action, Capability};

    struct TestPlugin {
        manifest: PluginManifest,
        loaded: bool,
        unloaded: bool,
        actions_to_emit: Vec<Action>,
    }

    impl TestPlugin {
        fn new(id: &str, capabilities: Vec<Capability>, actions_to_emit: Vec<Action>) -> Self {
            Self {
                manifest: PluginManifest::new(
                    id,
                    "Test Plugin",
                    "1.0.0",
                    "A plugin for testing",
                    capabilities,
                ),
                loaded: false,
                unloaded: false,
                actions_to_emit,
            }
        }

        fn with_api_version(mut self, api_version: impl Into<String>) -> Self {
            self.manifest = self.manifest.with_api_version(api_version);
            self
        }
    }

    impl Plugin for TestPlugin {
        fn manifest(&self) -> &PluginManifest {
            &self.manifest
        }

        fn on_load(&mut self) -> Result<(), String> {
            self.loaded = true;
            Ok(())
        }

        fn on_event(&mut self, _event: &PluginEvent) -> Vec<Action> {
            self.actions_to_emit.clone()
        }

        fn on_action(&mut self, name: &str, _payload: &serde_json::Value) -> Vec<Action> {
            if name == "ping" {
                vec![Action::ToggleHelp]
            } else {
                Vec::new()
            }
        }

        fn on_unload(&mut self) {
            self.unloaded = true;
        }
    }

    #[test]
    fn test_plugin_registration_and_lifecycle() {
        let mut mgr = PluginManager::new();
        assert_eq!(mgr.len(), 0);

        let plugin = Box::new(TestPlugin::new("test.lifecycle", vec![], vec![]));
        assert!(mgr.register(plugin).is_ok());
        assert_eq!(mgr.len(), 1);

        // Duplicate id fails
        let duplicate = Box::new(TestPlugin::new("test.lifecycle", vec![], vec![]));
        assert!(mgr.register(duplicate).is_err());

        // Unregister succeeds
        assert!(mgr.unregister("test.lifecycle"));
        assert_eq!(mgr.len(), 0);
        assert!(!mgr.unregister("test.lifecycle"));
    }

    #[test]
    fn test_plugin_capability_enforcement() {
        let mut mgr = PluginManager::new();

        // 1. Plugin with PlaybackControl capability emitting TogglePause (permitted)
        let permitted_plugin = Box::new(TestPlugin::new(
            "test.playback",
            vec![Capability::PlaybackControl],
            vec![Action::TogglePause],
        ));
        mgr.register(permitted_plugin).unwrap();

        let event = PluginEvent::TimePos { seconds: 10.0 };
        let envelopes = mgr.dispatch_event(&event);
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].action, Action::TogglePause);

        // 2. Plugin without capability attempting TogglePause (denied)
        let mut mgr2 = PluginManager::new();
        let unprivileged_plugin = Box::new(TestPlugin::new(
            "test.unprivileged",
            vec![],
            vec![Action::TogglePause],
        ));
        mgr2.register(unprivileged_plugin).unwrap();

        let envelopes2 = mgr2.dispatch_event(&event);
        assert!(envelopes2.is_empty(), "Action without capability must be denied");

        // 3. Plugin attempting HostOnly action (Quit) is always denied even if it claims capabilities
        let mut mgr3 = PluginManager::new();
        let rogue_plugin = Box::new(TestPlugin::new(
            "test.rogue",
            vec![Capability::PlaybackControl, Capability::PlaybackQueue, Capability::UiOverlay],
            vec![Action::Quit],
        ));
        mgr3.register(rogue_plugin).unwrap();

        let envelopes3 = mgr3.dispatch_event(&event);
        assert!(envelopes3.is_empty(), "HostOnly Action::Quit must always be blocked");
    }

    #[test]
    fn test_plugin_dispatch_action() {
        let mut mgr = PluginManager::new();
        let plugin = Box::new(TestPlugin::new(
            "test.custom",
            vec![Capability::UiOverlay],
            vec![],
        ));
        mgr.register(plugin).unwrap();

        let envelopes = mgr.dispatch_action("test.custom", "ping", &serde_json::Value::Null);
        assert_eq!(envelopes.len(), 1);
        assert_eq!(envelopes[0].action, Action::ToggleHelp);

        let empty = mgr.dispatch_action("test.custom", "unknown", &serde_json::Value::Null);
        assert!(empty.is_empty());
    }

    #[test]
    fn test_plugin_api_version_compatibility_check() {
        let mut mgr = PluginManager::new();

        // Compatible 0.x plugin succeeds
        let p_0 = Box::new(TestPlugin::new("test.v0", vec![], vec![]).with_api_version("0.3.7"));
        assert!(mgr.register(p_0).is_ok());

        // Compatible 1.x plugin succeeds
        let p_1 = Box::new(TestPlugin::new("test.v1", vec![], vec![]).with_api_version("1.0.0"));
        assert!(mgr.register(p_1).is_ok());

        // Incompatible 2.x plugin is strictly rejected
        let p_2 = Box::new(TestPlugin::new("test.v2", vec![], vec![]).with_api_version("2.0.0"));
        let res = mgr.register(p_2);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("incompatible API version"));
    }

    #[test]
    fn test_plugin_wall_clock_timeout_auto_disable() {
        struct SlowPlugin {
            manifest: PluginManifest,
        }

        impl Plugin for SlowPlugin {
            fn manifest(&self) -> &PluginManifest {
                &self.manifest
            }
            fn on_load(&mut self) -> Result<(), String> {
                Ok(())
            }
            fn on_event(&mut self, _event: &PluginEvent) -> Vec<Action> {
                std::thread::sleep(std::time::Duration::from_millis(55));
                vec![Action::ToggleHelp]
            }
            fn on_action(&mut self, _name: &str, _payload: &serde_json::Value) -> Vec<Action> {
                std::thread::sleep(std::time::Duration::from_millis(55));
                vec![Action::ToggleHelp]
            }
            fn on_unload(&mut self) {}
        }

        let mut mgr = PluginManager::new();
        let slow = Box::new(SlowPlugin {
            manifest: PluginManifest::new("test.slow", "Slow", "0.1", "", vec![Capability::UiOverlay]),
        });
        mgr.register(slow).unwrap();

        let event = PluginEvent::TimePos { seconds: 1.0 };

        // Strike 1
        let a1 = mgr.dispatch_event(&event);
        assert_eq!(a1.len(), 1);
        assert!(!mgr.is_disabled("test.slow"));

        // Strike 2
        let a2 = mgr.dispatch_event(&event);
        assert_eq!(a2.len(), 1);
        assert!(!mgr.is_disabled("test.slow"));

        // Strike 3: triggers auto-disable
        let a3 = mgr.dispatch_event(&event);
        assert!(a3.is_empty(), "On strike 3 plugin is disabled immediately");
        assert!(mgr.is_disabled("test.slow"));
        assert_eq!(mgr.len(), 0);

        // Subsequent dispatches are completely ignored
        let a4 = mgr.dispatch_event(&event);
        assert!(a4.is_empty());
        let a5 = mgr.dispatch_action("test.slow", "ping", &serde_json::Value::Null);
        assert!(a5.is_empty());
    }
}
