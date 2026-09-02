use lazyspec::engine::config::{
    validate_status, Authorship, Config, NumberingStrategy, ReservedFormat, Severity, TypeDef,
    ValidationRule,
};
use lazyspec::engine::document::Status;
use lazyspec::engine::fs::RealFileSystem;
use tempfile::TempDir;

/// Strict load now requires a `[[relationships]]` block; tests that build a
/// `[[types]]`-only config append this so they exercise the section under test.
const RELATIONSHIPS: &str = r#"
[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "related-to"
"#;

#[test]
fn parse_config_from_toml() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.type_by_name("rfc").unwrap().dir, "docs/rfcs");
    assert_eq!(config.documents.naming.pattern, "{type}-{n:03}-{title}.md");
}

#[test]
fn parse_types_from_toml() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "specs/rfcs"
prefix = "RFC"
icon = "●"

[[types]]
name = "epic"
plural = "epics"
dir = "docs/epics"
prefix = "EPIC"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.documents.types.len(), 2);

    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.dir, "specs/rfcs");

    let epic = config.type_by_name("epic").unwrap();
    assert_eq!(epic.plural, "epics");
    assert_eq!(epic.prefix, "EPIC");
    assert_eq!(epic.icon, None);
}

#[test]
fn type_by_name_returns_none_for_unknown() {
    let config = Config::default();
    assert!(config.type_by_name("nonexistent").is_none());
}

#[test]
fn parse_types_missing_required_field_returns_error() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
"#;

    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("prefix"),
        "Error should mention missing field 'prefix', got: {err_msg}"
    );
}

#[test]
fn no_rules_section_yields_empty_rules() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert!(config.rules.is_empty());
}

// The three tests below read through `Config::parse_lenient`. Strict load
// refuses a config that declares `[[rules]]` at all (STORY-259), so the lenient
// read is the only remaining reader of the shape — and it has to stay one, or
// `fix --config`, the remedy the refusal names, could not translate what it
// finds. `declared_rules_are_the_only_rules` went with the strict path: under
// it, a declared rule is a rejection rather than a rule, which
// `strict_load_refuses_a_config_declaring_rules_and_names_fix_config` asserts.

#[test]
fn the_lenient_read_parses_a_parent_child_rule() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[rules]]
shape = "parent-child"
name = "epics-need-themes"
child = "epic"
parent = "theme"
severity = "warning"
"#;

    let config = Config::parse_lenient(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.rules.len(), 1);
    assert_eq!(
        config.rules[0],
        ValidationRule::ParentChild {
            name: "epics-need-themes".to_string(),
            child: "epic".to_string(),
            parent: "theme".to_string(),
            severity: Severity::Warning,
        }
    );
}

#[test]
fn the_lenient_read_parses_a_relation_existence_rule() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[rules]]
shape = "relation-existence"
name = "rfcs-need-relations"
type = "rfc"
require = "any-relation"
severity = "error"
"#;

    let config = Config::parse_lenient(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.rules.len(), 1);
    assert_eq!(
        config.rules[0],
        ValidationRule::RelationExistence {
            name: "rfcs-need-relations".to_string(),
            doc_type: "rfc".to_string(),
            require: "any-relation".to_string(),
            severity: Severity::Error,
        }
    );
}

// Deserialization rejects the value before any of the load path's own
// diagnostics run, which is why the lenient read still fails here.
#[test]
fn invalid_severity_returns_parse_error() {
    let toml_str = r#"
[[rules]]
shape = "parent-child"
name = "bad-rule"
child = "iteration"
parent = "story"
severity = "fatal"
"#;

    let result = Config::parse_lenient(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(
        result.is_err(),
        "Expected parse error for invalid severity 'fatal'"
    );
}

#[test]
fn parse_tui_ascii_diagrams_true() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[tui]
ascii_diagrams = true
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert!(config.ui.ascii_diagrams);
}

#[test]
fn tui_defaults_to_ascii_diagrams_false() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert!(!config.ui.ascii_diagrams);
}

#[test]
fn default_config_has_ascii_diagrams_false() {
    let config = Config::default();
    assert!(!config.ui.ascii_diagrams);
}

// --- Numbering / Sqids config tests ---

#[test]
fn absent_numbering_defaults_to_incremental() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    for t in &config.documents.types {
        assert_eq!(t.numbering, NumberingStrategy::Incremental);
    }
}

#[test]
fn valid_sqids_config_parses() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = "my-secret-salt"
min_length = 5
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.numbering, NumberingStrategy::Sqids);
    let sqids_cfg = config.documents.sqids.unwrap();
    assert_eq!(sqids_cfg.salt, "my-secret-salt");
    assert_eq!(sqids_cfg.min_length, 5);
}

#[test]
fn sqids_config_defaults_min_length_to_3() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = "my-salt"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let sqids_cfg = config.documents.sqids.unwrap();
    assert_eq!(sqids_cfg.min_length, 3);
}

#[test]
fn sqids_missing_salt_fails() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"
"#;
    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("salt"),
        "Error should mention salt, got: {msg}"
    );
}

