//! End-to-end coverage for `[[edges]]` (STORY-256, ADR-031). The engine's own
//! tests build `Config` structs by hand, so they never exercise `Config::parse`
//! -- and resolution leans on a guarantee only the parser makes, that two
//! overlapping rows of equal specificity stating different severities never
//! reach a walk. These tests go through a `.lazyspec.toml` on disk, so the
//! guarantee and the finding it protects are proved against the same path a
//! user takes.

use lazyspec::engine::config::Config;
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;

const PREAMBLE: &str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "R"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
icon = "Y"

[[types]]
name = "iteration"
plural = "iterations"
dir = "docs/iterations"
prefix = "ITERATION"
icon = "I"

[[relationships]]
name = "implements"
inverse = "implemented-by"
traversal = "chain"

[[relationships]]
name = "related-to"
inverse = "related-to"
traversal = "related"
"#;

/// A fixture whose `.lazyspec.toml` is `PREAMBLE` plus `edges`, loaded strictly
/// off disk rather than constructed. Returns the parse result so a test can
/// assert on rejection as readily as on success.
fn load_with_edges(fixture: &crate::common::TestFixture, edges: &str) -> anyhow::Result<Config> {
    fixture.write_doc(".lazyspec.toml", &format!("{PREAMBLE}{edges}"));
    Config::load(fixture.root(), &RealFileSystem)
}

fn validate_json(fixture: &crate::common::TestFixture, config: &Config) -> serde_json::Value {
    let store = Store::load(fixture.root(), config).expect("store loads");
    let output = lazyspec::cli::validate::run_json(&store, config, &[]);
    serde_json::from_str(&output).expect("validate --json emits JSON")
}

fn strings(value: &serde_json::Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .unwrap_or_else(|| panic!("`{key}` is an array in {value}"))
        .iter()
        .map(|v| v.as_str().expect("finding is a string").to_string())
        .collect()
}

// ADR-031: requiredness comes from the most specific matching row that states
// it, so two rows that can cover one concrete edge at equal specificity and
// state different severities have no resolution. Rejecting them at load is what
// lets resolution assume a tie never arrives -- and only `Config::parse`
// enforces it, so only a load proves it.
#[test]
fn equally_specific_rows_that_state_different_severities_fail_config_load() {
    let fixture = crate::common::TestFixture::new();

    let err = load_with_edges(
        &fixture,
        r#"
[[edges]]
name = "iterations-need-something"
from = ["iteration"]
to = "*"
via = "*"
required = "error"

[[edges]]
name = "work-wants-something"
from = ["iteration", "story"]
to = "*"
via = "*"
required = "warning"
"#,
    )
    .expect_err("a requiredness tie must fail load");

    let err = err.to_string();
    for expected in [
        "iterations-need-something",
        "work-wants-something",
        "required = \"error\"",
        "required = \"warning\"",
    ] {
        assert!(
            err.contains(expected),
            "the load error must name {expected}, got: {err}"
        );
    }
}

// RFC-067 §Design's starter shape, which the old rule could not load: a
// wildcard `related-to` row and a `relation-existence`-shaped demand tie at one
// concrete position and differ only in that one of them is silent. Absence is
// not a disagreement, so the pair loads -- and the narrow documentation row
// does not displace the demand, so an iteration with no relations is a finding.
#[test]
fn a_documentation_only_row_neither_blocks_load_nor_silences_a_demand() {
    let fixture = crate::common::TestFixture::new();
    let config = load_with_edges(
        &fixture,
        r#"
[[edges]]
name = "general-relatedness"
from = "*"
to = "*"
via = "related-to"

[[edges]]
name = "iterations-may-implement-stories"
from = ["iteration"]
to = ["story"]
via = "implements"

[[edges]]
name = "iterations-need-relations"
from = ["iteration"]
to = "*"
via = "*"
required = "error"
"#,
    )
    .expect("a demand and a silent row of equal specificity load together");

    fixture.write_iteration("ITERATION-001.md", "Unlinked", "draft", None);

    let errors = strings(&validate_json(&fixture, &config), "errors");
    let edge_findings: Vec<&String> = errors
        .iter()
        .filter(|e| e.contains("unsatisfied edge"))
        .collect();

    assert_eq!(
        edge_findings.len(),
        1,
        "the demand survives both silent rows, got: {errors:?}"
    );
    assert!(
        edge_findings[0].contains("[iterations-need-relations]"),
        "the surviving finding is the demand, got: {}",
        edge_findings[0]
    );
}

