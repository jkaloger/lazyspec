use crate::engine::config::Config;
use crate::engine::hashing::sha256_hex;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Which hooks are trusted, in user-local state outside the repo (RFC-075
/// Trust): a fingerprint of the `[[hooks]]` table and of every file a `run`
/// names under the docs root, filed under the project root.
pub struct TrustStore {
    file: PathBuf,
}

impl TrustStore {
    pub fn user_local() -> Self {
        let dir = match std::env::var_os("LAZYSPEC_STATE_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".lazyspec")
            }
        };
        Self::in_dir(&dir)
    }

    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join("hook-trust.json"),
        }
    }

    fn key(root: &Path) -> String {
        root.canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
            .display()
            .to_string()
    }

    fn load(&self) -> HashMap<String, String> {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn is_trusted(&self, root: &Path, config: &Config) -> bool {
        self.load().get(&Self::key(root)) == Some(&fingerprint(root, config))
    }

    pub fn trust(&self, root: &Path, config: &Config) -> Result<()> {
        let mut entries = self.load();
        entries.insert(Self::key(root), fingerprint(root, config));
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, serde_json::to_string_pretty(&entries)?)?;
        Ok(())
    }
}

/// A hash of the `[[hooks]]` table plus the bytes of each file under the docs
/// root that a `run` argv names. Editing a hook or the script it runs changes
/// it. Scripts resolve against `config.docs_root(root)`, where templates do, so
/// a pack adopted via `extends` is fingerprinted at the pack.
fn fingerprint(root: &Path, config: &Config) -> String {
    let scripts_root = config.docs_root(root);
    let scripts_root = scripts_root.canonicalize().unwrap_or(scripts_root);
    let mut material = serde_json::to_vec(&config.hooks).expect("hooks serialise as JSON");
    for hook in &config.hooks {
        for arg in &hook.run {
            let Ok(target) = scripts_root.join(arg).canonicalize() else {
                continue;
            };
            if !target.starts_with(&scripts_root) || !target.is_file() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&target) else {
                continue;
            };
            material.extend(arg.as_bytes());
            material.extend(sha256_hex(&bytes).as_bytes());
        }
    }
    sha256_hex(&material)
}
