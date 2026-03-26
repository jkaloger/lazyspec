use lazyspec::engine::config::{Config, NumberingStrategy, ReservedFormat, Severity, ValidationRule};

#[test]
fn parse_config_from_toml() {
    let toml_str = r#"
[directories]
rfcs = "docs/rfcs"
adrs = "docs/adrs"
stories = "docs/stories"
iterations = "docs/iterations"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.filesystem.directories.rfcs, "docs/rfcs");
    assert_eq!(config.documents.naming.pattern, "{type}-{n:03}-{title}.md");
}

#[test]
fn default_config() {
    let config = Config::default();
    assert_eq!(config.filesystem.directories.rfcs, "docs/rfcs");
    assert_eq!(config.filesystem.directories.adrs, "docs/adrs");
    assert_eq!(config.filesystem.directories.stories, "docs/stories");
    assert_eq!(config.filesystem.directories.iterations, "docs/iterations");
    assert_eq!(config.filesystem.templates.dir, ".lazyspec/templates");
    assert_eq!(config.documents.naming.pattern, "{type}-{n:03}-{title}.md");
}

#[test]
fn default_config_has_four_type_defs() {
    let config = Config::default();
    assert_eq!(config.documents.types.len(), 5);

    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.plural, "rfcs");
    assert_eq!(rfc.dir, "docs/rfcs");
    assert_eq!(rfc.prefix, "RFC");
    assert_eq!(rfc.icon, Some("●".to_string()));

    let story = config.type_by_name("story").unwrap();
    assert_eq!(story.plural, "stories");
    assert_eq!(story.dir, "docs/stories");
    assert_eq!(story.prefix, "STORY");
    assert_eq!(story.icon, Some("▲".to_string()));

    let iteration = config.type_by_name("iteration").unwrap();
    assert_eq!(iteration.plural, "iterations");
    assert_eq!(iteration.dir, "docs/iterations");
    assert_eq!(iteration.prefix, "ITERATION");
    assert_eq!(iteration.icon, Some("◆".to_string()));

    let adr = config.type_by_name("adr").unwrap();
    assert_eq!(adr.plural, "adrs");
    assert_eq!(adr.dir, "docs/adrs");
    assert_eq!(adr.prefix, "ADR");
    assert_eq!(adr.icon, Some("■".to_string()));
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

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.documents.types.len(), 2);

    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.dir, "specs/rfcs");

    let epic = config.type_by_name("epic").unwrap();
    assert_eq!(epic.plural, "epics");
    assert_eq!(epic.prefix, "EPIC");
    assert_eq!(epic.icon, None);
}

#[test]
fn legacy_directories_populates_types() {
    let toml_str = r#"
[directories]
rfcs = "custom/rfcs"
adrs = "custom/adrs"
stories = "custom/stories"
iterations = "custom/iterations"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.documents.types.len(), 4);

    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.dir, "custom/rfcs");
    assert_eq!(config.filesystem.directories.rfcs, "custom/rfcs");
}

#[test]
fn no_types_or_directories_uses_defaults() {
    let toml_str = r#"
[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.documents.types.len(), 5);
    assert_eq!(config.type_by_name("rfc").unwrap().dir, "docs/rfcs");
    assert_eq!(config.filesystem.directories.rfcs, "docs/rfcs");
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

    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("prefix"), "Error should mention missing field 'prefix', got: {err_msg}");
}

#[test]
fn default_config_has_three_default_rules() {
    let config = Config::default();
    assert_eq!(config.rules.len(), 3);
    assert_eq!(
        config.rules[0],
        ValidationRule::ParentChild {
            name: "stories-need-rfcs".to_string(),
            child: "story".to_string(),
            parent: "rfc".to_string(),
            link: "implements".to_string(),
            severity: Severity::Warning,
        }
    );
    assert_eq!(
        config.rules[1],
        ValidationRule::ParentChild {
            name: "iterations-need-stories".to_string(),
            child: "iteration".to_string(),
            parent: "story".to_string(),
            link: "implements".to_string(),
            severity: Severity::Error,
        }
    );
    assert_eq!(
        config.rules[2],
        ValidationRule::RelationExistence {
            name: "adrs-need-relations".to_string(),
            doc_type: "adr".to_string(),
            require: "any-relation".to_string(),
            severity: Severity::Error,
        }
    );
}

