use lazyspec::engine::config::{Config, Severity, ValidationRule};
use lazyspec::engine::fs::RealFileSystem;
use tempfile::TempDir;

/// A pre-migration `.lazyspec.toml`: a valid `[[types]]`-only config with an
/// extra `[github]` section, NO `[[relationships]]` and NO `[[rules]]` — what an
/// upgraded legacy project looks like before `fix --config`.
const PRE_MIGRATION_CONFIG: &str = r#"[github]
repo = "owner/repo"

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
icon = "▲"

[[types]]
name = "iteration"
plural = "iterations"
dir = "docs/iterations"
prefix = "ITERATION"
icon = "◆"

[[types]]
name = "adr"
plural = "adrs"
dir = "docs/adrs"
prefix = "ADR"
icon = "■"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

struct ConfigFixture {
    dir: TempDir,
}

impl ConfigFixture {
    fn new(config: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
        std::fs::create_dir_all(root.join("docs/stories")).unwrap();
        std::fs::create_dir_all(root.join("docs/iterations")).unwrap();
        std::fs::create_dir_all(root.join("docs/adrs")).unwrap();
        std::fs::write(root.join(".lazyspec.toml"), config).unwrap();
        Self { dir }
    }

    fn pre_migration() -> Self {
        Self::new(PRE_MIGRATION_CONFIG)
    }

    fn root(&self) -> &std::path::Path {
        self.dir.path()
    }

    fn config_bytes(&self) -> Vec<u8> {
        std::fs::read(self.root().join(".lazyspec.toml")).unwrap()
    }

    fn config_text(&self) -> String {
        std::fs::read_to_string(self.root().join(".lazyspec.toml")).unwrap()
    }
}

fn rule_name(rule: &ValidationRule) -> &str {
    match rule {
        ValidationRule::ParentChild { name, .. } => name,
        ValidationRule::RelationExistence { name, .. } => name,
    }
}

// AC1 — injects standard blocks.
#[test]
fn fix_config_injects_relationships_and_rules() {
    let fixture = ConfigFixture::pre_migration();

    let code = lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    assert_eq!(code, 0);

    let text = fixture.config_text();
    let config = Config::parse(&text).expect("repaired config must parse strictly");

    // Exactly 4 relationships with the standard names/inverses.
    assert_eq!(config.relationships.len(), 4);
    let implements = config.relationship_by_name("implements").unwrap();
    assert_eq!(implements.inverse.as_deref(), Some("implemented-by"));
    let supersedes = config.relationship_by_name("supersedes").unwrap();
    assert_eq!(supersedes.inverse.as_deref(), Some("superseded-by"));
    let blocks = config.relationship_by_name("blocks").unwrap();
    assert_eq!(blocks.inverse.as_deref(), Some("blocked-by"));
    let related = config.relationship_by_name("related-to").unwrap();
    assert_eq!(related.inverse, None, "related-to is symmetric");

    // Exactly 3 rules with the standard names/shapes/severities.
    assert_eq!(config.rules.len(), 3);
    let by_name = |name: &str| config.rules.iter().find(|r| rule_name(r) == name).unwrap();
    match by_name("stories-need-rfcs") {
        ValidationRule::ParentChild {
            child,
            parent,
            link,
            severity,
            ..
        } => {
            assert_eq!(child, "story");
            assert_eq!(parent, "rfc");
            assert_eq!(link, "implements");
            assert_eq!(*severity, Severity::Warning);
        }
        other => panic!("unexpected shape: {other:?}"),
    }
    match by_name("iterations-need-stories") {
        ValidationRule::ParentChild {
            child,
            parent,
            link,
            severity,
            ..
        } => {
            assert_eq!(child, "iteration");
            assert_eq!(parent, "story");
            assert_eq!(link, "implements");
            assert_eq!(*severity, Severity::Error);
        }
        other => panic!("unexpected shape: {other:?}"),
    }
    match by_name("adrs-need-relations") {
        ValidationRule::RelationExistence {
            doc_type,
            require,
            severity,
            ..
        } => {
            assert_eq!(doc_type, "adr");
            assert_eq!(require, "any-relation");
            assert_eq!(*severity, Severity::Error);
        }
        other => panic!("unexpected shape: {other:?}"),
    }

    // Existing [[types]] preserved.
    assert_eq!(config.documents.types.len(), 4);
    assert!(config.type_by_name("rfc").is_some());
    assert!(config.type_by_name("adr").is_some());
    // The pre-existing [github] section is preserved.
    assert!(text.contains("[github]"));
    assert!(text.contains("repo = \"owner/repo\""));
}

