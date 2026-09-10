use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::action::Capability;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub keybinds: HashMap<String, String>,
}

impl PluginManifest {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        description: impl Into<String>,
        capabilities: Vec<Capability>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            version: version.into(),
            description: description.into(),
            capabilities,
            keybinds: HashMap::new(),
        }
    }

    pub fn with_keybinds(mut self, keybinds: HashMap<String, String>) -> Self {
        self.keybinds = keybinds;
        self
    }
}