#[test]
fn sqids_empty_salt_fails() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = ""
"#;
    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("salt"),
        "Error should mention salt, got: {msg}"
    );
}

#[test]
fn sqids_min_length_zero_fails() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = "my-salt"
min_length = 0
"#;
    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("min_length"),
        "Error should mention min_length, got: {msg}"
    );
}

#[test]
fn sqids_min_length_eleven_fails() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "sqids"

[numbering.sqids]
salt = "my-salt"
min_length = 11
"#;
    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("min_length"),
        "Error should mention min_length, got: {msg}"
    );
}

// --- Numbering / Reserved config tests ---

#[test]
fn valid_reserved_config_parses() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
remote = "upstream"
format = "incremental"
max_retries = 3
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.numbering, NumberingStrategy::Reserved);
    let reserved_cfg = config.documents.reserved.unwrap();
    assert_eq!(reserved_cfg.remote, "upstream");
    assert_eq!(reserved_cfg.format, ReservedFormat::Incremental);
    assert_eq!(reserved_cfg.max_retries, 3);
}

#[test]
fn reserved_config_defaults() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
format = "incremental"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let reserved_cfg = config.documents.reserved.unwrap();
    assert_eq!(reserved_cfg.remote, "origin");
    assert_eq!(reserved_cfg.max_retries, 5);
}

#[test]
fn reserved_sqids_format_requires_sqids_config() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
format = "sqids"
"#;
    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("sqids"),
        "Error should mention sqids, got: {msg}"
    );
}

#[test]
fn reserved_incremental_format_no_sqids_needed() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
format = "incremental"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(config.is_ok());
}

#[test]
fn reserved_empty_remote_fails() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
remote = ""
format = "incremental"
"#;
    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("remote"),
        "Error should mention remote, got: {msg}"
    );
}

#[test]
fn ref_count_ceiling_defaults_to_15() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.ref_count_ceiling, 15);
}

#[test]
fn ref_count_ceiling_configurable() {
    let toml_str = r#"
ref_count_ceiling = 20

[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.ref_count_ceiling, 20);
}

#[test]
fn default_config_has_ref_count_ceiling_15() {
    let config = Config::default();
    assert_eq!(config.ref_count_ceiling, 15);
}

#[test]
fn reserved_missing_section_fails() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
numbering = "reserved"
"#;
    let result = Config::parse(&format!("{toml_str}{RELATIONSHIPS}"));
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("reserved"),
        "Error should mention reserved, got: {msg}"
    );
}

#[test]
fn singleton_field_defaults_to_false() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let rfc = config.type_by_name("rfc").unwrap();
    assert!(!rfc.singleton);
}

#[test]
fn singleton_field_parses_true() {
    let toml_str = r#"
[[types]]
name = "convention"
plural = "conventions"
dir = "docs/conventions"
prefix = "CONV"
singleton = true
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let conv = config.type_by_name("convention").unwrap();
    assert!(conv.singleton);
}

#[test]
fn parent_type_defaults_to_none() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let rfc = config.type_by_name("rfc").unwrap();
    assert!(rfc.parent_type.is_none());
}

#[test]
fn parent_type_parses_value() {
    let toml_str = r#"
[[types]]
name = "dictum"
plural = "dicta"
dir = "docs/conventions/dicta"
prefix = "DICTUM"
parent_type = "convention"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let dictum = config.type_by_name("dictum").unwrap();
    assert_eq!(dictum.parent_type, Some("convention".to_string()));
}

// --- Strict config-driven type loading (STORY-125) ---

// AC1: no .lazyspec.toml -> load errors pointing at `init`.
#[test]
fn load_without_config_file_errors_pointing_to_init() {
    let tmp = TempDir::new().unwrap();
    let fs = RealFileSystem;

    let err = Config::load(tmp.path(), &fs).unwrap_err();
    let msg = err.to_string();

    assert!(
        msg.contains("init"),
        "error should point at init, got: {msg}"
    );
    assert!(
        msg.contains(".lazyspec.toml"),
        "error should name .lazyspec.toml, got: {msg}"
    );
}

// AC2: file exists but no [[types]] -> hard error naming the missing types.
#[test]
fn parse_without_types_errors() {
    let toml_str = r#"
[naming]
pattern = "{type}-{n:03}-{title}.md"

[templates]
dir = ".lazyspec/templates"
"#;

    let err = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap_err();
    let msg = err.to_string();

    assert!(msg.contains("types"), "error should name types, got: {msg}");
    assert!(
        msg.contains("init"),
        "error should suggest init, got: {msg}"
    );
}

// AC4: directories derive entirely from declared types' dir, not named fields.
#[test]
fn directories_derive_from_declared_type_dirs() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "specs/rfcs"
prefix = "RFC"

[[types]]
name = "epic"
plural = "epics"
dir = "planning/epics"
prefix = "EPIC"
"#;

    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.type_by_name("rfc").unwrap().dir, "specs/rfcs");
    assert_eq!(config.type_by_name("epic").unwrap().dir, "planning/epics");
}

