use crate::cli::style::{error_prefix, warning_prefix};
use crate::engine::config::Config;
use crate::engine::gh::{AuthStatus, GhAuth, GhCli};
use crate::engine::store::Store;
use console::{colors_enabled, Style};

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

pub fn run_full(store: &Store, config: &Config, json: bool, warnings: bool) -> i32 {
    let result = store.validate_full(config);

    let gh_warnings = if config.documents.has_github_issues_types() {
        let gh = GhCli::new();
        gh_auth_warnings(&gh)
    } else {
        vec![]
    };

    if json {
        let output = run_json(store, config, &gh_warnings);
        println!("{}", output);
    } else {
        let output = run_human(store, config, warnings, &gh_warnings);
        if output.is_empty() {
            println!("{}", success_message());
        } else {
            eprint!("{}", output);
        }
    }

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

pub fn run_json(store: &Store, config: &Config, extra_warnings: &[String]) -> String {
    let result = store.validate_full(config);
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
    config: &Config,
    show_warnings: bool,
    extra_warnings: &[String],
) -> String {
    let result = store.validate_full(config);
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
        let output = run_human(&store, &config, true, &extra);
        assert!(output.contains("gh CLI is not installed"));
    }

    #[test]
    fn run_human_hides_gh_warnings_when_not_shown() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        let store = Store::load(dir.path(), &config).unwrap();
        let extra = vec!["gh CLI is not installed; github-issues types will not sync".to_string()];
        let output = run_human(&store, &config, false, &extra);
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
        let output: serde_json::Value =
            serde_json::from_str(&run_json(&store, &config, &extra)).unwrap();

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

        let output: serde_json::Value =
            serde_json::from_str(&run_json(&store, &config, &[])).unwrap();

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
}
