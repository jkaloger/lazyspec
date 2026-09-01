use lazyspec::engine::config::{Config, Severity, Traversal};
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

/// A config on the pre-RFC-067 shape: `[[rules]]` blocks and `traversal`
/// markers, with every standard relationship and type already declared so the
/// only repair left is the edge migration itself.
const LEGACY_DAG_CONFIG: &str = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
lifecycle = { states = ["draft", "accepted"], edges = [{ from = "draft", to = "accepted" }] }

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
lifecycle = { states = ["draft", "accepted"], edges = [{ from = "draft", to = "accepted" }] }

[[types]]
name = "iteration"
plural = "iterations"
dir = "docs/iterations"
prefix = "ITERATION"
lifecycle = { states = ["draft", "accepted"], edges = [{ from = "draft", to = "accepted" }] }

[[types]]
name = "adr"
plural = "adrs"
dir = "docs/adrs"
prefix = "ADR"
lifecycle = { states = ["draft", "accepted"], edges = [{ from = "draft", to = "accepted" }] }

[[relationships]]
name = "implements"
inverse = "implemented-by"
traversal = "chain"

[[relationships]]
name = "supersedes"
inverse = "superseded-by"

[[relationships]]
name = "blocks"
inverse = "blocked-by"

[[relationships]]
name = "related-to"
traversal = "related"

[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning"

[[rules]]
name = "adrs-need-relations"
shape = "relation-existence"
type = "adr"
require = "any-relation"
severity = "error"
"#;

// AC1 — the source declarations are translated and then deleted.
#[test]
fn fix_config_translates_rules_and_traversal_into_edges() {
    let fixture = ConfigFixture::new(LEGACY_DAG_CONFIG);

    let code = lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    assert_eq!(code, 0);

    let text = fixture.config_text();
    assert!(text.contains("[[edges]]"), "got: {text}");
    assert!(!text.contains("rules"), "the rules key is gone: {text}");
    assert!(
        !text.contains("traversal = \"chain\"\n\n[[relationships]]"),
        "no relationship keeps a marker: {text}"
    );

    // Strict load is the real gate: the translated rows have to name declared
    // types and relationships, and must not read as a traversal contradiction.
    let config = Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
    assert!(config.rules.is_empty());
    assert!(config.relationships.iter().all(|r| r.traversal.is_none()));

    let names: Vec<&str> = config.edges.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "stories-need-rfcs",
            "adrs-need-relations",
            "implements-traversal",
            "related-to-traversal",
        ]
    );
}