// AC2 — dry-run reports but does not write.
#[test]
fn fix_config_dry_run_leaves_file_unchanged() {
    let fixture = ConfigFixture::pre_migration();
    let original = fixture.config_bytes();

    let output = lazyspec::cli::fix::run_config_human(fixture.root(), true, &RealFileSystem);

    // Reports the additions as "would add".
    assert!(
        output.contains("Would add relationship implements"),
        "{output}"
    );
    assert!(
        output.contains("Would add relationship supersedes"),
        "{output}"
    );
    assert!(output.contains("Would add relationship blocks"), "{output}");
    assert!(
        output.contains("Would add relationship related-to"),
        "{output}"
    );
    assert!(
        output.contains("Would add rule stories-need-rfcs"),
        "{output}"
    );
    assert!(
        output.contains("Would add rule iterations-need-stories"),
        "{output}"
    );
    assert!(
        output.contains("Would add rule adrs-need-relations"),
        "{output}"
    );

    // File is byte-for-byte unchanged.
    assert_eq!(fixture.config_bytes(), original);
}

// AC3 — repaired config loads under strict load.
#[test]
fn fix_config_result_passes_strict_load() {
    let fixture = ConfigFixture::pre_migration();

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    // Strict load path returns Ok.
    let config = Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
    assert_eq!(config.relationships.len(), 4);
    assert_eq!(config.rules.len(), 3);
    // A sample relationship reference resolves against the injected registry.
    assert_eq!(
        config.resolve_relationship("implements").unwrap(),
        ("implements".to_string(), false)
    );
    assert_eq!(
        config.resolve_relationship("implemented-by").unwrap(),
        ("implements".to_string(), true)
    );
}

// AC4 — config-only scope, no documents touched.
#[test]
fn fix_config_does_not_touch_documents() {
    let fixture = ConfigFixture::pre_migration();
    // A document with deliberately incomplete frontmatter that plain `fix` would rewrite.
    let broken = "---\ntitle: \"Broken\"\ntype: rfc\n---\n";
    let doc_path = fixture.root().join("docs/rfcs/RFC-broken.md");
    std::fs::write(&doc_path, broken).unwrap();
    let doc_original = std::fs::read(&doc_path).unwrap();

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    // Config was modified...
    let config = Config::parse(&fixture.config_text()).unwrap();
    assert_eq!(config.relationships.len(), 4);
    assert_eq!(config.rules.len(), 3);
    // ...but the broken document is byte-for-byte unchanged.
    assert_eq!(std::fs::read(&doc_path).unwrap(), doc_original);
}

// AC5 — strict-load error names `lazyspec fix`.
#[test]
fn strict_load_error_names_fix() {
    let err = Config::parse(PRE_MIGRATION_CONFIG).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("lazyspec fix"),
        "strict-load error should name the remedy, got: {msg}"
    );
}

// AC6 — idempotent (run twice, no change).
#[test]
fn fix_config_idempotent() {
    let fixture = ConfigFixture::pre_migration();

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    let after_first = fixture.config_bytes();

    // Second run reports zero additions.
    let output = lazyspec::cli::fix::run_config_human(fixture.root(), false, &RealFileSystem);
    assert!(
        !output.contains("Added relationship"),
        "second run should add no relationships, got: {output}"
    );
    assert!(
        !output.contains("Added rule"),
        "second run should add no rules, got: {output}"
    );
    assert!(
        output.contains("already up to date"),
        "second run should report no-op, got: {output}"
    );

    // File byte-for-byte identical to post-first-run.
    assert_eq!(fixture.config_bytes(), after_first);
}

// AC6 companion — dry-run on already-migrated config reports nothing to add.
#[test]
fn fix_config_idempotent_dry_run() {
    let fixture = ConfigFixture::pre_migration();
    // Migrate first.
    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    let after_migrate = fixture.config_bytes();

    let output = lazyspec::cli::fix::run_config_human(fixture.root(), true, &RealFileSystem);
    assert!(
        !output.contains("Would add relationship"),
        "dry-run on migrated config should add no relationships, got: {output}"
    );
    assert!(
        !output.contains("Would add rule"),
        "dry-run on migrated config should add no rules, got: {output}"
    );
    assert!(
        output.contains("already up to date"),
        "dry-run on migrated config should report no-op, got: {output}"
    );

    // File untouched.
    assert_eq!(fixture.config_bytes(), after_migrate);
}