// The same config that loads must also report through `validate --json`. The
// finding names the document's own type -- `iteration` -- and not the whole of
// the row's `from` set, which a document can only ever be one member of.
#[test]
fn validate_json_reports_an_unsatisfied_edge_naming_the_documents_own_type() {
    let fixture = crate::common::TestFixture::new();
    let config = load_with_edges(
        &fixture,
        r#"
[[edges]]
name = "work-implements-rfcs"
from = ["iteration", "story"]
to = ["rfc"]
via = "implements"
required = "error"
"#,
    )
    .expect("a config with edges loads");

    fixture.write_rfc("RFC-001.md", "An RFC", "draft");
    fixture.write_iteration("ITERATION-001.md", "Unlinked", "draft", None);
    fixture.write_story(
        "STORY-001.md",
        "Linked",
        "draft",
        Some("docs/rfcs/RFC-001.md"),
    );

    let parsed = validate_json(&fixture, &config);
    let errors = strings(&parsed, "errors");

    let edge_findings: Vec<&String> = errors
        .iter()
        .filter(|e| e.contains("unsatisfied edge"))
        .collect();
    assert_eq!(
        edge_findings.len(),
        1,
        "only the unlinked iteration is unsatisfied, got: {errors:?}"
    );
    assert!(
        edge_findings[0].contains("(iteration needs \"implements\" to one of: rfc)"),
        "the finding names the document's own type, got: {}",
        edge_findings[0]
    );
    assert!(
        edge_findings[0].contains("ITERATION-001.md"),
        "the finding names the offending document, got: {}",
        edge_findings[0]
    );
}

// A wildcard row renders as prose rather than echoing the config spelling, and
// severity `warning` lands in `warnings`, not `errors`.
#[test]
fn validate_json_renders_a_wildcard_row_as_prose_at_its_declared_severity() {
    let fixture = crate::common::TestFixture::new();
    let config = load_with_edges(
        &fixture,
        r#"
[[edges]]
name = "iterations-need-relations"
from = ["iteration"]
to = "*"
via = "*"
required = "warning"
"#,
    )
    .expect("a config with wildcard endpoints loads");

    fixture.write_iteration("ITERATION-001.md", "Unlinked", "draft", None);

    let parsed = validate_json(&fixture, &config);

    assert!(
        strings(&parsed, "warnings").iter().any(|w| {
            w.contains("unsatisfied edge [iterations-need-relations]")
                && w.contains("(iteration needs any relationship to a document of any type)")
        }),
        "got warnings {:?}",
        strings(&parsed, "warnings")
    );
    assert!(
        !strings(&parsed, "errors")
            .iter()
            .any(|e| e.contains("unsatisfied edge")),
        "a `warning` row must not raise an error, got: {:?}",
        strings(&parsed, "errors")
    );
}

// The wildcard has one spelling. A list is read as type names, so `["*"]` must
// say how to write a wildcard rather than sending the reader to `[[types]]`.
#[test]
fn a_wildcard_written_inside_a_list_fails_load_with_the_bare_string_spelling() {
    let fixture = crate::common::TestFixture::new();

    let err = load_with_edges(
        &fixture,
        r#"
[[edges]]
name = "listed-wildcard"
from = ["iteration"]
to = ["*"]
via = "implements"
"#,
    )
    .expect_err("a listed wildcard must fail load")
    .to_string();

    assert!(
        err.contains("listed-wildcard") && err.contains("to = \"*\""),
        "the error names the edge and the bare-string spelling, got: {err}"
    );
    assert!(
        !err.contains("not declared in [[types]]"),
        "the error must not send the reader to [[types]], got: {err}"
    );
}

/// The row `a_refused_edge_mutation_under_json_...` asks for, written out so the
/// message it expects comes from the loader rather than from the writer.
const UNDECLARED_TARGET_ROW: &str = r#"
[[edges]]
name = "x"
from = "story"
to = "nonsense"
via = "implements"
"#;

// STORY-261 AC5 under `--json`. No command in the binary has a JSON error
// envelope, so a refused mutation reports failure the way every other `--json`
// command does: non-zero exit, the loader's message on stderr, and nothing at
// all on stdout. That shape is the contract an agent reads -- empty stdout with
// a non-zero status is the refusal -- so it is pinned here rather than left to
// whichever error happens to be raised.
#[test]
fn a_refused_edge_mutation_under_json_leaves_stdout_empty_and_the_file_alone() {
    let fixture = crate::common::TestFixture::new();
    let path = fixture.write_doc(".lazyspec.toml", PREAMBLE);
    let before = std::fs::read_to_string(&path).unwrap();

    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lazyspec"))
        .args([
            "config",
            "add-edge",
            "x",
            "--from",
            "story",
            "--to",
            "nonsense",
            "--via",
            "implements",
            "--json",
        ])
        .current_dir(fixture.root())
        .output()
        .expect("the lazyspec binary runs");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "a refused mutation exits non-zero, stderr: {stderr}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "",
        "the success envelope is never printed, so stdout stays empty"
    );

    let expected = Config::parse(&format!("{PREAMBLE}{UNDECLARED_TARGET_ROW}"))
        .expect_err("a row naming an undeclared type does not load")
        .to_string();
    assert!(
        stderr.contains(&expected),
        "the loader's own message reaches stderr, got: {stderr}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        before,
        "the config is left as it was"
    );
}
