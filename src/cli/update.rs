use crate::engine::config::Config;
use crate::engine::git_ref::GitRefOps;
use crate::engine::hooks::HookEnv;
use crate::engine::store::Store;
use anyhow::{anyhow, bail, Result};
use std::path::Path;

pub use crate::engine::ops::update::{run_part, run_with_config};

const RESERVED_ATTR_KEYS: &[&str] = &["status", "title", "body", "author", "reviewed", "governs"];

/// What `update` prints: `message` to stdout, each of `warnings` (a git-ref
/// push that landed locally only, hook findings on a saved move) to stderr --
/// the same split every other command's push-outcome handling uses.
pub struct UpdateOutput {
    pub message: String,
    pub warnings: Vec<String>,
}

/// The `update` arguments as the user gave them.
pub struct UpdateArgs<'a> {
    pub path: &'a str,
    pub status: Option<&'a str>,
    pub title: Option<&'a str>,
    pub assignee: Option<&'a str>,
    pub body: Option<&'a str>,
    pub part: Option<&'a str>,
    pub attr: &'a [String],
    pub json: bool,
}

/// The whole `update` command. Lives here rather than main.rs per DICTUM-006
/// ("main.rs does wiring only").
pub fn run_cli(
    hooks: &HookEnv,
    cwd: &Path,
    config: &Config,
    store: &Store,
    args: UpdateArgs,
    git: &dyn GitRefOps,
) -> Result<UpdateOutput> {
    let UpdateArgs {
        path,
        status,
        title,
        assignee,
        body,
        part,
        attr,
        json,
    } = args;
    if let Some(part_name) = part {
        let has_conflicting_flags =
            status.is_some() || title.is_some() || assignee.is_some() || !attr.is_empty();
        return run_part_cli(
            cwd,
            config,
            store,
            path,
            part_name,
            body,
            has_conflicting_flags,
            git,
            json,
        );
    }

    let attr_pairs = parse_attr_pairs(attr)?;
    let updates: Vec<(&str, &str)> = [
        ("status", status),
        ("title", title),
        ("assignee", assignee),
        ("body", body),
    ]
    .into_iter()
    .filter_map(|(key, value)| Some((key, value?)))
    .chain(attr_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())))
    .collect();
    let resolved = crate::cli::resolve::resolve_to_path(store, path)?;
    let outcome = run_with_config(hooks, cwd, store, path, &updates, config, git)
        .map_err(|e| crate::cli::hook::exit_if_blocked(e, json))?;

    let mut warnings = Vec::new();
    let message = if json {
        let store = Store::load(cwd, config)?;
        let doc = crate::cli::resolve::resolve_shorthand_or_path(&store, path)?;
        let mut json_val = crate::cli::json::doc_to_json(doc);
        crate::cli::json::merge_push_outcome(&mut json_val, &outcome.push);
        if !outcome.findings.is_empty() {
            json_val["hook_findings"] = crate::cli::hook::findings_json(&outcome.findings);
        }
        serde_json::to_string_pretty(&json_val)?
    } else {
        warnings.extend(outcome.findings.iter().map(|f| f.to_string()));
        warnings.extend(outcome.push.warning().map(str::to_string));
        format!("Updated {}", resolved.display())
    };
    Ok(UpdateOutput { message, warnings })
}

#[allow(clippy::too_many_arguments)]
fn run_part_cli(
    cwd: &Path,
    config: &Config,
    store: &Store,
    path: &str,
    part_name: &str,
    body: Option<&str>,
    has_conflicting_flags: bool,
    git: &dyn GitRefOps,
    json: bool,
) -> Result<UpdateOutput> {
    if has_conflicting_flags {
        bail!(
            "'--part' cannot be combined with --status, --title, --assignee or --attr; \
             update the part on its own, then the document"
        );
    }
    let body = body.ok_or_else(|| anyhow!("'--part' requires --body or --body-file"))?;

    let push_outcome = run_part(cwd, config, store, path, part_name, body, git)?;
    let warnings = push_outcome
        .warning()
        .map(str::to_string)
        .into_iter()
        .collect();

    let message = if json {
        let store = Store::load(cwd, config)?;
        let doc = crate::cli::resolve::resolve_shorthand_or_path(&store, path)?;
        let mut json_val = crate::cli::json::doc_to_json(doc);
        crate::cli::json::merge_push_outcome(&mut json_val, &push_outcome);
        serde_json::to_string_pretty(&json_val)?
    } else {
        format!("Updated part {} of {}", part_name, path)
    };

    Ok(UpdateOutput { message, warnings })
}

