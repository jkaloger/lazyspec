use std::collections::BTreeSet;
use std::path::Path;

use lazyspec::engine::config::{Config, Severity, Traversal};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;
use lazyspec::engine::validation::ValidationIssue;
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

    /// Write a `draft` document carrying the given relations.
    ///
    /// Draft on purpose, and every fixture document alike. The
    /// status-consistency checkers fire on `accepted` documents and read
    /// `store.chain_relationships`, which the migration empties along with the
    /// `traversal` keys it deletes — a separate question from whether a rule
    /// survives translation, and one that would land in the same finding set.
    fn write_doc(&self, path: &str, doc_type: &str, related: &[(&str, &str)]) {
        let links: String = related
            .iter()
            .map(|(rel, target)| format!("- {rel}: {target}\n"))
            .collect();
        let related_block = if links.is_empty() {
            String::new()
        } else {
            format!("related:\n{links}")
        };
        let title = Path::new(path).file_stem().unwrap().to_str().unwrap();
        let body = format!(
            "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: draft\nauthor: \"test\"\n\
             date: 2026-01-01\ntags: []\n{related_block}---\nbody\n"
        );
        std::fs::write(self.root().join(path), body).unwrap();
    }
}

/// What a finding is, for the purpose of comparing a repository's validation
/// state across the migration.
///
/// Not the rendered message: `MissingParentLink` and `MissingRelation` become
/// `UnsatisfiedEdge`, whose `Display` reads differently by construction. What
/// has to survive is which document is in trouble, how loudly, and under which
/// named rule — which is why the translation carries the rule's own `name` onto
/// the edge it becomes. A finding of any other kind is compared whole, since
/// the migration is not supposed to touch one at all.
fn fingerprint(severity: &str, issue: &ValidationIssue) -> String {
    match issue {
        ValidationIssue::MissingParentLink {
            path, rule_name, ..
        }
        | ValidationIssue::MissingRelation {
            path, rule_name, ..
        } => format!("{severity} {} {rule_name}", path.display()),
        ValidationIssue::UnsatisfiedEdge {
            path, edge_name, ..
        } => format!("{severity} {} {edge_name}", path.display()),
        other => format!("{severity} {other}"),
    }
}

/// Every finding `validate` reports for the project at `root`, as fingerprints.
fn findings(root: &Path) -> BTreeSet<String> {
    let config = Config::load(root, &RealFileSystem).expect("the config strict-loads");
    let store = Store::load(root, &config).expect("the store loads");
    let result = store.validate_full(&config);
    result
        .errors
        .iter()
        .map(|issue| fingerprint("error", issue))
        .chain(
            result
                .warnings
                .iter()
                .map(|issue| fingerprint("warning", issue)),
        )
        .collect()
}