#[test]
fn no_rules_section_uses_defaults() {
    let toml_str = r#"
[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
"#;

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.rules.len(), 3);
}

#[test]
fn parse_parent_child_rule() {
    let toml_str = r#"
[[rules]]
shape = "parent-child"
name = "epics-need-themes"
child = "epic"
parent = "theme"
link = "belongs-to"
severity = "warning"
"#;

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.rules.len(), 1);
    assert_eq!(
        config.rules[0],
        ValidationRule::ParentChild {
            name: "epics-need-themes".to_string(),
            child: "epic".to_string(),
            parent: "theme".to_string(),
            link: "belongs-to".to_string(),
            severity: Severity::Warning,
        }
    );
}

#[test]
fn parse_relation_existence_rule() {
    let toml_str = r#"
[[rules]]
shape = "relation-existence"
name = "rfcs-need-relations"
type = "rfc"
require = "any-relation"
severity = "error"
"#;

    let config = Config::parse(toml_str).unwrap();
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

#[test]
fn custom_rules_fully_replace_defaults() {
    let toml_str = r#"
[[rules]]
shape = "relation-existence"
name = "only-this-rule"
type = "rfc"
require = "any-relation"
severity = "warning"
"#;

    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.rules.len(), 1);
    assert_eq!(
        config.rules[0],
        ValidationRule::RelationExistence {
            name: "only-this-rule".to_string(),
            doc_type: "rfc".to_string(),
            require: "any-relation".to_string(),
            severity: Severity::Warning,
        }
    );
}

#[test]
fn invalid_severity_returns_parse_error() {
    let toml_str = r#"
[[rules]]
shape = "parent-child"
name = "bad-rule"
child = "iteration"
parent = "story"
link = "implements"
severity = "fatal"
"#;

    let result = Config::parse(toml_str);
    assert!(result.is_err(), "Expected parse error for invalid severity 'fatal'");
}

#[test]
fn parse_tui_ascii_diagrams_true() {
    let toml_str = r#"
[tui]
ascii_diagrams = true
"#;
    let config = Config::parse(toml_str).unwrap();
    assert!(config.ui.ascii_diagrams);
}

#[test]
fn tui_defaults_to_ascii_diagrams_false() {
    let toml_str = r#"
[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(toml_str).unwrap();
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
[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(toml_str).unwrap();
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
    let config = Config::parse(toml_str).unwrap();
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
    let config = Config::parse(toml_str).unwrap();
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
    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("salt"), "Error should mention salt, got: {msg}");
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
    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("salt"), "Error should mention salt, got: {msg}");
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
    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("min_length"), "Error should mention min_length, got: {msg}");
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
    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("min_length"), "Error should mention min_length, got: {msg}");
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
    let config = Config::parse(toml_str).unwrap();
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
    let config = Config::parse(toml_str).unwrap();
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
    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("sqids"), "Error should mention sqids, got: {msg}");
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
    let config = Config::parse(toml_str);
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
    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("remote"), "Error should mention remote, got: {msg}");
}

#[test]
fn ref_count_ceiling_defaults_to_15() {
    let toml_str = r#"
[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(toml_str).unwrap();
    assert_eq!(config.ref_count_ceiling, 15);
}

#[test]
fn ref_count_ceiling_configurable() {
    let toml_str = r#"
ref_count_ceiling = 20

[templates]
dir = ".lazyspec/templates"
"#;
    let config = Config::parse(toml_str).unwrap();
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
    let result = Config::parse(toml_str);
    assert!(result.is_err());
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains("reserved"), "Error should mention reserved, got: {msg}");
}