/// Parse repeatable `--attr key=value` flags into owned `(key, value)` pairs.
///
/// Splits on the FIRST `=` so values may themselves contain `=`. A missing `=`,
/// an empty key, or a reserved field name (which has its own dedicated flag) is
/// an error.
pub fn parse_attr_pairs(raw: &[String]) -> Result<Vec<(String, String)>> {
    raw.iter()
        .map(|entry| {
            let (key, value) = entry
                .split_once('=')
                .ok_or_else(|| anyhow!("invalid --attr, expected key=value: {entry}"))?;
            if key.is_empty() {
                bail!("invalid --attr, empty key: {entry}");
            }
            if RESERVED_ATTR_KEYS.contains(&key) {
                bail!("'{key}' is a reserved frontmatter field and cannot be set via --attr; use --status, --title or --body, set author at `create`, and let `--status` or `pin` stamp reviewed; `governs` is edited with `govern add|remove`");
            }
            Ok((key.to_string(), value.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{Cli, Commands};
    use clap::Parser;

    // AC4: clap collects each --attr occurrence into a Vec.
    #[test]
    fn clap_collects_multiple_attr_flags() {
        let cli = Cli::try_parse_from([
            "lazyspec",
            "update",
            "STORY-1",
            "--attr",
            "owner=jkaloger",
            "--attr",
            "estimate=3",
        ])
        .unwrap();
        match cli.command {
            Some(Commands::Update { attr, .. }) => {
                assert_eq!(attr, vec!["owner=jkaloger", "estimate=3"]);
            }
            _ => panic!("expected Update command"),
        }
    }

    #[test]
    fn parse_attr_pairs_basic() {
        let pairs = parse_attr_pairs(&["owner=jkaloger".to_string()]).unwrap();
        assert_eq!(pairs, vec![("owner".to_string(), "jkaloger".to_string())]);
    }

    // Edge: split on the FIRST '=' so the value may contain '='.
    #[test]
    fn parse_attr_pairs_value_with_equals() {
        let pairs = parse_attr_pairs(&["k=a=b".to_string()]).unwrap();
        assert_eq!(pairs, vec![("k".to_string(), "a=b".to_string())]);
    }

    // Edge: missing '=' bails.
    #[test]
    fn parse_attr_pairs_missing_equals_bails() {
        let err = parse_attr_pairs(&["badpair".to_string()]).unwrap_err();
        assert!(err.to_string().contains("expected key=value"), "got: {err}");
    }

    // Edge: empty key bails.
    #[test]
    fn parse_attr_pairs_empty_key_bails() {
        let err = parse_attr_pairs(&["=v".to_string()]).unwrap_err();
        assert!(err.to_string().contains("empty key"), "got: {err}");
    }

    #[test]
    fn parse_attr_pairs_reserved_key_bails() {
        let err = parse_attr_pairs(&["status=done".to_string()]).unwrap_err();
        assert!(err.to_string().contains("reserved"), "got: {err}");
    }

    /// `reviewed` is the staleness anchor (RFC-069): `--status` and `pin` stamp
    /// it from HEAD. Hand-setting it via `--attr` would forge a review.
    #[test]
    fn parse_attr_pairs_refuses_reviewed() {
        let err = parse_attr_pairs(&["reviewed=deadbeef".to_string()]).unwrap_err();
        assert!(err.to_string().contains("reserved"), "got: {err}");
    }

    /// `governs` is a list field with its own verb (BUG-029). Refusing it here
    /// points at that verb rather than failing later as an unknown attribute.
    #[test]
    fn parse_attr_pairs_refuses_governs_and_names_the_verb() {
        let err = parse_attr_pairs(&["governs=src/**".to_string()]).unwrap_err();
        assert!(err.to_string().contains("govern add"), "got: {err}");
    }
}
