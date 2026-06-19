use anyhow::{bail, Result};
use toml_edit::{Array, DocumentMut, Item, Table, Value};

use crate::engine::config::{Config, NumberingStrategy, ReservedFormat, Severity, ValidationRule};

pub fn write_config_in_place(existing_src: &str, buffer: &Config) -> Result<String> {
    let mut doc: DocumentMut = existing_src.parse()?;

    write_naming(&mut doc, buffer);
    write_ref_count_ceiling(&mut doc, buffer);
    write_templates(&mut doc, buffer);
    write_types(&mut doc, buffer)?;
    write_tui(&mut doc, buffer);
    write_numbering(&mut doc, buffer);
    write_github(&mut doc, buffer);
    write_coordination(&mut doc, buffer);
    write_certification(&mut doc, buffer);
    write_agents(&mut doc, buffer);
    write_rules(&mut doc, buffer)?;

    Ok(doc.to_string())
}

fn write_naming(doc: &mut DocumentMut, buffer: &Config) {
    if let Some(table) = doc.get_mut("naming").and_then(Item::as_table_like_mut) {
        set_str(table, "pattern", &buffer.documents.naming.pattern);
    }
}

fn write_ref_count_ceiling(doc: &mut DocumentMut, buffer: &Config) {
    if doc.contains_key("ref_count_ceiling") {
        set_int(
            doc.as_table_mut(),
            "ref_count_ceiling",
            buffer.ref_count_ceiling as i64,
        );
    }
}

fn write_templates(doc: &mut DocumentMut, buffer: &Config) {
    if let Some(table) = doc.get_mut("templates").and_then(Item::as_table_like_mut) {
        set_str(table, "dir", &buffer.filesystem.templates.dir);
    }
}

fn write_types(doc: &mut DocumentMut, buffer: &Config) -> Result<()> {
    let Some(types) = doc.get_mut("types").and_then(Item::as_array_of_tables_mut) else {
        return Ok(());
    };
    if types.len() != buffer.documents.types.len() {
        bail!("in-place config writer requires equal [[types]] counts (add/remove entries is not supported here)");
    }
    for (entry, def) in types.iter_mut().zip(buffer.documents.types.iter()) {
        set_str(entry, "name", &def.name);
        set_str(entry, "plural", &def.plural);
        set_str(entry, "dir", &def.dir);
        set_str(entry, "prefix", &def.prefix);
        set_opt_str(entry, "icon", def.icon.as_deref());
        set_str_defaulted(
            entry,
            "numbering",
            numbering_str(&def.numbering),
            "incremental",
        );
        set_bool_defaulted(entry, "subdirectory", def.subdirectory, false);
        set_str_defaulted(entry, "store", &def.store.to_string(), "filesystem");
        set_bool_defaulted(entry, "singleton", def.singleton, false);
        set_opt_str(entry, "parent_type", def.parent_type.as_deref());
        set_str_array_defaulted(entry, "agents", &def.agents);
    }
    Ok(())
}

fn write_tui(doc: &mut DocumentMut, buffer: &Config) {
    let Some(tui) = doc.get_mut("tui").and_then(Item::as_table_like_mut) else {
        return;
    };
    set_bool_defaulted(tui, "ascii_diagrams", buffer.ui.ascii_diagrams, false);

    if let Some(statusbar) = tui.get_mut("statusbar").and_then(Item::as_table_like_mut) {
        set_bool_defaulted(statusbar, "enabled", buffer.ui.statusbar.enabled, true);
        set_opt_str_array(statusbar, "left", buffer.ui.statusbar.left.as_deref());
        set_opt_str_array(statusbar, "center", buffer.ui.statusbar.center.as_deref());
        set_opt_str_array(statusbar, "right", buffer.ui.statusbar.right.as_deref());
    }

    if let Some(multiline) = tui.get_mut("multiline").and_then(Item::as_table_like_mut) {
        set_int_defaulted(
            multiline,
            "max_expanded_height",
            buffer.ui.multiline.max_expanded_height as i64,
            5,
        );
    }
}

