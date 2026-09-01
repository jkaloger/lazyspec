use anyhow::Result;
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use crate::engine::config::{
    default_git_ref_remote, default_normalize, default_skills_entry, default_table_columns,
    AttrDef, AttrKind, Authorship, Config, Edge, EdgeDef, Lifecycle, NumberingStrategy,
    RelSelector, RelationshipDef, ReservedFormat, Severity, Traversal, TypeDef, TypeSelector,
    ValidationRule, WILDCARD,
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
    write_certification(&mut doc, buffer);
    write_agents(&mut doc, buffer);
    write_skills(&mut doc, buffer);
    write_git_ref(&mut doc, buffer);
    write_rules(&mut doc, buffer);
    write_edges(&mut doc, buffer);

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
    set_opt_str(entry, "github_issue_tag", def.github_issue_tag.as_deref());
    set_opt_str(entry, "github_issue_type", def.github_issue_type.as_deref());
    set_opt_str(entry, "status_authority", def.status_authority.as_deref());
    set_opt_str(entry, "clickup_list_id", def.clickup_list_id.as_deref());
    set_opt_int(entry, "clickup_task_type", def.clickup_task_type);
    set_lifecycle(entry, &def.lifecycle);
    set_attributes(entry, &def.attributes);
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

// Reconcile a type's `attributes` to the buffer as `[[types.attributes]]`
// array-of-tables. An empty buffer removes the key entirely (never emitting a
// dangling `attributes = []`, whose inline form conflicts with any later
// `[[types.attributes]]` block -- the c83bb99 outage shape). A present key that
// already matches the buffer is left untouched (decor preserved); anything else
// -- including a conflicting inline `attributes = [...]` value -- is replaced
// wholesale by the array-of-tables form, so the writer's output always parses.
fn set_attributes(entry: &mut Table, attributes: &[AttrDef]) {
    if attributes.is_empty() {
        entry.remove("attributes");
        return;
    }
    if entry
        .get("attributes")
        .and_then(parse_attributes)
        .as_deref()
        == Some(attributes)
    {
        return;
    }
    let mut tables = ArrayOfTables::new();
    for attr in attributes {
        let mut table = Table::new();
        set_str(&mut table, "name", &attr.name);
        set_str(&mut table, "kind", attr_kind_str(&attr.kind));
        set_bool_defaulted(&mut table, "required", attr.required, false);
        set_str_array_defaulted(&mut table, "values", &attr.values);
        tables.push(table);
    }
    entry.insert("attributes", Item::ArrayOfTables(tables));
}

fn parse_attributes(item: &Item) -> Option<Vec<AttrDef>> {
    let tables = item.as_array_of_tables()?;
    tables
        .iter()
        .map(|table| {
            let name = table.get("name")?.as_str()?.to_string();
            let kind = attr_kind_from_str(table.get("kind")?.as_str()?)?;
            let required = match table.get("required") {
                None => false,
                Some(item) => item.as_bool()?,
            };
            let values = match table.get("values") {
                None => Vec::new(),
                Some(item) => item
                    .as_array()?
                    .iter()
                    .map(|v| v.as_str().map(str::to_string))
                    .collect::<Option<Vec<_>>>()?,
            };
            Some(AttrDef {
                name,
                kind,
                required,
                values,
            })
        })
        .collect()
}

fn attr_kind_str(kind: &AttrKind) -> &'static str {
    match kind {
        AttrKind::Int => "int",
        AttrKind::Float => "float",
        AttrKind::Str => "string",
        AttrKind::Enum => "enum",
        AttrKind::Date => "date",
        AttrKind::Bool => "bool",
    }
}

