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