fn write_numbering(doc: &mut DocumentMut, buffer: &Config) {
    // Each sub-section follows the same Option contract as the top-level
    // dependency sections: Some-and-present updates in place (decor preserved),
    // Some-but-absent fabricates the sub-table under [numbering], and None leaves
    // an absent section absent (section removal is a later slice's concern; no
    // scalar edit nulls these). The parent [numbering] table is created on demand
    // and kept implicit so an empty `[numbering]` header is never emitted.
    if let Some(sqids) = &buffer.documents.sqids {
        let sqids_table = numbering_subtable(doc, "sqids");
        set_str(sqids_table, "salt", &sqids.salt);
        set_int_defaulted(sqids_table, "min_length", sqids.min_length as i64, 3);
    }

    if let Some(reserved) = &buffer.documents.reserved {
        let reserved_table = numbering_subtable(doc, "reserved");
        set_str_defaulted(reserved_table, "remote", &reserved.remote, "origin");
        set_str(
            reserved_table,
            "format",
            reserved_format_str(&reserved.format),
        );
        set_int_defaulted(
            reserved_table,
            "max_retries",
            reserved.max_retries as i64,
            5,
        );
    }
}

// Borrow `[numbering.<key>]`, creating the implicit `[numbering]` parent and/or
// the named sub-table when either is absent. Returns the sub-table for in-place
// key edits, so a present section keeps its decor while a fabricated one is a
// fresh `Item::Table`.
fn numbering_subtable<'a>(doc: &'a mut DocumentMut, key: &str) -> &'a mut dyn toml_edit::TableLike {
    if !doc.contains_key("numbering") {
        let mut numbering = Table::new();
        // Implicit so toml_edit renders the sub-tables (e.g. [numbering.sqids])
        // without emitting a bare `[numbering]` header.
        numbering.set_implicit(true);
        doc.insert("numbering", Item::Table(numbering));
    }
    let numbering = doc
        .get_mut("numbering")
        .and_then(Item::as_table_mut)
        .expect("numbering inserted/present as a table above");
    if !numbering.contains_key(key) {
        numbering.insert(key, Item::Table(Table::new()));
    }
    numbering
        .get_mut(key)
        .and_then(Item::as_table_like_mut)
        .expect("sub-table inserted/present above")
}

fn write_github(doc: &mut DocumentMut, buffer: &Config) {
    let Some(cfg) = &buffer.documents.github else {
        return;
    };
    if !doc.contains_key("github") {
        doc.insert("github", Item::Table(Table::new()));
    }
    let github = doc
        .get_mut("github")
        .and_then(Item::as_table_like_mut)
        .expect("github inserted/present as a table above");
    set_opt_str(github, "repo", cfg.repo.as_deref());
    set_int_defaulted(github, "cache_ttl", cfg.cache_ttl as i64, 60);
}

fn write_coordination(doc: &mut DocumentMut, buffer: &Config) {
    let Some(coordination) = doc
        .get_mut("coordination")
        .and_then(Item::as_table_like_mut)
    else {
        return;
    };
    let Some(cfg) = &buffer.coordination else {
        return;
    };
    set_str_defaulted(coordination, "remote", &cfg.remote, "origin");
    set_str_defaulted(coordination, "lease_duration", &cfg.lease_duration, "60m");
    set_str_defaulted(coordination, "grace_period", &cfg.grace_period, "2m");
    set_int_defaulted(
        coordination,
        "max_push_retries",
        cfg.max_push_retries as i64,
        5,
    );
    set_str_defaulted(coordination, "max_clock_skew", &cfg.max_clock_skew, "5m");
}

fn write_certification(doc: &mut DocumentMut, buffer: &Config) {
    let Some(certification) = doc
        .get_mut("certification")
        .and_then(Item::as_table_like_mut)
    else {
        return;
    };
    set_bool_defaulted(
        certification,
        "normalize",
        buffer.certification.normalize,
        true,
    );

    if let Some(overrides) = certification
        .get_mut("overrides")
        .and_then(Item::as_table_like_mut)
    {
        for (key, value) in overrides.iter_mut() {
            if let Some(override_table) = value.as_table_like_mut() {
                if let Some(cfg) = buffer.certification.overrides.get(key.get()) {
                    set_bool(override_table, "normalize", cfg.normalize);
                }
            }
        }
    }
}

fn write_agents(doc: &mut DocumentMut, buffer: &Config) {
    let Some(agents) = doc.get_mut("agents").and_then(Item::as_table_like_mut) else {
        return;
    };
    set_opt_str(agents, "interactive", buffer.agents.interactive.as_deref());
}

