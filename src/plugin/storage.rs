use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Maximum serialized persistent state size per plugin (256 KB).
pub const MAX_STATE_BYTES: usize = 256 * 1024;

#[derive(Debug)]
pub struct PluginStorage {
    plugin_id: String,
    file_path: PathBuf,
    data: HashMap<String, serde_json::Value>,
    dirty: bool,
}

impl PluginStorage {
    pub fn new(plugin_id: impl Into<String>, custom_root: Option<&Path>) -> Self {
        let plugin_id = plugin_id.into();
        let base_dir = custom_root
            .map(|p| p.to_path_buf())
            .unwrap_or_else(default_data_dir);
        let plugin_dir = base_dir.join(&plugin_id);
        let file_path = plugin_dir.join("state.json");

        let mut data = HashMap::new();
        if file_path.exists() {
            if let Ok(content) = fs::read_to_string(&file_path) {
                if let Ok(parsed) = serde_json::from_str::<HashMap<String, serde_json::Value>>(&content) {
                    data = parsed;
                } else {
                    tracing::warn!("Failed to parse state file for plugin '{}'", plugin_id);
                }
            }
        }

        Self {
            plugin_id,
            file_path,
            data,
            dirty: false,
        }
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: String, val: serde_json::Value) -> Result<(), String> {
        let mut tentative = self.data.clone();
        tentative.insert(key.clone(), val.clone());
        let serialized = serde_json::to_string(&tentative)
            .map_err(|e| format!("Failed to serialize state: {}", e))?;
        if serialized.len() > MAX_STATE_BYTES {
            return Err(format!(
                "Persistent state quota exceeded ({} bytes > {} bytes limit)",
                serialized.len(),
                MAX_STATE_BYTES
            ));
        }

        self.data.insert(key, val);
        self.dirty = true;
        Ok(())
    }

    pub fn del(&mut self, key: &str) -> bool {
        if self.data.remove(key).is_some() {
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn all(&self) -> &HashMap<String, serde_json::Value> {
        &self.data
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn flush(&mut self) -> Result<(), String> {
        if !self.dirty {
            return Ok(());
        }

        if let Some(parent) = self.file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create storage dir '{}': {}", parent.display(), e))?;
        }

        let serialized = serde_json::to_string_pretty(&self.data)
            .map_err(|e| format!("Failed to serialize state: {}", e))?;

        // Atomic write: write to tempfile first, then rename
        let tmp_path = self.file_path.with_extension("tmp");
        fs::write(&tmp_path, serialized)
            .map_err(|e| format!("Failed to write state tempfile '{}': {}", tmp_path.display(), e))?;

        fs::rename(&tmp_path, &self.file_path)
            .map_err(|e| format!("Failed to rename state file to '{}': {}", self.file_path.display(), e))?;

        self.dirty = false;
        tracing::debug!("Flushed persistent state for plugin '{}'", self.plugin_id);
        Ok(())
    }
}

pub fn default_data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| dirs_home().join(".local/share"))
        .join("tunotron/plugins")
}

fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_storage_crud_and_atomic_flush() {
        let temp_dir = std::env::temp_dir().join(format!("tunotron_storage_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let mut storage = PluginStorage::new("org.test.plugin", Some(&temp_dir));
        assert!(storage.get("counter").is_none());

        // Set values
        storage.set("counter".into(), serde_json::json!(42)).unwrap();
        storage.set("name".into(), serde_json::json!("Tunotron")).unwrap();
        assert_eq!(storage.get("counter"), Some(&serde_json::json!(42)));
        assert!(storage.is_dirty());

        // Flush to disk
        storage.flush().unwrap();
        assert!(!storage.is_dirty());
        assert!(storage.file_path().exists());

        // Reload from disk in a fresh storage instance
        let mut reloaded = PluginStorage::new("org.test.plugin", Some(&temp_dir));
        assert_eq!(reloaded.get("counter"), Some(&serde_json::json!(42)));
        assert_eq!(reloaded.get("name"), Some(&serde_json::json!("Tunotron")));

        // Delete key
        assert!(reloaded.del("counter"));
        assert_eq!(reloaded.get("counter"), None);
        reloaded.flush().unwrap();

        let third = PluginStorage::new("org.test.plugin", Some(&temp_dir));
        assert!(third.get("counter").is_none());
        assert_eq!(third.get("name"), Some(&serde_json::json!("Tunotron")));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_plugin_storage_quota_enforcement() {
        let temp_dir = std::env::temp_dir().join(format!("tunotron_storage_quota_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);

        let mut storage = PluginStorage::new("org.test.quota", Some(&temp_dir));

        // Create a payload exceeding MAX_STATE_BYTES (256 KB)
        let huge_string = "a".repeat(MAX_STATE_BYTES + 1024);
        let res = storage.set("bomb".into(), serde_json::json!(huge_string));
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("quota exceeded"));

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