// AC6 — default-lifecycle injection on a lifecycle-less config: every type ends
// with a non-empty lifecycle and the result re-parses under strict load.
#[test]
fn fix_config_injects_default_lifecycle() {
    let fixture = ConfigFixture::pre_migration();

    let code = lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    assert_eq!(code, 0);

    let config = Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
    for t in &config.documents.types {
        assert!(
            !t.lifecycle.states.is_empty(),
            "type {} should have a lifecycle after migration",
            t.name
        );
    }
    // The default lifecycle carries the seven prior statuses.
    let rfc = config.type_by_name("rfc").unwrap();
    for state in [
        "draft",
        "review",
        "accepted",
        "in-progress",
        "complete",
        "rejected",
        "superseded",
    ] {
        assert!(
            rfc.lifecycle.states.iter().any(|s| s == state),
            "missing state {state}"
        );
    }
    assert!(!rfc.lifecycle.edges.is_empty());
}

// AC6 — the JSON result lists each migrated type in `lifecycles_added`.
#[test]
fn fix_config_reports_lifecycles_added() {
    let fixture = ConfigFixture::pre_migration();

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), true, &RealFileSystem);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let added: Vec<&str> = parsed["lifecycles_added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for name in ["rfc", "story", "iteration", "adr"] {
        assert!(
            added.contains(&name),
            "lifecycles_added missing {name}: {json}"
        );
    }
}

// AC6 — idempotent: re-running adds no lifecycles and does not rewrite the file.
#[test]
fn fix_config_lifecycle_injection_idempotent() {
    let fixture = ConfigFixture::pre_migration();
    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    let after_first = fixture.config_bytes();

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), false, &RealFileSystem);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["lifecycles_added"].as_array().unwrap().is_empty(),
        "second run should add no lifecycles: {json}"
    );
    assert!(!parsed["written"].as_bool().unwrap());
    assert_eq!(fixture.config_bytes(), after_first);
}

// AC6 — comments and unrelated sections survive the migration.
#[test]
fn fix_config_migration_preserves_user_content() {
    let config_with_comment = r#"# my project config
[github]
repo = "owner/repo"

# the rfc type
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;
    let fixture = ConfigFixture::new(config_with_comment);

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    let text = fixture.config_text();
    assert!(text.contains("# my project config"));
    assert!(text.contains("# the rfc type"));
    assert!(text.contains("[github]"));
    assert!(text.contains("repo = \"owner/repo\""));
    Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
}

// AC6 — a type that already declares a lifecycle is left untouched.
#[test]
fn fix_config_leaves_existing_lifecycle_untouched() {
    let config_with_lifecycle = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
lifecycle = { states = ["open", "closed"], edges = [{ from = "open", to = "closed" }] }

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "supersedes"
inverse = "superseded-by"

[[relationships]]
name = "blocks"
inverse = "blocked-by"

[[relationships]]
name = "related-to"
"#;
    let fixture = ConfigFixture::new(config_with_lifecycle);

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), true, &RealFileSystem);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["lifecycles_added"].as_array().unwrap().is_empty(),
        "a type with a lifecycle must not be migrated: {json}"
    );

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    let config = Config::load(fixture.root(), &RealFileSystem).unwrap();
    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.lifecycle.states, vec!["open", "closed"]);
}

// Preserves user-defined extra relationships/rules (dedupe by name, append-only).
#[test]
fn fix_config_preserves_user_defined_extras() {
    let config_with_extras = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "tracks"
inverse = "tracked-by"

[[relationships]]
name = "implements"
inverse = "implemented-by"
"#;
    let fixture = ConfigFixture::new(config_with_extras);

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    let config = Config::parse(&fixture.config_text()).unwrap();
    // The user's custom `tracks` relationship is preserved.
    assert!(config.relationship_by_name("tracks").is_some());
    // `implements` is not duplicated; the 3 remaining standard ones are added.
    assert_eq!(
        config
            .relationships
            .iter()
            .filter(|r| r.name == "implements")
            .count(),
        1,
        "implements must not be duplicated"
    );
    assert!(config.relationship_by_name("supersedes").is_some());
    assert!(config.relationship_by_name("blocks").is_some());
    assert!(config.relationship_by_name("related-to").is_some());
    assert_eq!(config.relationships.len(), 5);
    // All 3 standard rules added.
    assert_eq!(config.rules.len(), 3);
}
