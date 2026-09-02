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
    /// status-consistency checkers fire on `accepted` documents and read the
    /// traversal declaration this migration rewrites — a separate question from
    /// whether a rule survives translation, and one that would land in the same
    /// finding set.
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

/// What a finding is, for the purpose of naming a migrated repository's
/// validation state.
///
/// Not the rendered message: a rule's `MissingParentLink` became an
/// `UnsatisfiedEdge`, whose `Display` reads differently by construction. What
/// had to survive is which document is in trouble, how loudly, and under which
/// named constraint — which is why the translation carries the rule's own
/// `name` onto the edge it becomes. A finding of any other kind is compared
/// whole, since the migration is not supposed to touch one at all.
fn fingerprint(severity: &str, issue: &ValidationIssue) -> String {
    match issue {
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

/// A legacy config carrying everything the translating rewrite deletes without
/// trace: a comment on a translated `[[rules]]` block, a comment on a
/// `traversal` key the rewrite strips off a relationship it otherwise keeps, and
/// the `require_parent_status` gate ADR-033 retired. `adrs-need-relations` and
/// the `blocks` relationship carry none of it and are the control — a plan that
/// warns about every block teaches the reader to skip the warning.
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
traversal = "chain" # the marker this project walks its chain by

[[relationships]]
name = "supersedes"
inverse = "superseded-by"

[[relationships]]
name = "blocks"
inverse = "blocked-by" # a key the rewrite keeps

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

// The finding set the migration has to produce.
//
// ITERATION-379 proved this set equal before and after `fix --config`, reading
// the "before" half off the legacy config's own checkers. STORY-259 deletes
// those checkers and refuses to load the config that fed them, so the "before"
// half is no longer measurable and the proof is spent — it did its work in
// ITERATION-379, on the code that was there to be proved. What survives is the
// half that still has a subject: the migrated config's findings, named
// document by document rather than compared to an unnameable predecessor.
#[test]
fn the_migrated_legacy_project_finds_each_unsatisfied_rule_and_nothing_for_the_near_misses() {
    let fixture = legacy_project();

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    assert_eq!(
        findings(fixture.root()),
        BTreeSet::from([
            "error docs/adrs/ADR-001-no-relations.md adrs-need-relations".to_string(),
            "warning docs/stories/STORY-001-no-link-at-all.md stories-need-rfcs".to_string(),
            "warning docs/stories/STORY-003-implements-an-iteration.md stories-need-rfcs"
                .to_string(),
            "warning docs/stories/STORY-004-blocks-the-rfc.md stories-need-rfcs".to_string(),
        ])
    );
}

// The case ADR-032's original wildcard would have widened: a story whose only
// link to an RFC is `blocks`, which this config does not mark chain. The old
// checker was satisfied by a chain relationship and by nothing else, so the
// document was a finding; naming the chain relationship in `via` is what keeps
// it one.
#[test]
fn a_non_chain_link_to_the_right_parent_type_is_still_a_finding_after_the_migration() {
    let fixture = legacy_project();

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    assert!(
        findings(fixture.root())
            .contains("warning docs/stories/STORY-004-blocks-the-rfc.md stories-need-rfcs"),
        "a relationship the config never marked chain must not satisfy the row, got: {:?}",
        findings(fixture.root())
    );
}

/// A legacy config with a `parent-child` rule and NOT ONE relationship marked
/// `traversal = "chain"`. Every standard relationship is already declared, so
/// the append step adds none and cannot supply the missing marker — a marker is
/// only ever written onto a relationship the file did not already have.
///
/// `validation.rs` satisfies a parent-child rule only through a chain-marked
/// relationship, so this rule is satisfiable by nothing: it fires on every
/// `story` in the repository, whatever that story links to.
const NO_CHAIN_RELATIONSHIP_CONFIG: &str = r#"[[types]]
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
traversal = "related"

[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning"
"#;

fn no_chain_relationship_project() -> ConfigFixture {
    let fixture = ConfigFixture::new(NO_CHAIN_RELATIONSHIP_CONFIG);
    fixture.write_doc("docs/rfcs/RFC-001-the-parent.md", "rfc", &[]);
    fixture.write_doc("docs/stories/STORY-001-no-link-at-all.md", "story", &[]);
    fixture.write_doc(
        "docs/stories/STORY-002-implements-the-rfc.md",
        "story",
        &[("implements", "RFC-001")],
    );
    fixture
}

// The rule no chain relationship can satisfy. It fired on every story before
// the migration, so it must fire on every story after: dropping it would be the
// migration silencing a finding rather than translating it.
#[test]
fn a_rule_no_chain_relationship_can_satisfy_goes_on_firing_on_every_story() {
    let fixture = no_chain_relationship_project();

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    assert_eq!(
        findings(fixture.root()),
        BTreeSet::from([
            "warning docs/stories/STORY-001-no-link-at-all.md stories-need-rfcs".to_string(),
            "warning docs/stories/STORY-002-implements-the-rfc.md stories-need-rfcs".to_string(),
        ]),
        "a row satisfiable by nothing fires on every source document"
    );
}

// AC1/AC2 — the row such a rule translates to: an empty `via`, which is the
// spelling of "no relationship realizes this edge". It has to strict-load.
#[test]
fn a_rule_no_chain_relationship_can_satisfy_becomes_a_row_naming_no_relationship() {
    let fixture = no_chain_relationship_project();

    lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);

    assert!(
        fixture.config_text().contains("via = []"),
        "got: {}",
        fixture.config_text()
    );
    let config = Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
    let row = config
        .edges
        .iter()
        .find(|e| e.name == "stories-need-rfcs")
        .expect("the rule's row survives translation");
    assert!(row.via.names().is_empty());
    assert_eq!(row.required, Some(Severity::Warning));
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

// The hole the migration is required to leave open. A story satisfying
// `stories-need-rfcs` through `targets` rather than `implements` was no finding
// before, and must be no finding after: closing it would be the migration
// deciding what the author meant.
//
// The disjunction is what the set in `via` carries: one row naming both chain
// relationships is satisfied by either, where a row apiece demanded both.
#[test]
fn a_rule_satisfied_through_targets_rather_than_implements_is_no_finding_after_the_migration() {
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

// AC7, ITERATION-378 — the rewrite strips the `traversal` key off a
// relationship it otherwise keeps, so the comment on that key dies too. The
// line names the relationship, because "rule X" would send the reader looking
// for a `[[rules]]` block that never held it.
#[test]
fn fix_config_dry_run_names_the_comment_on_a_traversal_key_it_removes() {
    let fixture = ConfigFixture::new(COMMENTED_LEGACY_CONFIG);

    let output = lazyspec::cli::fix::run_config_human(fixture.root(), true, &RealFileSystem);

    assert!(
        output.contains(
            "Would lose comment on relationship implements: \
             # the marker this project walks its chain by"
        ),
        "{output}"
    );
    assert!(
        !output.contains("a key the rewrite keeps"),
        "a comment on a key that survives must not be reported: {output}"
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
        !text.contains("the marker this project walks its chain by"),
        "the traversal key's comment goes with the key: {text}"
    );
    assert!(
        text.contains("# a key the rewrite keeps"),
        "a comment on a surviving key of a rewritten relationship stays: {text}"
    );
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
        .map(|c| c["name"].as_str().unwrap())
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
    let lost: Vec<(&str, &str, &str)> = parsed["comments_lost"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["block"].as_str().unwrap(),
                c["name"].as_str().unwrap(),
                c["comment"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        lost,
        vec![
            (
                "relationship",
                "implements",
                "# the marker this project walks its chain by"
            ),
            (
                "rule",
                "stories-need-rfcs",
                "# every story traces to an rfc"
            ),
            ("rule", "stories-need-rfcs", "# loud enough to notice"),
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

// AC1 — injects standard blocks. The standard constraints arrive as
// `[[edges]]`, the only shape that loads, so nothing here is "injecting rules".
#[test]
fn fix_config_injects_relationships_and_the_standard_constraints() {
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
    // a row is a constraint's only spelling now, which is what keeps one run
    // enough. The two chain ones carry the role the rules they replace
    // translated to, so the pair-walking findings have concrete endpoints to
    // enumerate.
    let by_name = |name: &str| config.edges.iter().find(|e| e.name == name).unwrap();

    let stories = by_name("stories-need-rfcs");
    assert_eq!(stories.from.names(), ["story"]);
    assert_eq!(stories.to.names(), ["rfc"]);
    assert_eq!(stories.via.names(), ["implements"]);
    assert_eq!(stories.required, Some(Severity::Warning));
    assert_eq!(stories.traversal, Some(Traversal::Chain));

    let iterations = by_name("iterations-need-stories");
    assert_eq!(iterations.from.names(), ["iteration"]);
    assert_eq!(iterations.to.names(), ["story"]);
    assert_eq!(iterations.required, Some(Severity::Error));
    assert_eq!(iterations.traversal, Some(Traversal::Chain));

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
    // The standard constraints are reported as the `[[edges]]` rows they land
    // as, once each. No `[[rules]]` block is written, so no line calls them
    // rules.
    assert!(
        output.contains("Would write edge stories-need-rfcs"),
        "{output}"
    );
    assert!(
        output.contains("Would write edge iterations-need-stories"),
        "{output}"
    );
    assert!(
        output.contains("Would write edge adrs-need-relations"),
        "{output}"
    );
    assert!(
        !output.contains("Would add rule"),
        "no line may name a [[rules]] block this run does not write: {output}"
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

// STORY-259 AC1 end to end: the refusal names `fix --config`, and `fix
// --config` has to be able to read the very config the refusal is about. It
// dispatches ahead of `Config::load` (`main.rs`) and reads leniently
// (ADR-012), so the remedy bypasses the rejection that names it. A config the
// tool rejects and cannot repair would be a dead end.
#[test]
fn a_config_strict_load_refuses_is_still_repairable_by_fix_config() {
    let fixture = ConfigFixture::new(LEGACY_DAG_CONFIG);

    let refusal = Config::load(fixture.root(), &RealFileSystem)
        .expect_err("a config declaring [[rules]] must not strict-load")
        .to_string();
    assert!(refusal.contains("[[rules]]"), "got: {refusal}");
    assert!(refusal.contains("fix --config"), "got: {refusal}");

    // The plan reads the same file and reports the migration, writing nothing.
    let plan = lazyspec::cli::fix::run_config_human(fixture.root(), true, &RealFileSystem);
    assert!(
        plan.contains("Would write edge stories-need-rfcs"),
        "got: {plan}"
    );

    assert_eq!(
        lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem),
        0
    );
    Config::load(fixture.root(), &RealFileSystem).expect("the repaired config strict-loads");
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
        !output.contains("Wrote edge"),
        "second run should write no edges, got: {output}"
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
        !output.contains("Would write edge"),
        "dry-run on migrated config should write no edges, got: {output}"
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
        parsed["edges_written"].as_array().unwrap().is_empty(),
        "{json}"
    );
    assert_eq!(
        fixture.config_bytes(),
        original,
        "a config already on the edge table's terms is not rewritten"
    );
}

// The rewrite must never replace a config that loads with one that does not.
// A hand-written `via = "*"` row carrying `traversal = "chain"` overlaps the
// marker row that the appended `related-to` relationship translates to on all
// three positions, and the loader refuses that pair. The rendered text is
// parsed before it is written, so the file survives and the error says why.
#[test]
fn fix_config_refuses_to_write_a_config_that_would_no_longer_load() {
    let collides = r#"[[types]]
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

[[edges]]
name = "everything-is-chain"
from = "story"
to = "*"
via = "*"
traversal = "chain"
"#;
    let fixture = ConfigFixture::new(collides);
    let original = fixture.config_bytes();

    let error =
        lazyspec::engine::ops::fix::collect_config_fixes(fixture.root(), false, &RealFileSystem)
            .expect_err("a rewrite that would not load is refused");

    let message = error.to_string();
    assert!(
        message.contains("everything-is-chain"),
        "the failure must name the row already in the file, got: {message}"
    );
    assert!(
        message.contains("The rows the migration writes are: related-to-traversal"),
        "the failure must name the row the migration wrote, got: {message}"
    );
    assert_eq!(
        fixture.config_bytes(),
        original,
        "nothing is written when the result would not load"
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
    let tracked = config
        .edges
        .iter()
        .find(|e| e.name == "stories-track-rfcs")
        .expect("the user's rule became a row");
    assert_eq!(tracked.via.names(), ["tracks"]);
}

/// A config that has said nothing whatsoever about its DAG: every standard
/// relationship and type declared, not one `traversal` marker among them, no
/// `[[rules]]` and no `[[edges]]`. Nothing here translates, so the seeded
/// standard set is the config's only declaration of hierarchy — and if it
/// declares none, the migrated project has none.
const UNMARKED_RELATIONSHIPS_CONFIG: &str = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[types]]
name = "iteration"
plural = "iterations"
dir = "docs/iterations"
prefix = "ITERATION"

[[types]]
name = "adr"
plural = "adrs"
dir = "docs/adrs"
prefix = "ADR"

[naming]
pattern = "{type}-{n:03}-{title}.md"

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

// The seeded standard set has to declare a hierarchy, not just three demands
// for a link. A config with no markers to translate gets nothing else, so the
// rows it is seeded carry the chain role a seeded `parent-child` rule used to
// translate to: the blanket row that keeps `implements` hierarchy between any
// pair of types, and the concrete rows whose `from` side names the child types
// `AllChildrenAccepted` and `UpwardOrphanedAcceptance` enumerate.
#[test]
fn seeding_a_config_that_declares_no_dag_declares_a_working_hierarchy() {
    let fixture = ConfigFixture::new(UNMARKED_RELATIONSHIPS_CONFIG);

    let code = lazyspec::cli::fix::run_config(fixture.root(), false, false, &RealFileSystem);
    assert_eq!(code, 0);

    let config = Config::load(fixture.root(), &RealFileSystem).expect("strict load must succeed");
    let by_name = |name: &str| {
        config
            .edges
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("seeded row {name} is missing from {:?}", config.edges))
    };
    let blanket = by_name("implements-traversal");
    assert!(blanket.from.names().is_empty() && blanket.to.names().is_empty());
    assert_eq!(blanket.via.names(), ["implements"]);
    assert_eq!(blanket.traversal, Some(Traversal::Chain));
    assert_eq!(
        by_name("stories-need-rfcs").traversal,
        Some(Traversal::Chain)
    );
    assert_eq!(
        by_name("iterations-need-stories").traversal,
        Some(Traversal::Chain)
    );

    // And the findings that declaration exists to produce.
    fixture.write_doc("docs/rfcs/RFC-001-one.md", "rfc", &[]);
    std::fs::write(
        fixture.root().join("docs/stories/STORY-001-a.md"),
        "---\ntitle: \"A\"\ntype: story\nstatus: accepted\nauthor: \"test\"\n\
         date: 2026-01-01\ntags: []\nrelated:\n- implements: RFC-001\n---\nbody\n",
    )
    .unwrap();

    let store = Store::load(fixture.root(), &config).unwrap();
    let warnings = store.validate_full(&config).warnings;
    assert!(
        warnings
            .iter()
            .any(|issue| matches!(issue, ValidationIssue::AllChildrenAccepted { .. })),
        "the story is `rfc`'s child type, so its acceptance is the RFC's finding: {warnings:?}"
    );
}