/// The legacy project the migration is proved against: a document for every
/// finding the pre-RFC-067 checkers produce, and one for every near miss that
/// must stay silent.
fn legacy_project() -> ConfigFixture {
    let fixture = ConfigFixture::new(LEGACY_DAG_CONFIG);
    fixture.write_doc("docs/rfcs/RFC-001-the-parent.md", "rfc", &[]);
    fixture.write_doc(
        "docs/iterations/ITERATION-001-a-parent-of-the-wrong-type.md",
        "iteration",
        &[],
    );
    // stories-need-rfcs, three ways to be unsatisfied and one to be satisfied.
    fixture.write_doc("docs/stories/STORY-001-no-link-at-all.md", "story", &[]);
    fixture.write_doc(
        "docs/stories/STORY-002-implements-the-rfc.md",
        "story",
        &[("implements", "RFC-001")],
    );
    fixture.write_doc(
        "docs/stories/STORY-003-implements-an-iteration.md",
        "story",
        &[("implements", "ITERATION-001")],
    );
    fixture.write_doc(
        "docs/stories/STORY-004-blocks-the-rfc.md",
        "story",
        &[("blocks", "RFC-001")],
    );
    // adrs-need-relations, unsatisfied and satisfied.
    fixture.write_doc("docs/adrs/ADR-001-no-relations.md", "adr", &[]);
    fixture.write_doc(
        "docs/adrs/ADR-002-related-to-the-rfc.md",
        "adr",
        &[("related-to", "RFC-001")],
    );
    fixture
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

/// A legacy config whose `[[rules]]` blocks carry the two things the
/// translating rewrite deletes without trace: comments on a translated block,
/// and the `require_parent_status` gate ADR-033 retired. `adrs-need-relations`
/// carries neither and is the control — a plan that warns about every block
/// teaches the reader to skip the warning.
const COMMENTED_LEGACY_CONFIG: &str = r#"[[types]]
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

# every story traces to an rfc
[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning" # loud enough to notice
require_parent_status = "accepted"

[[rules]]
name = "adrs-need-relations"
shape = "relation-existence"
type = "adr"
require = "any-relation"
severity = "error"

# the tracker this project files against
[bugtracker]
url = "https://example.invalid"
"#;

// AC5 — the fixture the preservation claim rests on. Set equality between two
// empty sets proves nothing, and equality between two wrong sets proves less,
// so the findings are named before they are compared.
#[test]
fn the_legacy_project_finds_each_unsatisfied_rule_and_nothing_for_the_near_misses() {
    let fixture = legacy_project();

    let before = findings(fixture.root());

    assert_eq!(
        before,
        BTreeSet::from([
            "error docs/adrs/ADR-001-no-relations.md adrs-need-relations".to_string(),
            "warning docs/stories/STORY-001-no-link-at-all.md stories-need-rfcs".to_string(),
            "warning docs/stories/STORY-003-implements-an-iteration.md stories-need-rfcs"
                .to_string(),
            "warning docs/stories/STORY-004-blocks-the-rfc.md stories-need-rfcs".to_string(),
        ])
    );
}

// AC5 — the whole justification for calling the migration safe: one project,
// validated before and after `fix --config` rewrites its rules and traversal
// keys into edges, produces the same finding set.
#[test]
fn the_finding_set_survives_the_migration() {
    let fixture = legacy_project();
    let before = findings(fixture.root());
    assert!(
        !before.is_empty(),
        "a fixture that finds nothing before proves nothing after"
    );

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    assert_eq!(findings(fixture.root()), before);
}

// AC5 — the case ADR-032's original wildcard would have widened: a story whose
// only link to an RFC is `blocks`, which this config does not mark chain. The
// old checker is satisfied by a chain relationship and by nothing else, so the
// document is a finding; naming the chain relationship in `via` is what keeps
// it one.
#[test]
fn a_non_chain_link_to_the_right_parent_type_is_a_finding_on_both_sides() {
    let fixture = legacy_project();
    let non_chain = "warning docs/stories/STORY-004-blocks-the-rfc.md stories-need-rfcs";
    assert!(findings(fixture.root()).contains(non_chain));

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    assert!(
        findings(fixture.root()).contains(non_chain),
        "a relationship the config never marked chain must not start satisfying the rule"
    );
}

/// A config marking two relationships chain, which is what this project's own
/// config does. `stories-need-rfcs` names neither relationship, so today it is
/// satisfied by `targets` as readily as by `implements` — the hole RFC-067
/// §Problem.1 opens with and ADR-032 leaves open, since closing it is a human
/// edit to `via` and not something a mechanical translation may decide.
const TWO_CHAIN_RELATIONSHIPS_CONFIG: &str = r#"[[types]]
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
traversal = "chain"

[[relationships]]
name = "targets"
inverse = "targeted-by"
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
"#;

// AC5 — the hole the migration is required to leave open. A story satisfying
// `stories-need-rfcs` through `targets` rather than `implements` is no finding
// today, and must be no finding after: closing it would be the migration
// deciding what the author meant.
//
// The disjunction is what the set in `via` carries: one row naming both chain
// relationships is satisfied by either, where a row apiece demanded both.
#[test]
fn a_rule_satisfied_through_targets_rather_than_implements_is_no_finding_on_either_side() {
    let fixture = ConfigFixture::new(TWO_CHAIN_RELATIONSHIPS_CONFIG);
    fixture.write_doc("docs/rfcs/RFC-001-the-parent.md", "rfc", &[]);
    fixture.write_doc(
        "docs/stories/STORY-001-targets-the-rfc.md",
        "story",
        &[("targets", "RFC-001")],
    );
    fixture.write_doc(
        "docs/stories/STORY-002-implements-the-rfc.md",
        "story",
        &[("implements", "RFC-001")],
    );
    assert_eq!(findings(fixture.root()), BTreeSet::new());

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    assert_eq!(
        findings(fixture.root()),
        BTreeSet::new(),
        "migration must not close the targets-satisfies-implements hole, nor open a new one \
         against the story that links the way the rule's author meant"
    );
}

// AC7 — the plan names each comment the rewrite destroys and the block it
// belongs to, before anything is written.
#[test]
fn fix_config_dry_run_names_the_comments_the_rewrite_destroys() {
    let fixture = ConfigFixture::new(COMMENTED_LEGACY_CONFIG);
    let original = fixture.config_bytes();

    let output = lazyspec::cli::fix::run_config_human(fixture.root(), true, &RealFileSystem);

    assert!(
        output.contains(
            "Would lose comment on rule stories-need-rfcs: # every story traces to an rfc"
        ),
        "{output}"
    );
    assert!(
        output.contains("Would lose comment on rule stories-need-rfcs: # loud enough to notice"),
        "{output}"
    );
    assert_eq!(
        fixture.config_bytes(),
        original,
        "the plan is shown before anything is applied"
    );
}

// AC7 — the warning is true: exactly the comments named are the ones gone from
// the file afterwards, and a comment on a section the migration does not
// translate is untouched.
#[test]
fn the_comments_the_plan_names_are_the_ones_the_rewrite_removes() {
    let fixture = ConfigFixture::new(COMMENTED_LEGACY_CONFIG);

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    let text = fixture.config_text();
    assert!(!text.contains("# every story traces to an rfc"), "{text}");
    assert!(!text.contains("# loud enough to notice"), "{text}");
    assert!(
        text.contains("# the tracker this project files against"),
        "a comment outside the translated blocks survives: {text}"
    );
}

// AC7 — no warning on a block that loses nothing.
#[test]
fn fix_config_reports_no_lost_comment_for_an_uncommented_rule() {
    let fixture = ConfigFixture::new(COMMENTED_LEGACY_CONFIG);

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), true, &RealFileSystem);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let named: Vec<&str> = parsed["comments_lost"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["rule"].as_str().unwrap())
        .collect();
    assert!(!named.contains(&"adrs-need-relations"), "{json}");
}

