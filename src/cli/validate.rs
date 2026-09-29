use crate::cli::resolve::resolve_shorthand_or_path;
use crate::cli::style::{error_prefix, warning_prefix};
use crate::engine::config::Config;
use crate::engine::gh::{AuthStatus, GhAuth, GhCli};
use crate::engine::hooks::HookEnv;
use crate::engine::store::Store;
use crate::engine::validation::{validate_full, ValidationIssue, ValidationResult};
use console::{colors_enabled, Style};
use std::path::Path;

fn success_message() -> String {
    if colors_enabled() {
        format!(
            "{} All documents valid.",
            Style::new().green().bold().apply_to("\u{2713}")
        )
    } else {
        "All documents valid.".to_string()
    }
}

pub fn gh_auth_warnings(gh: &dyn GhAuth) -> Vec<String> {
    match gh.auth_status() {
        Ok(AuthStatus::GhNotInstalled) => {
            vec!["gh CLI is not installed; github-issues types will not sync".to_string()]
        }
        Ok(AuthStatus::NotAuthenticated(msg)) => {
            vec![format!(
                "gh is not authenticated; github-issues types will not sync ({})",
                msg
            )]
        }
        Ok(AuthStatus::Authenticated { .. }) => vec![],
        Err(e) => {
            vec![format!("could not check gh auth status: {}", e)]
        }
    }
}

/// `validate --id <id>` (RFC-074 AC2): every finding `validate` would raise
/// over the whole store, scoped down to the one document `id` resolves to.
/// A rule with nothing to say about a single document (a type-level finding
/// like `singleton-violation`, or `governs-unowned`, about a source file
/// rather than a document) drops out rather than firing on every id; a rule
/// whose finding names several documents (`duplicate-id`) survives when the
/// target is any one of them.
pub fn run_full(
    store: &Store,
    config: &Config,
    hooks: &HookEnv,
    id: Option<&str>,
    json: bool,
    warnings: bool,
) -> anyhow::Result<i32> {
    let scope = id.map(|id| resolve_scope_path(store, id)).transpose()?;

    let mut result = validate_full(store, config, hooks);
    if let Some(target) = &scope {
        scope_to_path(&mut result, target);
    }

    let gh_warnings = if config.documents.has_github_issues_types() {
        let gh = GhCli::new();
        gh_auth_warnings(&gh)
    } else {
        vec![]
    };

    if json {
        let output = run_json(store, &result, &gh_warnings);
        println!("{}", output);
    } else {
        let output = run_human(store, &result, warnings, &gh_warnings);
        if output.is_empty() {
            println!("{}", success_message());
        } else {
            eprint!("{}", output);
        }
    }

    Ok(exit_code(store, &result))
}

