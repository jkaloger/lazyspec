use anyhow::Result;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::engine::config::{
    default_normalize, default_skills_entry, Authorship, Config, Edge, Lifecycle,
    NumberingStrategy, RelationshipDef, ReservedFormat, Severity, TypeDef, ValidationRule,
};

pub fn write_config_in_place(existing_src: &str, buffer: &Config) -> Result<String> {
    let mut doc: DocumentMut = existing_src.parse()?;

    write_naming(&mut doc, buffer);
    write_ref_count_ceiling(&mut doc, buffer);
    write_templates(&mut doc, buffer);
    write_types(&mut doc, buffer);
    write_relationships(&mut doc, buffer);
    write_tui(&mut doc, buffer);
    write_numbering(&mut doc, buffer);
    write_github(&mut doc, buffer);
    write_coordination(&mut doc, buffer);
    write_certification(&mut doc, buffer);
    write_agents(&mut doc, buffer);
    write_skills(&mut doc, buffer);
    write_rules(&mut doc, buffer);

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

fn write_types(doc: &mut DocumentMut, buffer: &Config) {
    if !doc.contains_key("types") {
        if buffer.documents.types.is_empty() {
            return;
        }
        doc.insert("types", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let Some(types) = doc.get_mut("types").and_then(Item::as_array_of_tables_mut) else {
        return;
    };
    reconcile_array_of_tables(
        types,
        &buffer.documents.types,
        |def| def.name.as_str(),
        update_type_table,
    );
}

// Update the scalar keys of an existing [[types]] table in place, preserving its
// decor. Also used to populate a freshly appended table (a new Table is empty, so
// every key is inserted).
fn update_type_table(entry: &mut Table, def: &TypeDef) {
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
    set_opt_str(entry, "intent", def.intent.as_deref());
    set_str_defaulted(
        entry,
        "authorship",
        authorship_str(&def.authorship),
        "assisted",
    );
    set_opt_int(entry, "clickup_task_type", def.clickup_task_type);
    set_lifecycle(entry, &def.lifecycle);
}

fn authorship_str(a: &Authorship) -> &'static str {
    match a {
        Authorship::Human => "human",
        Authorship::Assisted => "assisted",
        Authorship::Generated => "generated",
    }
}

// Reconcile the `lifecycle` inline table to the buffer. When the entry has no
// `lifecycle` key it is injected from a non-empty buffer (the migration path); a
// present `lifecycle` is rewritten only when the buffer differs from what the
// source declares, so an unchanged type keeps its existing representation
// (decor/formatting) untouched.
fn set_lifecycle(entry: &mut Table, lifecycle: &Lifecycle) {
    if !entry.contains_key("lifecycle") {
        if lifecycle.states.is_empty() {
            return;
        }
        entry.insert(
            "lifecycle",
            Item::Value(Value::InlineTable(lifecycle_inline(lifecycle))),
        );
        return;
    }
    if entry.get("lifecycle").and_then(parse_lifecycle).as_ref() == Some(lifecycle) {
        return;
    }
    entry.insert(
        "lifecycle",
        Item::Value(Value::InlineTable(lifecycle_inline(lifecycle))),
    );
}

fn parse_lifecycle(item: &Item) -> Option<Lifecycle> {
    let table = item.as_inline_table()?;
    let states = table
        .get("states")?
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect::<Option<Vec<_>>>()?;
    let edges = table
        .get("edges")?
        .as_array()?
        .iter()
        .map(|v| {
            let e = v.as_inline_table()?;
            Some(Edge {
                from: e.get("from")?.as_str()?.to_string(),
                to: e.get("to")?.as_str()?.to_string(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Lifecycle { states, edges })
}

fn lifecycle_inline(lifecycle: &Lifecycle) -> InlineTable {
    let mut table = InlineTable::new();
    let states: Array = lifecycle.states.iter().map(|s| s.as_str()).collect();
    table.insert("states", Value::Array(states));
    let mut edges = Array::new();
    for edge in &lifecycle.edges {
        let mut e = InlineTable::new();
        e.insert("from", edge.from.as_str().into());
        e.insert("to", edge.to.as_str().into());
        edges.push(Value::InlineTable(e));
    }
    table.insert("edges", Value::Array(edges));
    table
}

fn write_relationships(doc: &mut DocumentMut, buffer: &Config) {
    if !doc.contains_key("relationships") {
        if buffer.relationships.is_empty() {
            return;
        }
        doc.insert("relationships", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let Some(relationships) = doc
        .get_mut("relationships")
        .and_then(Item::as_array_of_tables_mut)
    else {
        return;
    };
    reconcile_array_of_tables(
        relationships,
        &buffer.relationships,
        |def| def.name.as_str(),
        update_relationship_table,
    );
}

fn update_relationship_table(entry: &mut Table, def: &RelationshipDef) {
    set_str(entry, "name", &def.name);
    set_opt_str(entry, "inverse", def.inverse.as_deref());
    set_opt_str(entry, "github_native", def.github_native.as_deref());
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
    // The [certification] parent is fabricated only when needed -- either a
    // non-default normalize on an absent section (preserved 188 behavior is
    // delegated to set_bool_defaulted) or a non-empty overrides map below.
    let has_overrides = !buffer.certification.overrides.is_empty();
    if !doc.contains_key("certification") {
        if !has_overrides && buffer.certification.normalize == default_normalize() {
            return;
        }
        let mut table = Table::new();
        // Implicit so a bare `[certification]` header is only emitted if the
        // section carries its own scalar (normalize); a sub-table-only section
        // renders as `[certification.overrides...]`.
        table.set_implicit(true);
        doc.insert("certification", Item::Table(table));
    }
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
        default_normalize(),
    );

    write_overrides(certification, buffer);
}

// Reconcile [certification.overrides] to the buffer map: drop sub-tables whose key
// left the map, update/insert a `normalize` sub-table for every buffer key, and
// remove the parent overrides table entirely when the buffer map is empty (no
// dangling empty table is left behind, and none is fabricated for an empty map).
fn write_overrides(certification: &mut dyn toml_edit::TableLike, buffer: &Config) {
    if buffer.certification.overrides.is_empty() {
        certification.remove("overrides");
        return;
    }

    if !certification.contains_key("overrides") {
        let mut table = Table::new();
        table.set_implicit(true);
        certification.insert("overrides", Item::Table(table));
    }
    let Some(overrides) = certification
        .get_mut("overrides")
        .and_then(Item::as_table_like_mut)
    else {
        return;
    };

    let stale: Vec<String> = overrides
        .iter()
        .map(|(key, _)| key.to_string())
        .filter(|key| !buffer.certification.overrides.contains_key(key))
        .collect();
    for key in stale {
        overrides.remove(&key);
    }

    for (key, cfg) in &buffer.certification.overrides {
        if !overrides.contains_key(key) {
            overrides.insert(key, Item::Table(Table::new()));
        }
        if let Some(sub) = overrides.get_mut(key).and_then(Item::as_table_like_mut) {
            set_bool(sub, "normalize", cfg.normalize);
        }
    }
}

fn write_agents(doc: &mut DocumentMut, buffer: &Config) {
    let Some(agents) = doc.get_mut("agents").and_then(Item::as_table_like_mut) else {
        return;
    };
    set_opt_str(agents, "interactive", buffer.agents.interactive.as_deref());
}

fn write_skills(doc: &mut DocumentMut, buffer: &Config) {
    if !doc.contains_key("skills") {
        if buffer.skills.entry == default_skills_entry() {
            return;
        }
        doc.insert("skills", Item::Table(Table::new()));
    }
    let Some(skills) = doc.get_mut("skills").and_then(Item::as_table_like_mut) else {
        return;
    };
    set_str_defaulted(
        skills,
        "entry",
        &buffer.skills.entry,
        &default_skills_entry(),
    );
}

fn write_rules(doc: &mut DocumentMut, buffer: &Config) {
    if !doc.contains_key("rules") {
        if buffer.rules.is_empty() {
            return;
        }
        doc.insert("rules", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let Some(rules) = doc.get_mut("rules").and_then(Item::as_array_of_tables_mut) else {
        return;
    };
    reconcile_array_of_tables(rules, &buffer.rules, rule_name, update_rule_table);
}

fn rule_name(rule: &ValidationRule) -> &str {
    match rule {
        ValidationRule::ParentChild { name, .. } => name,
        ValidationRule::RelationExistence { name, .. } => name,
    }
}

fn update_rule_table(entry: &mut Table, rule: &ValidationRule) {
    let shape_changed = entry.get("shape").and_then(Item::as_str) != Some(rule_shape(rule));
    // A shape (enum) edit switches the rule's variant, so the previous variant's
    // body keys are no longer valid and must be cleared before the new variant is
    // written. `name`/`severity` are common to both variants.
    if shape_changed {
        for key in [
            "child",
            "parent",
            "link",
            "type",
            "require",
            "require_parent_status",
        ] {
            entry.remove(key);
        }
    }
    match rule {
        ValidationRule::ParentChild {
            name,
            child,
            parent,
            severity,
            require_parent_status,
        } => {
            set_str(entry, "name", name);
            set_str(entry, "shape", "parent-child");
            set_str(entry, "child", child);
            set_str(entry, "parent", parent);
            set_str(entry, "severity", severity_str(severity));
            set_opt_str(
                entry,
                "require_parent_status",
                require_parent_status.as_deref(),
            );
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

// Reconcile an `[[...]]` array-of-tables to `buffer` entries by IDENTITY (the
// entry's `name`, unique per collection). Each surviving source table is updated
// in place via `update` (preserving its decor/comments); deleted source tables
// are removed by retaining only names still in the buffer (a middle delete drops
// only that one table, others keep their comments); new buffer entries (names not
// in the source) are appended as fresh tables. A rename (buffer name absent from
// source) is treated as remove-old + append-new.
fn reconcile_array_of_tables<T>(
    tables: &mut ArrayOfTables,
    buffer: &[T],
    name_of: impl Fn(&T) -> &str,
    update: impl Fn(&mut Table, &T),
) {
    let buffer_names: Vec<&str> = buffer.iter().map(&name_of).collect();
    tables.retain(|table| {
        let name = table.get("name").and_then(Item::as_str);
        name.is_some_and(|n| buffer_names.contains(&n))
    });
    for def in buffer {
        let name = name_of(def);
        let existing = tables
            .iter_mut()
            .find(|t| t.get("name").and_then(Item::as_str) == Some(name));
        match existing {
            Some(table) => update(table, def),
            None => {
                let mut table = Table::new();
                update(&mut table, def);
                tables.push(table);
            }
        }
    }
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

// An `Option<i64>` field: Some writes/updates the key, None removes a present key.
fn set_opt_int(table: &mut dyn toml_edit::TableLike, key: &str, value: Option<i64>) {
    match value {
        Some(v) => set_int(table, key, v),
        None => {
            table.remove(key);
        }
    }
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
    use crate::engine::config::{
        CertificationOverride, Config, RelationshipDef, StoreBackend, TypeDef,
    };

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
severity = "error"
"#;

    // AC5 (writer): a parent-child rule with `require_parent_status` round-trips.
    #[test]
    fn require_parent_status_round_trips() {
        let buffer = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.rules[0] = ValidationRule::ParentChild {
                name: "story-has-rfc".to_string(),
                child: "story".to_string(),
                parent: "rfc".to_string(),
                severity: Severity::Error,
                require_parent_status: Some("accepted".to_string()),
            };
            c
        };

        let out = write_config_in_place(RULES_SRC, &buffer).unwrap();
        assert!(out.contains(r#"require_parent_status = "accepted""#));
        let reparsed = Config::parse(&out).unwrap();
        match &reparsed.rules[0] {
            ValidationRule::ParentChild {
                require_parent_status,
                ..
            } => assert_eq!(require_parent_status.as_deref(), Some("accepted")),
            other => panic!("unexpected rule: {other:?}"),
        }
    }

    // AC5 (writer): a shape change (parent-child -> relation-existence) drops the
    // require_parent_status key.
    #[test]
    fn shape_change_drops_require_parent_status() {
        const SRC_WITH_REQ: &str = r#"[[types]]
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
severity = "error"
require_parent_status = "accepted"
"#;
        let buffer = {
            let mut c = Config::parse(SRC_WITH_REQ).unwrap();
            c.rules[0] = ValidationRule::RelationExistence {
                name: "story-has-rfc".to_string(),
                doc_type: "story".to_string(),
                require: "implements".to_string(),
                severity: Severity::Error,
            };
            c
        };

        let out = write_config_in_place(SRC_WITH_REQ, &buffer).unwrap();
        assert!(!out.contains("require_parent_status"));
        Config::parse(&out).unwrap();
    }

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
    fn adding_a_type_appends_and_preserves_existing_comments() {
        // SRC has 2 [[types]]; the `# document types follow` comment sits above the
        // first. Append a third type to the buffer.
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.types.push(TypeDef {
                name: "adr".to_string(),
                plural: "adrs".to_string(),
                dir: "docs/adrs".to_string(),
                prefix: "ADR".to_string(),
                icon: Some("#".to_string()),
                numbering: NumberingStrategy::default(),
                subdirectory: false,
                store: StoreBackend::default(),
                singleton: false,
                parent_type: None,
                agents: Vec::new(),
                intent: None,
                authorship: Default::default(),
                lifecycle: Default::default(),
                attributes: Default::default(),
                label_override: None,
                github_issue_tag: None,
                github_issue_type: None,
                clickup_list_id: None,
                clickup_task_type: None,
                clickup_custom_field_map: None,
            });
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        // The existing entries' surrounding comment survives.
        assert!(out.contains("# document types follow"));
        // The new entry is present.
        assert!(out.contains(r#"name = "adr""#));
        assert!(out.contains(r#"prefix = "ADR""#));
        // Re-parses with exactly three types in declared order.
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.documents.types.len(), 3);
        assert_eq!(reparsed.documents.types[2].name, "adr");
        assert_eq!(reparsed.documents.types[2].icon.as_deref(), Some("#"));
    }

    const RELS_WITH_COMMENTS_SRC: &str = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

# first relationship
[[relationships]]
name = "implements"
inverse = "implemented-by"

# second relationship
[[relationships]]
name = "supersedes"
inverse = "superseded-by"

# third relationship
[[relationships]]
name = "related-to"
"#;

    #[test]
    fn deleting_a_middle_relationship_removes_it_and_keeps_survivors_decor() {
        // Drop the MIDDLE relationship (supersedes) from the buffer.
        let buffer = {
            let mut c = Config::parse(RELS_WITH_COMMENTS_SRC).unwrap();
            c.relationships.retain(|r| r.name != "supersedes");
            c
        };

        let out = write_config_in_place(RELS_WITH_COMMENTS_SRC, &buffer).unwrap();

        // The deleted entry's block is gone.
        assert!(!out.contains(r#"name = "supersedes""#));
        assert!(!out.contains("# second relationship"));
        // The other two entries' comments survive.
        assert!(out.contains("# first relationship"));
        assert!(out.contains("# third relationship"));
        assert!(out.contains(r#"name = "implements""#));
        assert!(out.contains(r#"name = "related-to""#));

        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.relationships.len(), 2);
        assert!(reparsed.relationship_by_name("implements").is_some());
        assert!(reparsed.relationship_by_name("related-to").is_some());
        assert!(reparsed.relationship_by_name("supersedes").is_none());
    }

    #[test]
    fn add_and_delete_in_one_render() {
        // Buffer: add a type AND remove the one rule.
        let buffer = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.documents.types.push(TypeDef {
                name: "spec".to_string(),
                plural: "specs".to_string(),
                dir: "docs/specs".to_string(),
                prefix: "SPEC".to_string(),
                icon: None,
                numbering: NumberingStrategy::default(),
                subdirectory: false,
                store: StoreBackend::default(),
                singleton: false,
                parent_type: None,
                agents: Vec::new(),
                intent: None,
                authorship: Default::default(),
                lifecycle: Default::default(),
                attributes: Default::default(),
                label_override: None,
                github_issue_tag: None,
                github_issue_type: None,
                clickup_list_id: None,
                clickup_task_type: None,
                clickup_custom_field_map: None,
            });
            c.rules.clear();
            c
        };

        let out = write_config_in_place(RULES_SRC, &buffer).unwrap();

        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.documents.types.len(), 2);
        assert!(reparsed.type_by_name("spec").is_some());
        assert!(reparsed.rules.is_empty());
        assert!(!out.contains(r#"name = "story-has-rfc""#));
    }

    #[test]
    fn relationship_scalar_edit_persists() {
        // Source carries `implements`/`implemented-by`. Change the inverse to a new
        // value (Some -> other), then to None (Some -> None). This closes the latent
        // slice-3 gap where relationship scalar edits were never written.
        let buffer_some = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.relationships[0].inverse = Some("done-by".to_string());
            c
        };
        let out = write_config_in_place(RULES_SRC, &buffer_some).unwrap();
        assert!(out.contains(r#"inverse = "done-by""#));
        assert!(!out.contains(r#"inverse = "implemented-by""#));
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed.relationship_by_name("implements").unwrap().inverse,
            Some("done-by".to_string())
        );

        let buffer_none = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.relationships[0].inverse = None;
            c
        };
        let out = write_config_in_place(RULES_SRC, &buffer_none).unwrap();
        assert!(!out.contains("inverse"));
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed.relationship_by_name("implements").unwrap().inverse,
            None
        );
    }

    #[test]
    fn adding_a_relationship_appends_and_reparses() {
        let buffer = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.relationships.push(RelationshipDef {
                name: "blocks".to_string(),
                inverse: Some("blocked-by".to_string()),
                github_native: None,
                traversal: None,
            });
            c
        };
        let out = write_config_in_place(RULES_SRC, &buffer).unwrap();
        assert!(out.contains(r#"name = "blocks""#));
        assert!(out.contains(r#"inverse = "blocked-by""#));
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.relationships.len(), 2);
        assert!(reparsed.relationship_by_name("blocks").is_some());
    }

    #[test]
    fn github_native_sub_issue_round_trips_through_writer() {
        let buffer = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.relationships.push(RelationshipDef {
                name: "child".to_string(),
                inverse: Some("parent".to_string()),
                github_native: Some("sub-issue".to_string()),
                traversal: None,
            });
            c
        };
        let out = write_config_in_place(RULES_SRC, &buffer).unwrap();
        assert!(out.contains(r#"github_native = "sub-issue""#), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed
                .relationship_by_name("child")
                .unwrap()
                .github_native
                .as_deref(),
            Some("sub-issue")
        );
    }

    #[test]
    fn adding_a_rule_appends_and_reparses() {
        let buffer = {
            let mut c = Config::parse(RULES_SRC).unwrap();
            c.rules.push(ValidationRule::RelationExistence {
                name: "adrs-need-relations".to_string(),
                doc_type: "adr".to_string(),
                require: "any-relation".to_string(),
                severity: Severity::Error,
            });
            c
        };
        let out = write_config_in_place(RULES_SRC, &buffer).unwrap();
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.rules.len(), 2);
        assert!(matches!(
            &reparsed.rules[1],
            ValidationRule::RelationExistence { name, .. } if name == "adrs-need-relations"
        ));
    }

    const OVERRIDES_SRC: &str = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[certification]
normalize = true

[certification.overrides."docs/specs/SPEC-001"]
normalize = false
"#;

    #[test]
    fn overrides_add_key_creates_subtable() {
        let buffer = {
            let mut c = Config::parse(OVERRIDES_SRC).unwrap();
            c.certification.overrides.insert(
                "docs/specs/SPEC-002".to_string(),
                CertificationOverride { normalize: false },
            );
            c
        };
        let out = write_config_in_place(OVERRIDES_SRC, &buffer).unwrap();
        assert!(out.contains(r#"[certification.overrides."docs/specs/SPEC-002"]"#));
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.certification.overrides.len(), 2);
        assert!(reparsed
            .certification
            .overrides
            .contains_key("docs/specs/SPEC-002"));
    }

    #[test]
    fn overrides_remove_key_drops_subtable() {
        let buffer = {
            let mut c = Config::parse(OVERRIDES_SRC).unwrap();
            c.certification.overrides.clear();
            c
        };
        let out = write_config_in_place(OVERRIDES_SRC, &buffer).unwrap();
        assert!(!out.contains("SPEC-001"));
        let reparsed = Config::parse(&out).unwrap();
        assert!(reparsed.certification.overrides.is_empty());
    }

    #[test]
    fn overrides_fabricated_when_absent_but_buffer_has_them() {
        // SRC has no [certification.overrides] table at all; the buffer adds one.
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.certification.overrides.insert(
                "docs/specs/SPEC-009".to_string(),
                CertificationOverride { normalize: false },
            );
            c
        };
        let out = write_config_in_place(SRC, &buffer).unwrap();
        assert!(out.contains(r#"[certification.overrides."docs/specs/SPEC-009"]"#));
        let reparsed = Config::parse(&out).unwrap();
        assert!(!reparsed
            .certification
            .should_normalize("docs/specs/SPEC-009"));
    }

    #[test]
    fn empty_overrides_map_does_not_fabricate_table() {
        // SRC has no overrides and the buffer has none either: no table appears.
        let buffer = Config::parse(SRC).unwrap();
        let out = write_config_in_place(SRC, &buffer).unwrap();
        assert!(!out.contains("[certification.overrides"));
        Config::parse(&out).unwrap();
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