// AC7 — a config whose rules carry no comments produces no warning at all.
#[test]
fn fix_config_reports_no_lost_comments_for_a_config_carrying_none() {
    let fixture = ConfigFixture::new(LEGACY_DAG_CONFIG);

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), true, &RealFileSystem);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed["comments_lost"].as_array().unwrap().is_empty(),
        "{json}"
    );
    assert!(
        parsed["gates_dropped"].as_array().unwrap().is_empty(),
        "{json}"
    );
}

// AC7, dictum 2 — every fact in the human plan is a field in the JSON.
#[test]
fn fix_config_json_carries_the_lost_comments_and_dropped_gates() {
    let fixture = ConfigFixture::new(COMMENTED_LEGACY_CONFIG);

    let json = lazyspec::cli::fix::run_config_json(fixture.root(), true, &RealFileSystem);

    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    let lost: Vec<(&str, &str)> = parsed["comments_lost"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["rule"].as_str().unwrap(), c["comment"].as_str().unwrap()))
        .collect();
    assert_eq!(
        lost,
        vec![
            ("stories-need-rfcs", "# every story traces to an rfc"),
            ("stories-need-rfcs", "# loud enough to notice"),
        ],
        "{json}"
    );
    assert_eq!(
        parsed["gates_dropped"].as_array().unwrap(),
        &vec![serde_json::json!("stories-need-rfcs")],
        "{json}"
    );
}

// AC7 — the retired gate gets its own line naming ADR-033. It changes no
// finding, so nothing else in the plan would disclose it.
#[test]
fn fix_config_dry_run_reports_the_dropped_gate_naming_adr_033() {
    let fixture = ConfigFixture::new(COMMENTED_LEGACY_CONFIG);

    let output = lazyspec::cli::fix::run_config_human(fixture.root(), true, &RealFileSystem);

    let line = output
        .lines()
        .find(|l| l.contains("require_parent_status"))
        .unwrap_or_else(|| panic!("no gate line in: {output}"));
    assert!(line.contains("stories-need-rfcs"), "{line}");
    assert!(line.contains("ADR-033"), "{line}");
}

// AC7 — "nothing to add" is false over a rewrite, so the no-op line has to
// speak for the migration too.
#[test]
fn fix_config_no_op_line_speaks_for_the_migration_as_well() {
    let fixture = ConfigFixture::new(COMMENTED_LEGACY_CONFIG);
    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    let output = lazyspec::cli::fix::run_config_human(fixture.root(), true, &RealFileSystem);

    assert!(output.contains("nothing to migrate"), "{output}");
}

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
    assert_eq!(row.via.names(), ["implements"]);
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
    assert_eq!(stories.via.names(), ["implements"]);
    assert_eq!(stories.required, Some(Severity::Warning));

    let iterations = by_name("iterations-need-stories");
    assert_eq!(iterations.from.names(), ["iteration"]);
    assert_eq!(iterations.to.names(), ["story"]);
    assert_eq!(iterations.required, Some(Severity::Error));

    let adrs = by_name("adrs-need-relations");
    assert_eq!(adrs.from.names(), ["adr"]);
    assert!(adrs.to.names().is_empty(), "any target type");
    assert!(adrs.via.names().is_empty(), "any relationship");
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
    assert_eq!(tracked.via.names(), ["tracks"]);
}