// AC5: omitting a previously-built-in type leaves it absent; never injected.
#[test]
fn omitted_builtin_type_is_absent() {
    let toml_str = r#"
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
"#;

    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert!(config.type_by_name("spec").is_none());
    assert_eq!(config.documents.types.len(), 2);
}

// AC6: explicit plural taken verbatim, no engine-side pluralization.
#[test]
fn explicit_plural_taken_verbatim() {
    let toml_str = r#"
[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"

[[types]]
name = "quux"
plural = "quuxen"
dir = "docs/quuxen"
prefix = "QUUX"
"#;

    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    assert_eq!(config.type_by_name("story").unwrap().plural, "stories");
    assert_eq!(config.type_by_name("quux").unwrap().plural, "quuxen");
}

// AC6: plural is a required field; omitting it errors at parse.
#[test]
fn missing_plural_field_errors() {
    let toml_str = r#"
[[types]]
name = "story"
dir = "docs/stories"
prefix = "STORY"
"#;

    let err = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("plural"),
        "error should name the missing plural field, got: {msg}"
    );
}

// AC1: intent / authorship / lifecycle all parse together and are readable,
// including a `*` edge source carried verbatim.
#[test]
fn type_with_all_three_axes_parses() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
intent = "why this type exists"
authorship = "human"

[types.lifecycle]
states = ["draft", "review", "superseded"]

[[types.lifecycle.edges]]
from = "draft"
to = "review"

[[types.lifecycle.edges]]
from = "*"
to = "superseded"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.intent.as_deref(), Some("why this type exists"));
    assert_eq!(rfc.authorship, Authorship::Human);
    assert_eq!(rfc.lifecycle.states, vec!["draft", "review", "superseded"]);
    assert_eq!(rfc.lifecycle.edges.len(), 2);
    assert_eq!(rfc.lifecycle.edges[0].from, "draft");
    assert_eq!(rfc.lifecycle.edges[0].to, "review");
    assert_eq!(rfc.lifecycle.edges[1].from, "*");
    assert_eq!(rfc.lifecycle.edges[1].to, "superseded");
}

// AC1: the starter default lifecycle survives a to_toml -> parse round-trip.
#[test]
fn to_toml_round_trips_lifecycle() {
    let config = Config::default();
    let toml = config.to_toml().unwrap();
    let reparsed = Config::parse(&toml).unwrap();

    let original = config.type_by_name("rfc").unwrap();
    let round_tripped = reparsed.type_by_name("rfc").unwrap();
    assert_eq!(round_tripped.lifecycle.states, original.lifecycle.states);
    assert_eq!(round_tripped.lifecycle.edges, original.lifecycle.edges);
}

// AC2: an absent `authorship` key resolves to Assisted.
#[test]
fn authorship_defaults_to_assisted_when_absent() {
    let toml_str = r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
"#;
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.authorship, Authorship::Assisted);
}

// AC2: each authorship variant parses via rename_all = "lowercase".
#[test]
fn authorship_parses_each_variant() {
    for (literal, expected) in [
        ("human", Authorship::Human),
        ("assisted", Authorship::Assisted),
        ("generated", Authorship::Generated),
    ] {
        let toml_str = format!(
            r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
authorship = "{literal}"
"#
        );
        let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
        assert_eq!(config.type_by_name("rfc").unwrap().authorship, expected);
    }
}

fn type_def_with_states(states: &[&str]) -> TypeDef {
    let toml_str = format!(
        r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[types.lifecycle]
states = [{}]
"#,
        states
            .iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let config = Config::parse(&format!("{toml_str}{RELATIONSHIPS}")).unwrap();
    config.type_by_name("rfc").unwrap().clone()
}

// AC3: a status naming one of the type's lifecycle states is accepted.
#[test]
fn status_in_lifecycle_states_is_accepted() {
    let type_def = type_def_with_states(&["draft", "review", "accepted"]);
    assert!(validate_status(&type_def, &Status::new("review")).is_ok());
}

// AC3: a status outside the type's lifecycle states is rejected, naming both
// the offending status and the type.
#[test]
fn status_outside_lifecycle_states_is_rejected() {
    let type_def = type_def_with_states(&["draft", "review", "accepted"]);
    let err = validate_status(&type_def, &Status::new("frozen")).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("frozen"),
        "error should name the status: {msg}"
    );
    assert!(msg.contains("rfc"), "error should name the type: {msg}");
}

// RFC-061 (STORY-208 AC3): the lease subsystem is gone, but configs written
// before its removal may still carry a `[coordination]` block. It must parse
// as an ignored unknown table, not an error.
#[test]
fn stray_coordination_block_still_parses() {
    let toml_str = format!(
        "{}{}",
        r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[coordination]
remote = "upstream"
lease_duration = "30m"
grace_period = "5m"
max_push_retries = 10
max_clock_skew = "5m"
"#,
        RELATIONSHIPS
    );
    let config = Config::parse(&toml_str).unwrap();
    assert_eq!(config.documents.types.len(), 1);
}