// AC2 as amended — the translated parent-child row names the chain
// relationship, because that is the only relationship the rule accepted.
#[test]
fn fix_config_names_the_chain_relationship_on_a_translated_parent_child_row() {
    let fixture = ConfigFixture::new(LEGACY_DAG_CONFIG);

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    let config = Config::load(fixture.root(), &RealFileSystem).unwrap();
    let row = config
        .edges
        .iter()
        .find(|e| e.name == "stories-need-rfcs")
        .expect("the rule's row");
    assert_eq!(row.via.name(), Some("implements"));
    assert_eq!(row.traversal, Some(Traversal::Chain));
    assert_eq!(row.required, Some(Severity::Warning));
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

    // The three standard constraints arrive as `[[edges]]`, not as `[[rules]]`:
    // seeding them through the translation is what keeps one run enough.
    assert!(config.rules.is_empty(), "no rules are written: {text}");
    let by_name = |name: &str| config.edges.iter().find(|e| e.name == name).unwrap();

    let stories = by_name("stories-need-rfcs");
    assert_eq!(stories.from.names(), ["story"]);
    assert_eq!(stories.to.names(), ["rfc"]);
    assert_eq!(stories.via.name(), Some("implements"));
    assert_eq!(stories.required, Some(Severity::Warning));

    let iterations = by_name("iterations-need-stories");
    assert_eq!(iterations.from.names(), ["iteration"]);
    assert_eq!(iterations.to.names(), ["story"]);
    assert_eq!(iterations.required, Some(Severity::Error));

    let adrs = by_name("adrs-need-relations");
    assert_eq!(adrs.from.names(), ["adr"]);
    assert!(adrs.to.names().is_empty(), "any target type");
    assert_eq!(adrs.via.name(), None, "any relationship");
    assert_eq!(adrs.required, Some(Severity::Error));

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
    assert!(config.rules.is_empty());
    assert_eq!(config.edges.len(), 5, "3 constraints + 2 traversal rows");
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
    assert!(!config.edges.is_empty());
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

// AC6/AC8 — comments and unrelated sections survive the migration. Asserted on
// the file text: a reparse cannot tell you a comment was dropped, and an
// unrecognised section never reaches `Config` at all.
#[test]
fn fix_config_migration_preserves_user_content() {
    let config_with_comment = r#"# my project config
[github]
repo = "owner/repo"

[tui]
ascii_diagrams = true

# a section this tool has never heard of
[deployment]
target = "fly.io"
regions = ["syd", "iad"]

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
    assert!(text.contains("# my project config"), "got: {text}");
    assert!(text.contains("# the rfc type"), "got: {text}");
    assert!(text.contains("[github]"), "got: {text}");
    assert!(text.contains("repo = \"owner/repo\""), "got: {text}");
    assert!(text.contains("[tui]"), "got: {text}");
    assert!(text.contains("ascii_diagrams = true"), "got: {text}");
    // The migration understands none of this and must not touch any of it.
    assert!(
        text.contains("# a section this tool has never heard of"),
        "got: {text}"
    );
    assert!(text.contains("[deployment]"), "got: {text}");
    assert!(text.contains("target = \"fly.io\""), "got: {text}");
    assert!(text.contains(r#"regions = ["syd", "iad"]"#), "got: {text}");
    Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
}

// AC6 — a second run over a migrated config changes nothing and writes nothing.
#[test]
fn fix_config_migration_is_idempotent() {
    let fixture = ConfigFixture::new(LEGACY_DAG_CONFIG);
    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    let after_migration = fixture.config_bytes();

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), false, &RealFileSystem);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(!parsed["written"].as_bool().unwrap(), "{json}");
    assert!(
        parsed["edges_written"].as_array().unwrap().is_empty(),
        "{json}"
    );
    assert!(
        parsed["rules_removed"].as_array().unwrap().is_empty(),
        "{json}"
    );
    assert!(
        parsed["traversal_removed"].as_array().unwrap().is_empty(),
        "{json}"
    );
    assert_eq!(fixture.config_bytes(), after_migration);
}

// AC6 — the config the AC actually names: one carrying `[[edges]]` and no
// `[[rules]]` that this tool has never written. It must be left alone.
#[test]
fn fix_config_leaves_a_hand_written_edge_table_untouched() {
    let hand_written = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
lifecycle = { states = ["draft", "accepted"], edges = [{ from = "draft", to = "accepted" }] }

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
lifecycle = { states = ["draft", "accepted"], edges = [{ from = "draft", to = "accepted" }] }

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

# hand-written, and narrower than any migration would emit
[[edges]]
name = "stories-implement-rfcs"
from = "story"
to = ["rfc"]
via = "implements"
required = "warning"
traversal = "chain"
"#;
    let fixture = ConfigFixture::new(hand_written);
    let original = fixture.config_bytes();

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), false, &RealFileSystem);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert!(!parsed["written"].as_bool().unwrap(), "{json}");
    assert!(
        parsed["rules_added"].as_array().unwrap().is_empty(),
        "{json}"
    );
    assert_eq!(
        fixture.config_bytes(),
        original,
        "a config already on the edge table's terms is not rewritten"
    );
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

// Preserves user-defined extra relationships and the user's own rule, and keeps
// the sections and comments around them (AC8).
#[test]
fn fix_config_preserves_user_defined_extras() {
    let config_with_extras = r#"# the extras this project added itself
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[relationships]]
name = "tracks"
inverse = "tracked-by"
traversal = "chain"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[rules]]
name = "stories-track-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "error"

[bugtracker]
url = "https://example.invalid"
"#;
    let fixture = ConfigFixture::new(config_with_extras);

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    let text = fixture.config_text();
    // The comment and the unrecognised section are none of the migration's business.
    assert!(
        text.contains("# the extras this project added itself"),
        "got: {text}"
    );
    assert!(text.contains("[bugtracker]"), "got: {text}");
    assert!(
        text.contains("url = \"https://example.invalid\""),
        "got: {text}"
    );

    let config = Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
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

    // The user's own rule is translated like any other, through the one
    // relationship the config marks chain.
    assert!(config.rules.is_empty());
    let tracked = config
        .edges
        .iter()
        .find(|e| e.name == "stories-track-rfcs")
        .expect("the user's rule became a row");
    assert_eq!(tracked.via.name(), Some("tracks"));
}
