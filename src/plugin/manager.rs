use crate::action::ActionEnvelope;
use super::manifest::PluginManifest;
use super::traits::{Plugin, PluginEvent};

pub struct PluginManager {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<(), String> {
        let id = plugin.manifest().id.clone();
        if self.plugins.iter().any(|p| p.manifest().id == id) {
            return Err(format!("Plugin with id '{}' is already registered", id));
        }

        plugin.on_load()?;
        tracing::info!("Registered plugin: [{}] {}", id, plugin.manifest().name);
        self.plugins.push(plugin);
        Ok(())
    }

    pub fn unregister(&mut self, id: &str) -> bool {
        if let Some(pos) = self.plugins.iter().position(|p| p.manifest().id == id) {
            let mut plugin = self.plugins.remove(pos);
            plugin.on_unload();
            tracing::info!("Unregistered plugin: [{}]", id);
            true
        } else {
            false
        }
    }

    pub fn manifests(&self) -> Vec<PluginManifest> {
        self.plugins.iter().map(|p| p.manifest().clone()).collect()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Broadcasts a domain event to all plugins and enforces capability permissions
    /// on any returned actions via the strict default-deny matrix.
    pub fn dispatch_event(&mut self, event: &PluginEvent) -> Vec<ActionEnvelope> {
        let mut allowed_envelopes = Vec::new();

        for plugin in &mut self.plugins {
            let manifest = plugin.manifest().clone();
            let raw_actions = plugin.on_event(event);

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

        for plugin in &mut self.plugins {
            if plugin.manifest().id == target_plugin_id {
                let manifest = plugin.manifest().clone();
                let raw_actions = plugin.on_action(name, payload);

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

        let event = PluginEvent::TimePos(10.0);
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
}
