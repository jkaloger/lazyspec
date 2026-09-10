//! The `extends` probe (STORY-284, ITERATION-441): whether a `.lazyspec.toml`
//! declares only `extends`, so `Config::load` can resolve it before
//! `parse_inner` would refuse it for a missing `[[types]]`/`[[relationships]]`.
//!
//! `probe` reads the raw TOML text with `toml_edit` rather than deserializing
//! into `RawConfig`: the whole point is to see `extends` before anything else
//! about the config is judged.

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

/// The resolved location an `extends`-only config points at. `remote` and
/// `branch` are `Some` only when `extends` was a clone URL (ITERATION-443);
/// a local directory leaves both `None`. ITERATION-444 reads them back to
/// bring the clone current on `fetch`.
#[derive(Debug, Clone, Default)]
pub struct Extends {
    pub root: PathBuf,
    pub remote: Option<String>,
    pub branch: Option<String>,
}

impl Extends {
    /// `.lazyspec.toml` under `root`, wherever `root` came from -- a resolved
    /// local directory or a URL's managed clone. The one spelling every
    /// resolver and watcher joins against, so the join itself is never
    /// repeated open-coded.
    pub fn config_path(root: &Path) -> PathBuf {
        root.join(".lazyspec.toml")
    }
}

/// `extends`'s value, if the top-level table declares the key. `Ok(None)`
/// when absent -- including when `toml_edit` cannot parse the document at
/// all, since `toml_edit` rejects some shapes the `toml` crate tolerates
/// (e.g. redefining a key as an array-of-tables after an inline assignment);
/// `parse_inner`'s own `toml::from_str` is the source of truth for a syntax
/// error, and gives the friendlier message. Declaring `extends` beside any
/// other top-level key is a load error naming every sibling, in file order
/// (STORY-284 AC5); a non-string value is a load error too.
pub fn probe(toml_str: &str) -> Result<Option<String>> {
    let Ok(doc) = toml_str.parse::<toml_edit::DocumentMut>() else {
        return Ok(None);
    };

    let mut extends = None;
    let mut others = Vec::new();
    for (key, item) in doc.as_table().iter() {
        if key == "extends" {
            extends = Some(item.clone());
        } else {
            others.push(key.to_string());
        }
    }

    let Some(item) = extends else {
        return Ok(None);
    };

    if !others.is_empty() {
        let spec = item.as_str().unwrap_or("<extends value>");
        bail!(
            "extends = \"{}\" must be the only key in .lazyspec.toml; remove {}",
            spec,
            others.join(", ")
        );
    }

    match item.as_str() {
        Some(spec) => Ok(Some(spec.to_string())),
        None => bail!(
            "extends must be a string (a directory path); found {}",
            item.type_name()
        ),
    }
}

/// Resolve `spec` (an `extends` value) against `project_root`, the directory
/// containing the `.lazyspec.toml` that declared it: absolute as-is, relative
/// joined onto `project_root`, then lexically normalized so `config --json`
/// reports a clean path (AC7).
pub fn resolve_dir(project_root: &Path, spec: &str) -> PathBuf {
    let candidate = Path::new(spec);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        project_root.join(candidate)
    };
    crate::engine::store::normalize(&joined)
}

/// Whether `spec` (an `extends` value) names a clone URL rather than a local
/// directory: it contains `://`, or has a `:` before its first `/` (scp-style
/// `user@host:path`, which `git clone` also accepts). Anything else -- a bare
/// relative or absolute path -- is a directory (ITERATION-441).
///
/// ponytail: a two-rule heuristic, not a URL parser. Ceiling: a Windows drive
/// letter (`C:\x`) reads as scp-style and would misclassify; out of scope for
/// this darwin/linux tool (ITERATION-443 Out of scope).
pub fn is_url(spec: &str) -> bool {
    if spec.contains("://") {
        return true;
    }
    match (spec.find(':'), spec.find('/')) {
        (Some(colon), Some(slash)) => colon < slash,
        (Some(_), None) => true,
        _ => false,
    }
}

/// Split a URL `extends` spec on its last `#`, isolating the branch fragment
/// (`<url>#<branch>`) from the clone URL. `None` when there is no `#`, which
/// clones the remote's default branch.
pub fn split_fragment(spec: &str) -> (&str, Option<&str>) {
    match spec.rsplit_once('#') {
        Some((url, branch)) => (url, Some(branch)),
        None => (spec, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_extends_key_is_none() {
        let toml = "[[types]]\nname = \"rfc\"\n";
        assert_eq!(probe(toml).unwrap(), None);
    }

    #[test]
    fn extends_alone_is_its_value() {
        let toml = "extends = \"../x\"\n";
        assert_eq!(probe(toml).unwrap(), Some("../x".to_string()));
    }

    #[test]
    fn extends_beside_other_keys_names_them_in_file_order() {
        // TOML has no syntax to return to the root table after a `[[types]]`
        // header, so `types` is written as a top-level array of inline
        // tables here to keep it a genuine sibling of `extends` and
        // `naming`, in that file order.
        let toml =
            "types = [{ name = \"rfc\" }]\n\nextends = \"../x\"\n\n[naming]\npattern = \"x\"\n";
        let err = probe(toml).unwrap_err();
        let message = err.to_string();
        assert!(
            message.starts_with("extends = \"../x\" must be the only key"),
            "expected a grammatical sentence naming the spec, got: {message}"
        );
        let types_pos = message.find("types").unwrap();
        let naming_pos = message.find("naming").unwrap();
        assert!(
            types_pos < naming_pos,
            "expected \"types\" before \"naming\" in: {message}"
        );
    }

    #[test]
    fn non_string_extends_is_an_error() {
        let toml = "extends = 3\n";
        assert!(probe(toml).is_err());
    }

    #[test]
    fn relative_extends_resolves_against_project_root() {
        let root = Path::new("/tmp/project");
        assert_eq!(resolve_dir(root, "../shared"), PathBuf::from("/tmp/shared"));
    }

    #[test]
    fn absolute_extends_is_used_as_is() {
        let root = Path::new("/tmp/project");
        assert_eq!(
            resolve_dir(root, "/abs/shared"),
            PathBuf::from("/abs/shared")
        );
    }

    #[test]
    fn is_url_true_for_every_clone_url_shape() {
        assert!(is_url("https://h/o/r.git"));
        assert!(is_url("git@github.com:o/r.git"));
        assert!(is_url("file:///tmp/x"));
        assert!(is_url("ssh://git@h/o/r"));
    }

    #[test]
    fn is_url_false_for_every_local_dir_shape() {
        assert!(!is_url("../shared"));
        assert!(!is_url("/abs/dir"));
        assert!(!is_url("shared"));
    }

    #[test]
    fn split_fragment_isolates_the_trailing_branch() {
        assert_eq!(
            split_fragment("https://h/r.git#next"),
            ("https://h/r.git", Some("next"))
        );
    }

    #[test]
    fn split_fragment_with_no_hash_is_no_branch() {
        assert_eq!(split_fragment("https://h/r.git"), ("https://h/r.git", None));
    }
}