fn write_rules(doc: &mut DocumentMut, buffer: &Config) -> Result<()> {
    let Some(rules) = doc.get_mut("rules").and_then(Item::as_array_of_tables_mut) else {
        return Ok(());
    };
    if rules.len() != buffer.rules.len() {
        bail!("in-place config writer requires equal [[rules]] counts (add/remove entries is not supported here)");
    }
    for (entry, rule) in rules.iter_mut().zip(buffer.rules.iter()) {
        let shape_changed = entry.get("shape").and_then(Item::as_str) != Some(rule_shape(rule));
        // A shape (enum) edit switches the rule's variant, so the previous
        // variant's body keys are no longer valid and must be cleared before the
        // new variant is written. `name`/`severity` are common to both variants.
        if shape_changed {
            for key in ["child", "parent", "link", "type", "require"] {
                entry.remove(key);
            }
        }
        match rule {
            ValidationRule::ParentChild {
                name,
                child,
                parent,
                link,
                severity,
            } => {
                set_str(entry, "name", name);
                set_str(entry, "shape", "parent-child");
                set_str(entry, "child", child);
                set_str(entry, "parent", parent);
                set_str(entry, "link", link);
                set_str(entry, "severity", severity_str(severity));
            }
            ValidationRule::RelationExistence {
                name,
                doc_type,
                require,
                severity,
            } => {
                set_str(entry, "name", name);
                set_str(entry, "shape", "relation-existence");
                set_str(entry, "type", doc_type);
                set_str(entry, "require", require);
                set_str(entry, "severity", severity_str(severity));
            }
        }
    }
    Ok(())
}

fn rule_shape(rule: &ValidationRule) -> &'static str {
    match rule {
        ValidationRule::ParentChild { .. } => "parent-child",
        ValidationRule::RelationExistence { .. } => "relation-existence",
    }
}

fn numbering_str(n: &NumberingStrategy) -> &'static str {
    match n {
        NumberingStrategy::Incremental => "incremental",
        NumberingStrategy::Sqids => "sqids",
        NumberingStrategy::Reserved => "reserved",
    }
}

fn reserved_format_str(f: &ReservedFormat) -> &'static str {
    match f {
        ReservedFormat::Incremental => "incremental",
        ReservedFormat::Sqids => "sqids",
    }
}

