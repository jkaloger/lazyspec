use lazyspec::engine::config::Config;

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
    assert_eq!(config.directories.rfcs, "docs/rfcs");
    assert_eq!(config.naming.pattern, "{type}-{n:03}-{title}.md");
}

#[test]
fn default_config() {
    let config = Config::default();
    assert_eq!(config.directories.rfcs, "docs/rfcs");
    assert_eq!(config.directories.adrs, "docs/adrs");
    assert_eq!(config.directories.stories, "docs/stories");
    assert_eq!(config.directories.iterations, "docs/iterations");
    assert_eq!(config.templates.dir, ".lazyspec/templates");
    assert_eq!(config.naming.pattern, "{type}-{n:03}-{title}.md");
}

#[test]
fn default_config_has_four_type_defs() {
    let config = Config::default();
    assert_eq!(config.types.len(), 4);

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
    assert_eq!(config.types.len(), 2);

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
    assert_eq!(config.types.len(), 4);

    let rfc = config.type_by_name("rfc").unwrap();
    assert_eq!(rfc.dir, "custom/rfcs");
    assert_eq!(config.directories.rfcs, "custom/rfcs");
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
    assert_eq!(config.types.len(), 4);
    assert_eq!(config.type_by_name("rfc").unwrap().dir, "docs/rfcs");
    assert_eq!(config.directories.rfcs, "docs/rfcs");
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