/// The path `--id` names, from a shorthand ID or a literal path. Mirrors
/// `show`'s own resolution (`src/cli/show.rs`): an ambiguous prefix is a hard
/// error here, since (unlike `show`) there is no reasonable output to fall
/// back to for the scoping half of the command.
fn resolve_scope_path(store: &Store, id: &str) -> anyhow::Result<std::path::PathBuf> {
    resolve_shorthand_or_path(store, id)
        .map(|doc| doc.path.clone())
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Whether `issue`'s rendered finding names `target` at all: its `path`
/// field equals it, its `paths` field contains it, or its `source` field
/// equals it (`broken-link` names the document the broken reference lives
/// in as `source`, not `path`). Read off the same JSON
/// [`ValidationIssue::to_json`] emits, rather than a match over every
/// variant, so a rule added later is scoped by `--id` with no change here.
fn issue_names_path(issue: &ValidationIssue, target: &str) -> bool {
    let json = issue.to_json();
    if json.get("path").and_then(|v| v.as_str()) == Some(target) {
        return true;
    }
    if json.get("source").and_then(|v| v.as_str()) == Some(target) {
        return true;
    }
    json.get("paths")
        .and_then(|v| v.as_array())
        .is_some_and(|paths| paths.iter().any(|p| p.as_str() == Some(target)))
}

fn scope_to_path(result: &mut ValidationResult, target: &Path) {
    let target = target.display().to_string();
    result
        .errors
        .retain(|issue| issue_names_path(issue, &target));
    result
        .warnings
        .retain(|issue| issue_names_path(issue, &target));
}

/// The process exit code a validated store yields: anything that made it into
/// `errors`, a severity the config chooses, fails the run. Separate from
/// [`run_full`] so a test can assert it without a report on stdout.
fn exit_code(store: &Store, result: &ValidationResult) -> i32 {
    if result.errors.is_empty() && store.parse_errors().is_empty() {
        0
    } else {
        2
    }
}

/// The slug for the warnings `gh_auth_warnings` raises. They are environment
/// findings rather than [`ValidationIssue`](crate::engine::validation::ValidationIssue)s,
/// but they travel in the same array, so they carry the same `rule`/`message`
/// shape and one slug for all of them.
pub const GH_AUTH_RULE: &str = "gh-auth";

/// Renders the result [`run_full`] already computed. Both renders take it
/// rather than a [`Config`] to validate against a second time: with the `stale`
/// rule live, a re-validation is another `git diff` per pinned document.
pub fn run_json(store: &Store, result: &ValidationResult, extra_warnings: &[String]) -> String {
    let errors: Vec<_> = result.errors.iter().map(|e| e.to_json()).collect();
    let mut warnings: Vec<_> = result.warnings.iter().map(|w| w.to_json()).collect();
    warnings.extend(
        extra_warnings
            .iter()
            .map(|w| serde_json::json!({ "rule": GH_AUTH_RULE, "message": w })),
    );
    let parse_errors: Vec<_> = store
        .parse_errors()
        .iter()
        .map(|pe| serde_json::json!({ "path": pe.path.display().to_string(), "error": pe.error }))
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "errors": errors,
        "warnings": warnings,
        "parse_errors": parse_errors,
    }))
    .unwrap()
}

pub fn run_human(
    store: &Store,
    result: &ValidationResult,
    show_warnings: bool,
    extra_warnings: &[String],
) -> String {
    let mut output = String::new();

    for pe in store.parse_errors() {
        output.push_str(&format!(
            "  {} parse error in {}: {}\n",
            error_prefix(),
            pe.path.display(),
            pe.error
        ));
    }
    for error in &result.errors {
        output.push_str(&format!("  {} {}\n", error_prefix(), error));
    }
    if show_warnings {
        for warning in &result.warnings {
            output.push_str(&format!("  {} {}\n", warning_prefix(), warning));
        }
        for warning in extra_warnings {
            output.push_str(&format!("  {} {}\n", warning_prefix(), warning));
        }
    }

    output
}

#[cfg(test)]
mod stale_tests {
    use super::*;
    use crate::engine::config::{StalenessConfig, StalenessFinding};
    use chrono::{Duration, Utc};

    /// One document per band, banded by the default `age` driver off its own
    /// date -- so no git subprocess runs and the bands are the fixture's to
    /// choose. Mirrors the engine's own stale fixture.
    fn one_of_each_band(config: &Config) -> (tempfile::TempDir, Store) {
        let dated = |title: &str, age_days: i64| {
            let date = Utc::now().date_naive() - Duration::days(age_days);
            format!(
                "---\ntitle: \"{title}\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: {date}\ntags: []\ngoverns: []\nrelated: []\n---\n\nbody\n"
            )
        };
        crate::engine::store::test_support::store_from_with_config(
            &[
                ("docs/rfcs/RFC-001-fresh.md", &dated("Fresh", 10)),
                ("docs/rfcs/RFC-002-aging.md", &dated("Aging", 100)),
                ("docs/rfcs/RFC-003-stale.md", &dated("Stale", 200)),
            ],
            config,
        )
    }

    fn config_with(finding: StalenessFinding) -> Config {
        Config {
            staleness: StalenessConfig {
                finding,
                ..StalenessConfig::default()
            },
            ..Config::default()
        }
    }