fn severity_str(s: &Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

// A required scalar key always exists in a loadable config, so it is updated in
// place (preserving decor) when the value differs.
fn set_str(table: &mut dyn toml_edit::TableLike, key: &str, value: &str) {
    if table.get(key).and_then(Item::as_str) == Some(value) {
        return;
    }
    set_value(table, key, value.into());
}

// An optional scalar key with a serde default. It is mutated only when already
// present (preserving decor); when absent, it is added only if the value is
// non-default. This keeps the writer's output identical to the input wherever a
// default-valued field was simply omitted from the source.
fn set_str_defaulted(table: &mut dyn toml_edit::TableLike, key: &str, value: &str, default: &str) {
    if !table.contains_key(key) && value == default {
        return;
    }
    set_str(table, key, value);
}

// A required bool key (always present in a loadable config); updated in place.
fn set_bool(table: &mut dyn toml_edit::TableLike, key: &str, value: bool) {
    if table.get(key).and_then(Item::as_bool) == Some(value) {
        return;
    }
    set_value(table, key, value.into());
}

fn set_bool_defaulted(table: &mut dyn toml_edit::TableLike, key: &str, value: bool, default: bool) {
    if !table.contains_key(key) && value == default {
        return;
    }
    if table.get(key).and_then(Item::as_bool) == Some(value) {
        return;
    }
    set_value(table, key, value.into());
}

fn set_int(table: &mut dyn toml_edit::TableLike, key: &str, value: i64) {
    if table.get(key).and_then(Item::as_integer) == Some(value) {
        return;
    }
    set_value(table, key, value.into());
}

fn set_int_defaulted(table: &mut dyn toml_edit::TableLike, key: &str, value: i64, default: i64) {
    if !table.contains_key(key) && value == default {
        return;
    }
    set_int(table, key, value);
}

// An `Option<String>` field: Some writes/updates the key (adding it within an
// existing table is allowed), None removes a present key.
fn set_opt_str(table: &mut dyn toml_edit::TableLike, key: &str, value: Option<&str>) {
    match value {
        Some(v) => set_str(table, key, v),
        None => {
            table.remove(key);
        }
    }
}

// A defaulted string array (e.g. a type's `agents`). Added only when non-empty;
// updated in place when already present.
fn set_str_array_defaulted(table: &mut dyn toml_edit::TableLike, key: &str, values: &[String]) {
    if !table.contains_key(key) && values.is_empty() {
        return;
    }
    if array_matches(table.get(key).and_then(Item::as_array), values) {
        return;
    }
    let array: Array = values.iter().map(|v| v.as_str()).collect();
    set_value(table, key, Value::Array(array));
}

// An `Option<Vec<String>>` field (statusbar slots): Some writes/updates, None
// removes a present key.
fn set_opt_str_array(table: &mut dyn toml_edit::TableLike, key: &str, values: Option<&[String]>) {
    match values {
        Some(v) => {
            if array_matches(table.get(key).and_then(Item::as_array), v) {
                return;
            }
            let array: Array = v.iter().map(|s| s.as_str()).collect();
            set_value(table, key, Value::Array(array));
        }
        None => {
            table.remove(key);
        }
    }
}

fn array_matches(array: Option<&Array>, values: &[String]) -> bool {
    let Some(array) = array else {
        // An absent array equals an empty buffer value, so no key is fabricated.
        return values.is_empty();
    };
    if array.len() != values.len() {
        return false;
    }
    array
        .iter()
        .zip(values.iter())
        .all(|(item, value)| item.as_str() == Some(value.as_str()))
}

// Set `key` to `new`, mutating the existing value's inner data so its surrounding
// decor (prefix/suffix whitespace and trailing inline comments) survives. Falls
// back to a plain insert when the key (or a value-shaped item) is absent.
fn set_value(table: &mut dyn toml_edit::TableLike, key: &str, new: Value) {
    if let Some(slot) = table.get_mut(key).and_then(Item::as_value_mut) {
        let decor = slot.decor().clone();
        *slot = new;
        *slot.decor_mut() = decor;
    } else {
        table.insert(key, Item::Value(new));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;

    const SRC: &str = r#"# lazyspec configuration
ref_count_ceiling = 15

[naming]
pattern = "{type}-{n:03}-{title}.md"  # filename template

[templates]
dir = ".lazyspec/templates"

# document types follow
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "*"

[[types]]
name = "story"
plural = "stories"
dir = "docs/stories"
prefix = "STORY"
parent_type = "rfc"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "related-to"
"#;

    fn changed_lines<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
        before
            .lines()
            .zip(after.lines())
            .filter(|(b, a)| b != a)
            .collect()
    }

    #[test]
    fn preserves_comments_and_only_changes_one_value() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.naming.pattern = "{type}-{title}.md".to_string();
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        // (a) the standalone comment survives.
        assert!(out.contains("# lazyspec configuration"));
        // The inline trailing comment on the pattern line survives the rewrite.
        assert!(out.contains("# filename template"));
        // (b) re-parsing succeeds.
        Config::parse(&out).unwrap();
        // (c) only the one value changed, nothing else.
        assert!(out.contains("{type}-{title}.md"));
        assert!(!out.contains("{type}-{n:03}-{title}.md"));
        assert_eq!(before_and_after_line_count(SRC, &out), 1);
    }

    fn before_and_after_line_count(before: &str, after: &str) -> usize {
        assert_eq!(
            before.lines().count(),
            after.lines().count(),
            "line count must be stable when only a scalar value changes"
        );
        changed_lines(before, after).len()
    }

    #[test]
    fn option_none_removes_key_and_some_sets_it() {
        // None on an existing Option key removes the key.
        let buffer_none = {
            let mut c = Config::parse(SRC).unwrap();
            // story is types[1]; clear its parent_type.
            c.documents.types[1].parent_type = None;
            c
        };
        let out_none = write_config_in_place(SRC, &buffer_none).unwrap();
        assert!(!out_none.contains("parent_type"));
        Config::parse(&out_none).unwrap();

        // Some on an existing Option key writes the new value.
        let buffer_some = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.types[0].icon = Some("@".to_string());
            c
        };
        let out_some = write_config_in_place(SRC, &buffer_some).unwrap();
        assert!(out_some.contains("icon = \"@\""));
        assert!(!out_some.contains("icon = \"*\""));
        Config::parse(&out_some).unwrap();
    }

    #[test]
    fn absent_optional_section_with_none_buffer_stays_absent() {
        // SRC has no [github]/[numbering] sections, and the buffer leaves those
        // Option fields None: the writer must not invent any of them.
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.github = None;
            c.documents.sqids = None;
            c.documents.reserved = None;
            c
        };
        let out = write_config_in_place(SRC, &buffer).unwrap();
        assert!(!out.contains("[github]"));
        assert!(!out.contains("[numbering"));
        Config::parse(&out).unwrap();
    }

    #[test]
    fn some_optional_section_is_fabricated_when_absent() {
        use crate::engine::config::{GithubConfig, ReservedConfig, SqidsConfig};

        // [github]: Some-but-absent -> fabricate a top-level table; repo present.
        let github_buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.github = Some(GithubConfig {
                repo: Some("owner/repo".to_string()),
                cache_ttl: 99,
            });
            c
        };
        let out = write_config_in_place(SRC, &github_buffer).unwrap();
        assert!(out.contains("[github]"), "github section fabricated");
        assert!(out.contains(r#"repo = "owner/repo""#));
        assert!(out.contains("cache_ttl = 99"));
        let reparsed = Config::parse(&out).unwrap();
        let gh = reparsed.documents.github.unwrap();
        assert_eq!(gh.repo.as_deref(), Some("owner/repo"));
        assert_eq!(gh.cache_ttl, 99);

        // [numbering.reserved]: Some-but-absent -> fabricate a sub-table under an
        // implicit [numbering] parent (no bare [numbering] header).
        let reserved_buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.reserved = Some(ReservedConfig {
                remote: "upstream".to_string(),
                format: ReservedFormat::Incremental,
                max_retries: 9,
            });
            c
        };
        let out = write_config_in_place(SRC, &reserved_buffer).unwrap();
        assert!(
            out.contains("[numbering.reserved]"),
            "reserved sub-table fabricated"
        );
        assert!(
            !out.contains("[numbering]\n") && !out.ends_with("[numbering]"),
            "no bare [numbering] header is emitted"
        );
        let reparsed = Config::parse(&out).unwrap();
        let r = reparsed.documents.reserved.unwrap();
        assert_eq!(r.remote, "upstream");
        assert_eq!(r.format, ReservedFormat::Incremental);
        assert_eq!(r.max_retries, 9);

        // [numbering.sqids]: Some-but-absent (non-empty salt) -> fabricate a
        // sub-table; the non-empty salt keeps Config::parse happy.
        let sqids_buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.sqids = Some(SqidsConfig {
                salt: "round-trip-salt".to_string(),
                min_length: 7,
            });
            c
        };
        let out = write_config_in_place(SRC, &sqids_buffer).unwrap();
        assert!(
            out.contains("[numbering.sqids]"),
            "sqids sub-table fabricated"
        );
        let reparsed = Config::parse(&out).unwrap();
        let s = reparsed.documents.sqids.unwrap();
        assert_eq!(s.salt, "round-trip-salt");
        assert_eq!(s.min_length, 7);
    }

    const RULES_SRC: &str = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[rules]]