fn attr_kind_from_str(value: &str) -> Option<AttrKind> {
    match value {
        "int" => Some(AttrKind::Int),
        "float" => Some(AttrKind::Float),
        "string" => Some(AttrKind::Str),
        "enum" => Some(AttrKind::Enum),
        "date" => Some(AttrKind::Date),
        "bool" => Some(AttrKind::Bool),
        _ => None,
    }
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
    set_opt_str(entry, "traversal", def.traversal.map(traversal_str));
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

    set_opt_str(tui, "viewer", buffer.ui.viewer.as_deref());

    write_table(tui, buffer);
    write_status_colors(tui, buffer);
}

// Reconcile [tui.table] to the buffer columns: update/insert the `columns` array
// whenever it differs from the default set, and never fabricate the table for a
// default column set that has no source block (matching absent == default).
fn write_table(tui: &mut dyn toml_edit::TableLike, buffer: &Config) {
    if !tui.contains_key("table") && buffer.ui.table.columns == default_table_columns() {
        return;
    }
    if !tui.contains_key("table") {
        tui.insert("table", Item::Table(Table::new()));
    }
    let Some(table) = tui.get_mut("table").and_then(Item::as_table_like_mut) else {
        return;
    };
    set_str_array_defaulted(table, "columns", &buffer.ui.table.columns);
}

