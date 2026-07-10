//! Proves `config_schema()` emits a real, usable JSON Schema: one a standards
//! validator accepts as well-formed, that admits a genuine `.lazyspec.toml`, and
//! that rejects content violating the derived constraints. This is the guarantee
//! that the schema stays faithful to what `Config::parse_inner` actually accepts,
//! rather than merely being JSON that happens to have a `properties` key.

use jsonschema::validator_for;
use lazyspec::engine::config::config_schema;
use serde_json::Value;

fn schema_json() -> Value {
    serde_json::to_value(config_schema()).expect("schema serialises to JSON")
}

/// Parse a TOML document into the JSON value that a JSON Schema validator sees.
/// This is the same value space `.lazyspec.toml` occupies once deserialised, so
/// validating it against the schema is a faithful check of the config grammar.
fn toml_to_json(toml_src: &str) -> Value {
    toml::from_str::<Value>(toml_src).expect("TOML parses into a JSON value")
}

// (a) The generated schema is itself a well-formed JSON Schema document: a real
// validator can compile it (resolving every internal `$ref`/`$defs` reference)
// without error.
#[test]
fn generated_schema_is_valid_json_schema() {
    let schema = schema_json();
    let compiled = validator_for(&schema);
    assert!(
        compiled.is_ok(),
        "config_schema() must be a compilable JSON Schema, got: {:?}",
        compiled.err()
    );
}

// (b) The load-bearing test: this repo's own live `.lazyspec.toml` must satisfy
// the schema derived from RawConfig. If the derivation drifts from what the
// deserialize path actually accepts, this fails.
#[test]
fn repo_lazyspec_toml_validates_against_schema() {
    let schema = schema_json();
    let validator = validator_for(&schema).expect("schema compiles");

    let config_toml = include_str!("../../.lazyspec.toml");
    let instance = toml_to_json(config_toml);

    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| format!("{} at {}", e, e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "the repo's own .lazyspec.toml must validate against the generated schema, errors:\n{}",
        errors.join("\n")
    );
}

// A minimal-but-complete config used as the positive control for (c): identical
// to the invalid fixture except for a valid `severity`, proving the rejection
// below is caused specifically by the enum violation, not some unrelated defect.
const VALID_MINIMAL_TOML: &str = r#"
[[types]]
name = "spec"
plural = "specs"
dir = "docs/specs"
prefix = "SPEC"
icon = "S"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
icon = "Y"

[[relationships]]
name = "implements"
inverse = "implemented-by"
traversal = "chain"

[[rules]]
shape = "parent-child"
name = "stories-need-specs"
child = "story"
parent = "spec"
severity = "warning"
"#;

#[test]
fn valid_minimal_config_is_accepted() {
    let schema = schema_json();
    let validator = validator_for(&schema).expect("schema compiles");
    let instance = toml_to_json(VALID_MINIMAL_TOML);
    assert!(
        validator.is_valid(&instance),
        "the positive-control minimal config should validate; errors: {:?}",
        validator
            .iter_errors(&instance)
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
    );
}

// (c) An invalid `severity` (not in the serde-renamed lowercase enum) must be
// rejected. `severity = "fatal"` fails the `parent-child` branch's Severity
// constraint and matches no other rule shape, so the whole document is invalid.
#[test]
fn invalid_severity_value_is_rejected() {
    let invalid_toml =
        VALID_MINIMAL_TOML.replace(r#"severity = "warning""#, r#"severity = "fatal""#);
    assert_ne!(
        invalid_toml, VALID_MINIMAL_TOML,
        "fixture substitution must actually change the severity"
    );

    let schema = schema_json();
    let validator = validator_for(&schema).expect("schema compiles");
    let instance = toml_to_json(&invalid_toml);
    assert!(
        !validator.is_valid(&instance),
        "a rule with severity = \"fatal\" must be rejected by the schema"
    );
}

// (d) Spot-check that the enum constraint sets are exactly the serde-renamed
// wire values, not the Rust variant names. This is what makes (c) meaningful:
// the schema constrains to the lowercase/kebab-case vocabulary the TOML uses.
#[test]
fn enum_defs_use_serde_wire_names() {
    let schema = schema_json();
    let defs = &schema["$defs"];

    let severity: Vec<&str> = defs["Severity"]["enum"]
        .as_array()
        .expect("Severity has an enum")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        severity,
        vec!["error", "warning"],
        "Severity enum must be the lowercase serde names, not Rust variants"
    );

    let backends: Vec<&str> = defs["StoreBackend"]["enum"]
        .as_array()
        .expect("StoreBackend has an enum")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    for expected in [
        "filesystem",
        "github-issues",
        "github-milestones",
        "github-projects",
        "git-ref",
        "clickup-tasks",
    ] {
        assert!(
            backends.contains(&expected),
            "StoreBackend enum must contain kebab-case variant {expected:?}, got {backends:?}"
        );
    }
}
