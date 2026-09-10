use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::library::track::Track;

#[derive(Debug, Clone)]
pub enum BrowserEntry {
    ParentDir(PathBuf),
    Directory {
        name: String,
        path: PathBuf,
    },
    AudioTrack(Arc<Track>),
}

#[allow(dead_code)]
impl BrowserEntry {
    pub fn is_dir(&self) -> bool {
        matches!(self, BrowserEntry::ParentDir(_) | BrowserEntry::Directory { .. })
    }

    pub fn path(&self) -> &Path {
        match self {
            BrowserEntry::ParentDir(p) => p,
            BrowserEntry::Directory { path, .. } => path,
            BrowserEntry::AudioTrack(t) => &t.path,
        }
    }
}

/// Canonical jail resolver: returns Some(canonical_path) if and only if
/// the candidate path resides strictly within the canonical root directory.
/// Note: `canonical_root` is expected to be already canonicalized.
pub fn resolve_in_jail(canonical_root: &Path, candidate: &Path) -> Option<PathBuf> {
    let canon_cand = candidate.canonicalize().ok()?;
    if canon_cand == canonical_root || canon_cand.starts_with(canonical_root) {
        Some(canon_cand)
    } else {
        None
    }
}

pub fn read_directory(dir: &Path, root_boundary: &Path) -> Vec<BrowserEntry> {
    let mut items = Vec::new();

    // Only allow navigating up if we are strictly inside a subdirectory of the music root
    if dir != root_boundary && dir.starts_with(root_boundary) {
        if let Some(parent) = dir.parent() {
            if let Some(jailed_parent) = resolve_in_jail(root_boundary, parent) {
                items.push(BrowserEntry::ParentDir(jailed_parent));
            }
        }
    }

    if let Ok(entries) = fs::read_dir(dir) {
        let mut dirs = Vec::new();
        let mut tracks = Vec::new();

        for entry in entries.filter_map(|e| e.ok()) {
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };

            // Hardened jail: do not traverse symlinks pointing outside the jail
            if ft.is_symlink() {
                continue;
            }

            let path = entry.path();
            if ft.is_dir() {
                if let Some(jailed_dir) = resolve_in_jail(root_boundary, &path) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    dirs.push(BrowserEntry::Directory { name, path: jailed_dir });
                }
            } else if ft.is_file() && Track::is_audio_file(&path) {
                let track = Track::new(path);
                tracks.push(track);
            }
        }

        // Sort directories alphabetically (ASCII fast comparison without allocations)
        dirs.sort_by(|a, b| match (a, b) {
            (BrowserEntry::Directory { name: a_name, .. }, BrowserEntry::Directory { name: b_name, .. }) => {
                ascii_case_cmp(a_name, b_name)
            }
            _ => std::cmp::Ordering::Equal,
        });

        // Sort tracks by track number if available, then by title / filename
        tracks.sort_by(|a, b| {
            match (a.track_number, b.track_number) {
                (Some(an), Some(bn)) if an != bn => an.cmp(&bn),
                _ => ascii_case_cmp(&a.filename, &b.filename),
            }
        });

        items.extend(dirs);
        for track in tracks {
            items.push(BrowserEntry::AudioTrack(Arc::new(track)));
        }
    }

    items
}

#[inline]
fn ascii_case_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    a.bytes()
        .map(|c| c.to_ascii_lowercase())
        .cmp(b.bytes().map(|c| c.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii_case_cmp() {
        assert_eq!(ascii_case_cmp("abc", "ABC"), std::cmp::Ordering::Equal);
        assert_eq!(ascii_case_cmp("abc", "abd"), std::cmp::Ordering::Less);
        assert_eq!(ascii_case_cmp("02 track", "01 track"), std::cmp::Ordering::Greater);
    }

    #[test]
    fn test_resolve_in_jail() {
        let tmp = std::env::temp_dir();
        let jail_root = tmp.join("tunotron_test_jail");
        let sub = jail_root.join("sub");
        let _ = std::fs::create_dir_all(&sub);

        let canonical_root = jail_root.canonicalize().unwrap();

        // Child is allowed
        let resolved_sub = resolve_in_jail(&canonical_root, &sub);
        assert!(resolved_sub.is_some());

        // Jail root itself is allowed
        let resolved_root = resolve_in_jail(&canonical_root, &jail_root);
        assert!(resolved_root.is_some());

        // Path traversal escaping root is blocked
        let escape_path = jail_root.join("../");
        let resolved_escape = resolve_in_jail(&canonical_root, &escape_path);
        assert!(resolved_escape.is_none());

        // Cleanup
        let _ = std::fs::remove_dir_all(&jail_root);
    }
}