// Reconcile [tui.status_colors] to the buffer map: drop keys that left the map,
// update/insert a scalar `status = "colour"` pair for every buffer entry, and
// remove the parent status_colors table entirely when the buffer map is empty
// (no dangling empty table is left behind, and none is fabricated for an empty
// map).
fn write_status_colors(tui: &mut dyn toml_edit::TableLike, buffer: &Config) {
    if buffer.ui.status_colors.is_empty() {
        tui.remove("status_colors");
        return;
    }

    if !tui.contains_key("status_colors") {
        tui.insert("status_colors", Item::Table(Table::new()));
    }
    let Some(colors) = tui
        .get_mut("status_colors")
        .and_then(Item::as_table_like_mut)
    else {
        return;
    };

    let stale: Vec<String> = colors
        .iter()
        .map(|(key, _)| key.to_string())
        .filter(|key| !buffer.ui.status_colors.contains_key(key))
        .collect();
    for key in stale {
        colors.remove(&key);
    }

    for (key, value) in &buffer.ui.status_colors {
        set_str(colors, key, value);
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

fn write_git_ref(doc: &mut DocumentMut, buffer: &Config) {
    if !doc.contains_key("git-ref") {
        if buffer.git_ref.remote == default_git_ref_remote() {
            return;
        }
        doc.insert("git-ref", Item::Table(Table::new()));
    }
    let Some(git_ref) = doc.get_mut("git-ref").and_then(Item::as_table_like_mut) else {
        return;
    };
    set_str_defaulted(
        git_ref,
        "remote",
        &buffer.git_ref.remote,
        &default_git_ref_remote(),
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

fn write_edges(doc: &mut DocumentMut, buffer: &Config) {
    if !doc.contains_key("edges") {
        if buffer.edges.is_empty() {
            return;
        }
        doc.insert("edges", Item::ArrayOfTables(ArrayOfTables::new()));
    }
    let Some(edges) = doc.get_mut("edges").and_then(Item::as_array_of_tables_mut) else {
        return;
    };
    reconcile_array_of_tables(
        edges,
        &buffer.edges,
        |def| def.name.as_str(),
        update_edge_table,
    );
}

fn update_edge_table(entry: &mut Table, def: &EdgeDef) {
    set_str(entry, "name", &def.name);
    set_type_selector(entry, "from", &def.from);
    set_type_selector(entry, "to", &def.to);
    set_str(entry, "via", rel_selector_str(&def.via));
    set_opt_str(entry, "required", def.required.as_ref().map(severity_str));
    set_opt_str(entry, "traversal", def.traversal.map(traversal_str));
}

// A type position spelled the way a human writes it: the wildcard is a bare
// string (`["*"]` is a wildcard inside a list, which the loader rejects), a lone
// type name is bare, and only a genuine set becomes an array. A source that
// already spells the selector as a list keeps that spelling, so rendering an
// unedited config changes nothing.
fn set_type_selector(entry: &mut Table, key: &str, selector: &TypeSelector) {
    let names = match selector {
        TypeSelector::Any => {
            set_str(entry, key, WILDCARD);
            return;
        }
        TypeSelector::Types(names) => names,
    };
    if array_matches(entry.get(key).and_then(Item::as_array), names) {
        return;
    }
    match names.as_slice() {
        [only] => set_str(entry, key, only),
        many => {
            let array: Array = many.iter().map(|name| name.as_str()).collect();
            set_value(entry, key, Value::Array(array));
        }
    }
}

fn rel_selector_str(via: &RelSelector) -> &str {
    match via {
        RelSelector::Any => WILDCARD,
        RelSelector::Named(name) => name,
    }
}

fn traversal_str(traversal: Traversal) -> &'static str {
    match traversal {
        Traversal::Chain => "chain",
        Traversal::Related => "related",
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
        CertificationOverride, Config, EdgeDef, RelSelector, RelationshipDef, StoreBackend,
        Traversal, TypeDef, TypeSelector,
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
                status_authority: None,
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
                status_authority: None,
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

    fn attr(name: &str, kind: AttrKind, required: bool, values: &[&str]) -> AttrDef {
        AttrDef {
            name: name.to_string(),
            kind,
            required,
            values: values.iter().map(|v| v.to_string()).collect(),
        }
    }

    fn all_kind_attrs() -> Vec<AttrDef> {
        vec![
            attr("estimate", AttrKind::Int, false, &[]),
            attr("weight", AttrKind::Float, false, &[]),
            attr("owner", AttrKind::Str, true, &[]),
            attr("priority", AttrKind::Enum, false, &["low", "high"]),
            attr("due", AttrKind::Date, false, &[]),
            attr("blocked", AttrKind::Bool, false, &[]),
        ]
    }

    // STORY-213 AC1/AC3: an attribute-bearing type writes [[types.attributes]]
    // sub-tables and round-trips through reparse with identical AttrDefs, for
    // all six kinds.
    #[test]
    fn type_attributes_round_trip_all_kinds() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.types[0].attributes = all_kind_attrs();
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        assert!(out.contains("[[types.attributes]]"), "got: {out}");
        assert!(!out.contains("attributes = ["), "no inline form: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.documents.types[0].attributes, all_kind_attrs());
        // The other type gained no attributes key.
        assert!(reparsed.documents.types[1].attributes.is_empty());
    }

    // AC1: a conflicting inline `attributes = []` on the source type is replaced
    // by the array-of-tables form, never left to collide with it (the c83bb99
    // duplicate-key outage shape).
    #[test]
    fn inline_attributes_key_is_replaced_not_duplicated() {
        const INLINE_SRC: &str = r#"[[types]]
name = "bug"
plural = "bugs"
dir = "docs/bugs"
prefix = "BUG"
attributes = []

[[relationships]]
name = "implements"
inverse = "implemented-by"
"#;
        let buffer = {
            let mut c = Config::parse(INLINE_SRC).unwrap();
            c.documents.types[0].attributes =
                vec![attr("severity", AttrKind::Enum, true, &["low", "high"])];
            c
        };

        let out = write_config_in_place(INLINE_SRC, &buffer).unwrap();

        assert!(!out.contains("attributes = []"), "got: {out}");
        assert_eq!(out.matches("[[types.attributes]]").count(), 1);
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed.documents.types[0].attributes,
            vec![attr("severity", AttrKind::Enum, true, &["low", "high"])]
        );

        // An empty buffer removes the inline key rather than keeping the
        // collision-prone `attributes = []` form.
        let empty_buffer = Config::parse(INLINE_SRC).unwrap();
        let out = write_config_in_place(INLINE_SRC, &empty_buffer).unwrap();
        assert!(!out.contains("attributes"), "got: {out}");
        Config::parse(&out).unwrap();
    }

    // Unchanged attributes keep their source representation (decor untouched),
    // and clearing them drops the whole [[types.attributes]] block.
    #[test]
    fn unchanged_attributes_preserve_decor_and_cleared_ones_are_removed() {
        const ATTRS_SRC: &str = r#"[[types]]
name = "bug"
plural = "bugs"
dir = "docs/bugs"
prefix = "BUG"

# how bad is it
[[types.attributes]]
name = "severity"
kind = "enum"
required = true
values = ["low", "high"]

[[relationships]]
name = "implements"
inverse = "implemented-by"
"#;
        // No-op write: the comment above the block survives byte-for-byte.
        let buffer = Config::parse(ATTRS_SRC).unwrap();
        let out = write_config_in_place(ATTRS_SRC, &buffer).unwrap();
        assert_eq!(out, ATTRS_SRC);

        // Clearing the attributes removes the sub-table block.
        let cleared = {
            let mut c = Config::parse(ATTRS_SRC).unwrap();
            c.documents.types[0].attributes.clear();
            c
        };
        let out = write_config_in_place(ATTRS_SRC, &cleared).unwrap();
        assert!(!out.contains("[[types.attributes]]"));
        assert!(!out.contains("severity"));
        let reparsed = Config::parse(&out).unwrap();
        assert!(reparsed.documents.types[0].attributes.is_empty());
    }

    #[test]
    fn unchanged_status_authority_survives_a_rewrite_with_its_decor() {
        const AUTHORITY_SRC: &str = r#"[github]
repo = "owner/repo"

[[types]]
name = "bug"
plural = "bugs"
dir = "docs/bugs"
prefix = "BUG"
store = "github-issues"
# the board that owns this type's lifecycle
status_authority = "PROJECT-7"

[[relationships]]
name = "implements"
inverse = "implemented-by"
"#;
        let buffer = Config::parse(AUTHORITY_SRC).unwrap();

        let out = write_config_in_place(AUTHORITY_SRC, &buffer).unwrap();

        assert_eq!(out, AUTHORITY_SRC);
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed.documents.types[0].status_authority.as_deref(),
            Some("PROJECT-7")
        );
    }

    #[test]
    fn setting_status_authority_writes_the_key() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.types[0].status_authority = Some("PROJECT-7".to_string());
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed.documents.types[0].status_authority.as_deref(),
            Some("PROJECT-7")
        );
    }

    // A freshly appended type carries its attributes in the same render.
    #[test]
    fn adding_a_type_with_attributes_emits_sub_tables() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.documents.types.push(TypeDef {
                attributes: vec![attr("estimate", AttrKind::Int, false, &[])],
                ..TypeDef::test_fixture("bug", StoreBackend::Filesystem)
            });
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        let reparsed = Config::parse(&out).unwrap();
        let bug = reparsed.type_by_name("bug").unwrap();
        assert_eq!(
            bug.attributes,
            vec![attr("estimate", AttrKind::Int, false, &[])]
        );
    }

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

    #[test]
    fn status_colors_round_trip_through_config_write() {
        let buffer = {
            let mut c = Config::parse(STATUSBAR_SRC).unwrap();
            c.ui.status_colors
                .insert("draft".to_string(), "magenta".to_string());
            c.ui.status_colors
                .insert("pending".to_string(), "#336699".to_string());
            c
        };
        let out = write_config_in_place(STATUSBAR_SRC, &buffer).unwrap();
        assert!(out.contains("[tui.status_colors]"), "got: {out}");
        assert!(out.contains(r#"draft = "magenta""#), "got: {out}");
        assert!(out.contains(r##"pending = "#336699""##), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.ui.status_colors, buffer.ui.status_colors);
    }

    #[test]
    fn viewer_round_trips_through_config_write() {
        let buffer = {
            let mut c = Config::parse(STATUSBAR_SRC).unwrap();
            c.ui.viewer = Some("glow".to_string());
            c
        };
        let out = write_config_in_place(STATUSBAR_SRC, &buffer).unwrap();
        assert!(out.contains(r#"viewer = "glow""#), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.ui.viewer.as_deref(), Some("glow"));

        // Clearing the viewer removes the key.
        let cleared = Config::parse(&out).unwrap();
        let out = write_config_in_place(&out, &cleared).unwrap();
        assert!(out.contains(r#"viewer = "glow""#));

        let mut none_buffer = Config::parse(&out).unwrap();
        none_buffer.ui.viewer = None;
        let out = write_config_in_place(&out, &none_buffer).unwrap();
        assert!(!out.contains("viewer"), "got: {out}");
        assert_eq!(Config::parse(&out).unwrap().ui.viewer, None);
    }

    // STORY-218 AC1: a non-default [git-ref] remote survives the config writer.
    #[test]
    fn git_ref_remote_round_trips_through_config_write() {
        let buffer = {
            let mut c = Config::parse(STATUSBAR_SRC).unwrap();
            c.git_ref.remote = "upstream".to_string();
            c
        };
        let out = write_config_in_place(STATUSBAR_SRC, &buffer).unwrap();
        assert!(out.contains("[git-ref]"), "got: {out}");
        assert!(out.contains(r#"remote = "upstream""#), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.git_ref.remote, "upstream");
    }

    // The default remote is not fabricated into a config that omits [git-ref].
    #[test]
    fn git_ref_default_remote_writes_no_section() {
        let buffer = Config::parse(STATUSBAR_SRC).unwrap();
        let out = write_config_in_place(STATUSBAR_SRC, &buffer).unwrap();
        assert!(!out.contains("[git-ref]"), "got: {out}");
    }

    const TRAVERSAL_SRC: &str = r#"[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"
traversal = "chain"

[[relationships]]
name = "related-to"
traversal = "related"

# every adr answers to something
[[rules]]
name = "adrs-need-relations"
shape = "relation-existence"
type = "adr"
require = "any-relation"
severity = "error"

[github]
repo = "owner/repo"
"#;

    // The edge migration deletes every rule, and an array-of-tables reconciled
    // against an empty buffer leaves its key behind, so the key comes off the
    // document outright. Asserted on the text: a reparse cannot tell an absent
    // key from an empty one.
    #[test]
    fn clearing_every_rule_takes_the_rules_key_off_the_document() {
        let buffer = {
            let mut c = Config::parse(TRAVERSAL_SRC).unwrap();
            c.rules.clear();
            c
        };

        let out = write_config_in_place(TRAVERSAL_SRC, &buffer).unwrap();

        assert!(!out.contains("rules"), "got: {out}");
        assert!(
            out.contains("[github]"),
            "the section after it survives: {out}"
        );
        assert!(Config::parse(&out).unwrap().rules.is_empty());
    }

    #[test]
    fn clearing_a_relationship_traversal_removes_the_key() {
        let buffer = {
            let mut c = Config::parse(TRAVERSAL_SRC).unwrap();
            for relationship in &mut c.relationships {
                relationship.traversal = None;
            }
            c
        };

        let out = write_config_in_place(TRAVERSAL_SRC, &buffer).unwrap();

        assert!(!out.contains("traversal"), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert!(reparsed.relationships.iter().all(|r| r.traversal.is_none()));
    }

    #[test]
    fn setting_a_relationship_traversal_writes_the_key() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.relationships[0].traversal = Some(Traversal::Chain);
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        assert!(out.contains(r#"traversal = "chain""#), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed
                .relationship_by_name("implements")
                .unwrap()
                .traversal,
            Some(Traversal::Chain)
        );
    }

    fn wildcard_and_concrete_edges() -> Vec<EdgeDef> {
        vec![
            EdgeDef {
                name: "stories-need-rfcs".to_string(),
                from: TypeSelector::Types(vec!["story".to_string()]),
                to: TypeSelector::Types(vec!["rfc".to_string()]),
                via: RelSelector::Any,
                required: Some(Severity::Warning),
                traversal: None,
            },
            EdgeDef {
                name: "related-to-traversal".to_string(),
                from: TypeSelector::Any,
                to: TypeSelector::Any,
                via: RelSelector::Named("related-to".to_string()),
                required: None,
                traversal: Some(Traversal::Related),
            },
        ]
    }

    #[test]
    fn edges_are_written_with_every_key_and_round_trip() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.edges = wildcard_and_concrete_edges();
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        assert!(out.contains("[[edges]]"), "got: {out}");
        assert!(out.contains(r#"required = "warning""#), "got: {out}");
        assert!(out.contains(r#"traversal = "related""#), "got: {out}");
        assert!(out.contains(r#"via = "related-to""#), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.edges, wildcard_and_concrete_edges());
    }

    // ITERATION-368 Task 3 guards the same spelling in `to_toml`; the in-place
    // writer is a second code path and needs its own assertion. `["*"]` is not
    // just ugly -- a wildcard inside a list is rejected at load.
    #[test]
    fn a_wildcard_edge_position_is_written_as_a_bare_string() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.edges = wildcard_and_concrete_edges();
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        assert!(out.contains(r#"from = "*""#), "got: {out}");
        assert!(out.contains(r#"to = "*""#), "got: {out}");
        assert!(out.contains(r#"via = "*""#), "got: {out}");
        assert!(!out.contains(r#"["*"]"#), "got: {out}");
    }

    #[test]
    fn an_edge_naming_several_target_types_is_written_as_a_list() {
        let buffer = {
            let mut c = Config::parse(SRC).unwrap();
            c.edges = vec![EdgeDef {
                name: "story-parent".to_string(),
                from: TypeSelector::Types(vec!["story".to_string()]),
                to: TypeSelector::Types(vec!["story".to_string(), "rfc".to_string()]),
                via: RelSelector::Named("implements".to_string()),
                required: None,
                traversal: Some(Traversal::Chain),
            }];
            c
        };

        let out = write_config_in_place(SRC, &buffer).unwrap();

        assert!(out.contains(r#"from = "story""#), "got: {out}");
        assert!(out.contains(r#"to = ["story", "rfc"]"#), "got: {out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(
            reparsed.edges[0].to,
            TypeSelector::Types(vec!["story".to_string(), "rfc".to_string()])
        );
    }

    // AC6 at the writer's level: a config already carrying `[[edges]]` is what a
    // second migration run reads, and rendering it must change nothing.
    #[test]
    fn an_unchanged_edge_block_survives_byte_for_byte() {
        const EDGES_SRC: &str = r#"[[types]]
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
name = "implements"
inverse = "implemented-by"

# the chain a story has to hang off
[[edges]]
name = "stories-need-rfcs"
from = "story"
to = ["rfc"]
via = "*"
required = "warning"
"#;
        let buffer = Config::parse(EDGES_SRC).unwrap();

        let out = write_config_in_place(EDGES_SRC, &buffer).unwrap();

        assert_eq!(out, EDGES_SRC);
    }

    #[test]
    fn table_columns_round_trip_through_config_write() {
        let buffer = {
            let mut c = Config::parse(STATUSBAR_SRC).unwrap();
            c.ui.table.columns = vec!["status".to_string(), "priority".to_string()];
            c
        };
        let out = write_config_in_place(STATUSBAR_SRC, &buffer).unwrap();
        assert!(out.contains("[tui.table]"), "got: {out}");
        assert!(
            out.contains(r#"columns = ["status", "priority"]"#),
            "got: {out}"
        );
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.ui.table.columns, buffer.ui.table.columns);
    }
}