    fn findings_of(output: &serde_json::Value, array: &str) -> Vec<serde_json::Value> {
        output[array]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["rule"] == "stale")
            .cloned()
            .collect()
    }

    /// AC1: of three bands only `stale` is a finding, and the object it carries
    /// is `rule`, the document's `path`, and the same `staleness` object
    /// `show --json` prints -- flat beside them, not nested under the variant.
    #[test]
    fn run_json_gives_the_stale_document_its_rule_path_and_staleness() {
        let config = config_with(StalenessFinding::Warning);
        let (_tmp, store) = one_of_each_band(&config);
        let result = store.validate_full(&config, &crate::engine::hooks::HookEnv::disabled());

        let output: serde_json::Value =
            serde_json::from_str(&run_json(&store, &result, &[])).unwrap();

        let stale = findings_of(&output, "warnings");
        assert_eq!(stale.len(), 1, "got {:?}", output["warnings"]);
        assert_eq!(stale[0]["path"], "docs/rfcs/RFC-003-stale.md");
        assert_eq!(stale[0]["staleness"]["band"], "stale");
        assert_eq!(stale[0]["staleness"]["driver"], "age");
        assert_eq!(stale[0]["staleness"]["age_days"], 200);
        assert_eq!(
            stale[0]["staleness"]["drift"],
            serde_json::json!({"files": 0, "insertions": 0, "deletions": 0})
        );
        assert!(findings_of(&output, "errors").is_empty());

        let issue = result
            .warnings
            .iter()
            .find(|w| w.rule() == "stale")
            .expect("the stale document reaches validate_full");
        assert_eq!(stale[0]["message"], issue.to_string());
    }

    /// AC2: `finding` picks the array and, with it, the exit code. Human render
    /// too, because a warning-only run prints nothing without `--warnings`.
    #[test]
    fn the_configured_finding_severity_decides_the_array_and_the_exit_code() {
        for (finding, array, exit) in [
            (StalenessFinding::Warning, "warnings", 0),
            (StalenessFinding::Error, "errors", 2),
        ] {
            let config = config_with(finding);
            let (_tmp, store) = one_of_each_band(&config);
            let result = store.validate_full(&config, &crate::engine::hooks::HookEnv::disabled());

            let output: serde_json::Value =
                serde_json::from_str(&run_json(&store, &result, &[])).unwrap();
            assert_eq!(
                findings_of(&output, array).len(),
                1,
                "{finding:?}: {output}"
            );

            assert_eq!(exit_code(&store, &result), exit, "{finding:?}");
            assert!(run_human(&store, &result, true, &[])
                .contains("docs/rfcs/RFC-003-stale.md is stale"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::gh::{test_support::MockGhClient, AuthStatus};

    #[test]
    fn gh_auth_warnings_when_not_installed() {
        let gh = MockGhClient::new().with_auth(AuthStatus::GhNotInstalled);
        let warnings = gh_auth_warnings(&gh);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not installed"));
    }

    #[test]
    fn gh_auth_warnings_when_not_authenticated() {
        let gh = MockGhClient::new()
            .with_auth(AuthStatus::NotAuthenticated("token expired".to_string()));
        let warnings = gh_auth_warnings(&gh);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("not authenticated"));
        assert!(warnings[0].contains("token expired"));
    }

    #[test]
    fn gh_auth_warnings_when_authenticated() {
        let gh = MockGhClient::new();
        let warnings = gh_auth_warnings(&gh);
        assert!(warnings.is_empty());
    }

    #[test]
    fn run_human_includes_gh_warnings_when_shown() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let store = Store::load(dir.path(), &config).unwrap();
        let extra = vec!["gh CLI is not installed; github-issues types will not sync".to_string()];
        let output = run_human(
            &store,
            &store.validate_full(&config, &crate::engine::hooks::HookEnv::disabled()),
            true,
            &extra,
        );
        assert!(output.contains("gh CLI is not installed"));
    }

    #[test]
    fn run_human_hides_gh_warnings_when_not_shown() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let store = Store::load(dir.path(), &config).unwrap();
        let extra = vec!["gh CLI is not installed; github-issues types will not sync".to_string()];
        let output = run_human(
            &store,
            &store.validate_full(&config, &crate::engine::hooks::HookEnv::disabled()),
            false,
            &extra,
        );
        assert!(!output.contains("gh CLI is not installed"));
    }

    /// The one warning in `warnings` that names no document. The README and the
    /// skill prose both tell agents so; this is what keeps that true.
    #[test]
    fn run_json_gives_gh_warnings_the_gh_auth_rule_and_no_document_field() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let store = Store::load(dir.path(), &config).unwrap();
        let extra = vec!["gh not installed warning".to_string()];
        let output: serde_json::Value = serde_json::from_str(&run_json(
            &store,
            &store.validate_full(&config, &crate::engine::hooks::HookEnv::disabled()),
            &extra,
        ))
        .unwrap();

        let warning = &output["warnings"][0];
        assert_eq!(warning["rule"], GH_AUTH_RULE);
        assert_eq!(warning["message"], "gh not installed warning");
        assert_eq!(
            warning.as_object().unwrap().keys().collect::<Vec<_>>(),
            ["message", "rule"]
        );
    }

    /// The repair fields (`renamed`, `suggested_glob`) ride along empty until
    /// STORY-271 fills them; what an agent selects on today is `rule`, and what
    /// it reads back is `path` and `glob`.
    #[test]
    fn run_json_gives_a_rotted_pin_its_rule_path_and_glob() {
        let tmp = crate::engine::store::test_support::write_docs(&[(
            "docs/rfcs/RFC-001-engine.md",
            "---\ntitle: \"Engine\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-09-01\ntags: []\ngoverns:\n  - src/gone/**\nrelated: []\n---\n\nbody\n",
        )]);
        let config = Config::default();
        let store = Store::load(tmp.path(), &config).unwrap();

        let output: serde_json::Value = serde_json::from_str(&run_json(
            &store,
            &store.validate_full(&config, &crate::engine::hooks::HookEnv::disabled()),
            &[],
        ))
        .unwrap();

        let finding = output["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["rule"] == "governs-no-match")
            .expect("the pin matches nothing, so validate --json should carry the finding");
        assert_eq!(finding["path"], "docs/rfcs/RFC-001-engine.md");
        assert_eq!(finding["glob"], "src/gone/**");
        assert_eq!(
            finding["message"],
            "governs glob in docs/rfcs/RFC-001-engine.md: \"src/gone/**\" matches no file"
        );
    }

    /// What `jq 'select(.rule=="governs-unowned") | .file'` reads back: the
    /// slug to select on and the path to open, relative to the code root.
    #[test]
    fn run_json_gives_an_unowned_file_its_rule_and_file() {
        let tmp = crate::engine::store::test_support::write_docs(&[
            (
                "docs/rfcs/RFC-001-engine.md",
                "---\ntitle: \"Engine\"\ntype: rfc\nstatus: draft\nauthor: t\ndate: 2026-09-01\ntags: []\ngoverns:\n  - src/engine/**\nrelated: []\n---\n\nbody\n",
            ),
            ("src/engine/store.rs", "fn main() {}\n"),
            ("src/cli/show.rs", "fn main() {}\n"),
        ]);
        let config = Config {
            governs: crate::engine::config::GovernsConfig {
                scope: vec!["src/**".to_string()],
                unowned: Some(crate::engine::config::Severity::Warning),
                ..Default::default()
            },
            ..Config::default()
        };
        let store = Store::load(tmp.path(), &config).unwrap();

        let output: serde_json::Value = serde_json::from_str(&run_json(
            &store,
            &store.validate_full(&config, &crate::engine::hooks::HookEnv::disabled()),
            &[],
        ))
        .unwrap();

        let unowned: Vec<&serde_json::Value> = output["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["rule"] == "governs-unowned")
            .collect();
        assert_eq!(unowned.len(), 1, "got {:?}", output["warnings"]);
        assert_eq!(unowned[0]["file"], "src/cli/show.rs");
        assert_eq!(
            unowned[0]["message"],
            "src/cli/show.rs is in [governs] scope but no document governs it"
        );
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use std::path::PathBuf;

    fn story_path() -> PathBuf {
        PathBuf::from("docs/stories/STORY-001-a.md")
    }

    fn other_path() -> PathBuf {
        PathBuf::from("docs/stories/STORY-002-b.md")
    }

    // AC2: `--id` resolves a shorthand ID to the document's own path, the
    // same resolution `show` uses.
    #[test]
    fn resolve_scope_path_resolves_a_shorthand_id() {
        let tmp = crate::engine::store::test_support::write_docs(&[(
            "docs/stories/STORY-001-a.md",
            "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-09-01\ntags: []\nrelated: []\n---\n\nbody\n",
        )]);
        let store = Store::load(tmp.path(), &Config::default()).unwrap();

        let path = resolve_scope_path(&store, "STORY-001").unwrap();

        assert_eq!(path, story_path());
    }

    #[test]
    fn resolve_scope_path_errors_on_an_unknown_id() {
        let tmp = crate::engine::store::test_support::write_docs(&[]);
        let store = Store::load(tmp.path(), &Config::default()).unwrap();

        let err = resolve_scope_path(&store, "STORY-999").unwrap_err();

        assert!(err.to_string().contains("STORY-999"), "got: {err}");
    }

    // AC2: a finding whose `path` names the target document is scoped in.
    #[test]
    fn issue_names_path_matches_a_path_field() {
        let issue = ValidationIssue::InvalidAcSlug {
            path: story_path(),
            slug: "Bad Slug".to_string(),
            reason: "empty AC slug".to_string(),
        };

        assert!(issue_names_path(&issue, "docs/stories/STORY-001-a.md"));
        assert!(!issue_names_path(&issue, "docs/stories/STORY-002-b.md"));
    }

    // AC2: `duplicate-id` names several documents; the target matching any
    // one of them scopes the finding in.
    #[test]
    fn issue_names_path_matches_any_entry_in_a_paths_field() {
        let issue = ValidationIssue::DuplicateId {
            id: "STORY-001".to_string(),
            paths: vec![story_path(), other_path()],
        };

        assert!(issue_names_path(&issue, "docs/stories/STORY-002-b.md"));
        assert!(!issue_names_path(&issue, "docs/stories/STORY-003-c.md"));
    }

    // AC2: `broken-link` names the document the broken reference lives in as
    // `source`, not `path`; `--id` on that document must still scope it in.
    #[test]
    fn issue_names_path_matches_a_broken_links_source_field() {
        let issue = ValidationIssue::BrokenLink {
            source: story_path(),
            target: "STORY-999".to_string(),
        };

        assert!(issue_names_path(&issue, "docs/stories/STORY-001-a.md"));
        assert!(!issue_names_path(&issue, "docs/stories/STORY-002-b.md"));
    }

    // A finding with no document at all (`governs-unowned` names a source
    // file, not a document) never matches any `--id`.
    #[test]
    fn issue_names_path_never_matches_a_finding_naming_no_document() {
        let issue = ValidationIssue::GovernsUnowned {
            file: PathBuf::from("src/engine/orphan.rs"),
        };

        assert!(!issue_names_path(&issue, "src/engine/orphan.rs"));
    }

    // AC2: scoping a `ValidationResult` drops every finding that does not
    // name the target, from both `errors` and `warnings`.
    #[test]
    fn scope_to_path_keeps_only_findings_naming_the_target() {
        let mut result = ValidationResult {
            errors: vec![ValidationIssue::DuplicateId {
                id: "STORY-001".to_string(),
                paths: vec![story_path(), other_path()],
            }],
            warnings: vec![
                ValidationIssue::InvalidAcSlug {
                    path: story_path(),
                    slug: "Bad Slug".to_string(),
                    reason: "empty AC slug".to_string(),
                },
                ValidationIssue::InvalidAcSlug {
                    path: other_path(),
                    slug: "Other Slug".to_string(),
                    reason: "empty AC slug".to_string(),
                },
            ],
        };

        scope_to_path(&mut result, &story_path());

        assert_eq!(result.errors.len(), 1, "got {:?}", result.errors);
        assert_eq!(result.warnings.len(), 1, "got {:?}", result.warnings);
        assert!(matches!(
            &result.warnings[0],
            ValidationIssue::InvalidAcSlug { path, .. } if path == &story_path()
        ));
    }
}
