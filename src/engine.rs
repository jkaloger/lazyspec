pub mod cache;
pub mod cache_lock;
pub mod certification;
pub mod clickup;
pub mod clickup_cache;
pub mod config;
pub mod config_write;
pub mod context;
pub mod credentials;
pub mod doc_json;
pub mod document;
pub mod fs;
pub mod fs_ops;
pub mod gh;
pub mod gh_fetch;
pub mod gh_schema;
pub mod gh_subissue;
pub mod git_ref;
pub mod git_ref_store;
pub mod git_status;
pub mod git_store;
pub mod github;
pub mod github_url;
pub mod graph;
pub mod hashing;
pub mod hooks;
pub mod issue_body;
pub mod issue_cache;
pub mod issue_map;
pub mod milestone_cache;
pub mod ops;
pub mod pre_transition;
pub mod prompt;
pub mod provenance;
pub mod refs;
pub mod reservation;
pub mod skills;
pub mod staleness;
pub mod staleness_cache;
pub mod status_colors;
pub mod store;
pub mod store_dispatch;
pub mod subprocess;
pub mod symbols;
pub mod sync;
pub mod task_map;
pub mod template;
pub mod traversal;
pub mod validation;
pub mod watch;

use std::ffi::OsString;
use std::path::PathBuf;

/// User-local lazyspec state (trust, cache, credentials): `$LAZYSPEC_STATE_DIR`,
/// else `$HOME/.lazyspec`.
pub fn user_state_dir() -> PathBuf {
    resolve_state_dir(
        std::env::var_os("LAZYSPEC_STATE_DIR"),
        std::env::var_os("HOME"),
    )
}

fn resolve_state_dir(state_dir: Option<OsString>, home: Option<OsString>) -> PathBuf {
    if let Some(dir) = state_dir {
        return PathBuf::from(dir);
    }
    PathBuf::from(home.unwrap_or_else(|| ".".into())).join(".lazyspec")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_override_wins_over_home() {
        let dir = resolve_state_dir(Some("/state".into()), Some("/home/me".into()));
        assert_eq!(dir, PathBuf::from("/state"));
    }

    #[test]
    fn state_dir_defaults_under_home() {
        let dir = resolve_state_dir(None, Some("/home/me".into()));
        assert_eq!(dir, PathBuf::from("/home/me/.lazyspec"));
    }

    #[test]
    fn state_dir_without_home_is_relative() {
        assert_eq!(resolve_state_dir(None, None), PathBuf::from("./.lazyspec"));
    }
}