name = "story-has-rfc"
shape = "parent-child"
child = "story"
parent = "rfc"
link = "implements"
severity = "error"
"#;

    #[test]
    fn shape_change_clears_stale_variant_keys() {
        let buffer = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.rules[0] = ValidationRule::RelationExistence {
                name: "story-has-rfc".to_string(),
                doc_type: "story".to_string(),
                require: "implements".to_string(),
                severity: Severity::Error,
            };
            c
        };

        let out = write_config_in_place(RULES_SRC, &buffer).unwrap();

        assert!(out.contains(r#"shape = "relation-existence""#));
        assert!(out.contains(r#"type = "story""#));
        assert!(out.contains(r#"require = "implements""#));
        assert!(!out.contains("child"));
        assert!(!out.contains("parent"));
        assert!(!out.contains("link"));
        Config::parse(&out).unwrap();
    }

    #[test]
    fn type_count_mismatch_errors() {
        // One extra [[types]] entry in the buffer.
        let buffer_more = {
            let mut c = Config::parse(SRC).unwrap();
            let extra = c.documents.types[0].clone();
            c.documents.types.push(extra);
            c
        };
        assert!(write_config_in_place(SRC, &buffer_more).is_err());

        // One fewer [[types]] entry in the buffer.
        let buffer_fewer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.types.pop();
            c
        };
        assert!(write_config_in_place(SRC, &buffer_fewer).is_err());
    }

    const STATUSBAR_SRC: &str = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[tui]

[tui.statusbar]
enabled = true
left = ["mode"]
"#;

    #[test]
    fn empty_array_on_absent_slot_is_not_fabricated() {
        // `right` is absent in the source; an empty buffer list must keep it absent.
        let buffer = {
            let mut c = Config::parse(STATUSBAR_SRC).unwrap();
            c.ui.statusbar.right = Some(vec![]);
            c
        };
        let out = write_config_in_place(STATUSBAR_SRC, &buffer).unwrap();
        assert!(!out.contains("right"));
        Config::parse(&out).unwrap();
    }
}
