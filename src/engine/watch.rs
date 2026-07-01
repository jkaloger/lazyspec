//! Pure watch-set logic shared by the TUI file watcher and the web reload loop.
//!
//! [`watch_paths`] computes the filesystem paths a watcher should monitor for a
//! given project root and [`Config`]. It has no side effects and no dependency
//! on `notify` or any UI layer, so both `tui` and `web` can build a watcher over
//! it without either depending on the other (lift precedent: `flatten_forest`).

use crate::engine::config::Config;
use std::path::{Path, PathBuf};

/// The filesystem paths a watcher monitors: `.lazyspec.toml` plus each existing
/// type directory of the current config. Pure (no side effects) so the watch set
/// is unit-testable independent of `notify`.
pub fn watch_paths(root: &Path, config: &Config) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let config_path = root.join(".lazyspec.toml");
    if config_path.exists() {
        paths.push(config_path);
    }
    for t in &config.documents.types {
        let full = root.join(&t.dir);
        if full.exists() {
            paths.push(full);
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{StoreBackend, TypeDef};
    use tempfile::TempDir;

    fn config_with_dirs(dirs: &[&str]) -> Config {
        let mut config = Config::default();
        config.documents.types = dirs
            .iter()
            .map(|dir| {
                let mut t = TypeDef::test_fixture("doc", StoreBackend::Filesystem);
                t.dir = dir.to_string();
                t
            })
            .collect();
        config
    }

    // AC5: `.lazyspec.toml` is always in the watch set, regardless of which type
    // dirs exist.
    #[test]
    fn watch_paths_always_includes_lazyspec_toml() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), "").unwrap();
        let config = config_with_dirs(&[]);

        let paths = watch_paths(root, &config);

        assert!(
            paths.contains(&root.join(".lazyspec.toml")),
            "expected watch set to contain .lazyspec.toml, got {paths:?}"
        );
    }

    // AC4: the watch set contains existing type dirs and excludes missing ones.
    #[test]
    fn watch_paths_includes_existing_dirs_excludes_missing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(root.join(".lazyspec.toml"), "").unwrap();
        std::fs::create_dir_all(root.join("docs/present")).unwrap();
        let config = config_with_dirs(&["docs/present", "docs/missing"]);

        let paths = watch_paths(root, &config);

        assert!(
            paths.contains(&root.join("docs/present")),
            "expected the existing dir in the watch set, got {paths:?}"
        );
        assert!(
            !paths.contains(&root.join("docs/missing")),
            "expected the missing dir excluded from the watch set, got {paths:?}"
        );
    }
}
