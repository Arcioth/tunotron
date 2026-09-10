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
    pub api_version: Option<String>,
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
            api_version: None,
            keybinds: HashMap::new(),
        }
    }

    pub fn with_api_version(mut self, api_version: impl Into<String>) -> Self {
        self.api_version = Some(api_version.into());
        self
    }

    pub fn with_keybinds(mut self, keybinds: HashMap<String, String>) -> Self {
        self.keybinds = keybinds;
        self
    }
}
