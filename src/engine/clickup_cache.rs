//! ClickUp read path: fetch a bound List's tasks and materialize each as a
//! read-only cache document under `.lazyspec/cache/<type>/<ID>.md`, in the same
//! shape the github-issues cache uses. Parallel to
//! [`issue_cache`](crate::engine::issue_cache); it reuses
//! [`write_cache_file`](crate::engine::store_dispatch::write_cache_file) and
//! [`TaskMap`] unchanged rather than inventing a second cache mechanism.
//!
//! Mapping (RFC-056 §Field mapping): doc `status` is the raw ClickUp status
//! string verbatim; `priority`/`estimate`/`due` come from ClickUp's native task
//! fields, not a body blob. The write path and relation decoding are later
//! stories and live elsewhere.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::engine::clickup::{
    ClickupClient, ClickupStatus, ClickupTask, TaskAssigneeUpdate, TaskCreate, TaskUpdate,
};
use crate::engine::config::{Lifecycle, TypeDef, CLICKUP_RELATIONS_FIELD};
use crate::engine::document::{self, AttrValue, DocMeta, DocType, Relation, Status};
use crate::engine::store_dispatch::write_cache_file;
use crate::engine::task_map::TaskMap;

/// The outcome of fetching one ClickUp-backed type, mirroring the github-issues
/// fetch summary the CLI reports.
#[derive(Debug)]
pub struct ClickupFetchResult {
    pub fetched: usize,
    pub new: usize,
    pub removed: usize,
}

/// Fetch every task in `type_def`'s bound List and rewrite the type's cache
/// directory from it. The fetch is authoritative for the whole type dir: tasks
/// no longer present (including ones ClickUp archived, which drop out of the
/// list response) leave the cache and the [`TaskMap`].
pub fn fetch_tasks(
    root: &Path,
    type_def: &TypeDef,
    client: &dyn ClickupClient,
    token: &str,
    task_map: &mut TaskMap,
) -> Result<ClickupFetchResult> {
    let list_id = type_def.clickup_list_id.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "type '{}' is clickup-tasks but has no clickup_list_id configured",
            type_def.name
        )
    })?;

    let tasks = client
        .task_list(token, list_id)
        .with_context(|| format!("fetching ClickUp tasks for list {}", list_id))?;

    // When the type binds a custom task type, materialize only tasks of that type:
    // a List can hold several task types, but a lazyspec type maps to exactly one.
    // Unset means the type spans the whole List, so every task materializes.
    let tasks: Vec<ClickupTask> = match type_def.clickup_task_type {
        Some(expected) => tasks
            .into_iter()
            .filter(|task| task.custom_item_id == Some(expected))
            .collect(),
        None => tasks,
    };

    let cache_root = root.join(".lazyspec/cache");
    let cache_dir = cache_root.join(&type_def.name);
    let previously_cached: HashSet<String> = list_cached_ids(&cache_dir);

    // A full fetch owns the whole type directory; rebuild it so a task removed
    // from the List leaves no stale cache file behind. The rebuild happens in a
    // staging dir swapped into place only when every write succeeded, so a
    // failure partway (disk full, crash) leaves the previous cache intact
    // (AUDIT-018 C4).
    std::fs::create_dir_all(&cache_root)?;
    let staging_root = cache_root.join(format!(".staging-{}", type_def.name));
    if staging_root.exists() {
        std::fs::remove_dir_all(&staging_root)?;
    }
    // write_cache_file derives `<root>/.lazyspec/cache/<type>` from the root it
    // is handed, so staging just means handing it a staging root.
    let staged_type_dir = staging_root.join(".lazyspec/cache").join(&type_def.name);

    let write_result = (|| -> Result<()> {
        std::fs::create_dir_all(&staged_type_dir)?;
        for task in &tasks {
            let id = type_def.make_id(&task.id);
            let (meta, body) = task_to_doc(task, type_def, &id);
            write_cache_file(&staging_root, type_def, &meta, &body)?;
        }
        Ok(())
    })();
    if let Err(e) = write_result {
        let _ = std::fs::remove_dir_all(&staging_root);
        return Err(e);
    }

    // Swap: move the previous dir aside, promote the staged one, drop the old.
    // Only after this point does the task map reflect the fetch.
    let old_dir = cache_root.join(format!(".old-{}", type_def.name));
    if old_dir.exists() {
        std::fs::remove_dir_all(&old_dir)?;
    }
    if cache_dir.exists() {
        std::fs::rename(&cache_dir, &old_dir)?;
    }
    std::fs::rename(&staged_type_dir, &cache_dir)?;
    let _ = std::fs::remove_dir_all(&old_dir);
    let _ = std::fs::remove_dir_all(&staging_root);

    let mut new_count = 0usize;
    let mut fetched_ids = HashSet::new();

    for task in &tasks {
        let id = type_def.make_id(&task.id);

        let updated_at = task
            .date_updated
            .map(|ms| ms.to_string())
            .unwrap_or_default();
        task_map.insert(&id, &task.id, updated_at);

        if !previously_cached.contains(&id) {
            new_count += 1;
        }
        fetched_ids.insert(id);
    }

    let removed: Vec<String> = previously_cached
        .difference(&fetched_ids)
        .cloned()
        .collect();
    for id in &removed {
        task_map.remove(id);
    }

    Ok(ClickupFetchResult {
        fetched: tasks.len(),
        new: new_count,
        removed: removed.len(),
    })
}

/// Fetch the bound List's status workflow once and derive both the type's
/// effective [`Lifecycle`] (RFC-056 §Status handling) and the per-status colour
/// map from it. Called at sync time so both always reflect the live List --
/// a single `list_statuses` call, never a second round-trip for colours.
pub fn fetch_lifecycle_and_colors(
    client: &dyn ClickupClient,
    token: &str,
    list_id: &str,
) -> Result<(Lifecycle, HashMap<String, String>)> {
    let statuses = client
        .list_statuses(token, list_id)
        .with_context(|| format!("fetching ClickUp list statuses for list {}", list_id))?;
    Ok((derive_lifecycle(&statuses), derive_status_colors(&statuses)))
}

/// Derive a type's effective [`Lifecycle`] from a bound List's status set: the
/// states are the status names lowercased (the form task payloads carry, vs the
/// display casing `list_statuses` returns) in ClickUp workflow order (ascending
/// `orderindex`), and there are no edges. ClickUp enforces its own transition
/// rules, so lazyspec adds no local edges or gating -- the same empty-edge
/// posture the `ticket` type takes.
pub fn derive_lifecycle(statuses: &[ClickupStatus]) -> Lifecycle {
    let mut ordered: Vec<&ClickupStatus> = statuses.iter().collect();
    ordered.sort_by_key(|s| s.orderindex);
    Lifecycle {
        states: ordered
            .into_iter()
            .map(|s| s.status.to_lowercase())
            .collect(),
        edges: Vec::new(),
    }
}

/// Derive a status-name -> colour map from a bound List's status set. Keys are
/// lowercased to match the form task payloads carry, so the case-sensitive
/// colour lookup hits. A status with an empty colour is omitted, so a later
/// `get` miss lets the renderer fall back to its default rather than painting
/// with an empty string.
pub fn derive_status_colors(statuses: &[ClickupStatus]) -> HashMap<String, String> {
    statuses
        .iter()
        .filter(|s| !s.color.is_empty())
        .map(|s| (s.status.to_lowercase(), s.color.clone()))
        .collect()
}

/// The lazyspec doc ids currently cached for a type: the `.md` filename stems
/// under the type's flat cache dir. ClickUp tasks materialize flat (no native
/// parentage in this iteration), so a shallow scan is sufficient.
fn list_cached_ids(cache_dir: &Path) -> HashSet<String> {
    let mut ids = HashSet::new();
    if let Ok(entries) = std::fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                ids.insert(crate::engine::store::extract_id_from_name(stem));
            }
        }
    }
    ids
}

/// Map a ClickUp task to a lazyspec document: `(DocMeta, body)`.
///
/// `status` is the raw ClickUp status string verbatim. `priority`/`estimate`/
/// `due` become custom attributes read from ClickUp's native fields: `priority`
/// the priority name, `estimate` the `time_estimate` in ms, `due` the `due_date`
/// epoch ms. The body prefers `markdown_description`, falling back to
/// `text_content`.
///
/// Anything with no native ClickUp home is decoded off the task's custom fields
/// through the type's `clickup_custom_field_map` (RFC-056 §Field mapping): the
/// field mapped from the reserved [`CLICKUP_RELATIONS_FIELD`] key holds a
/// serialized relations block (`- implements: RFC-056`) whose entries become
/// `DocMeta.related` -- targets are lazyspec doc ids stored directly, so they
/// resolve identically to a filesystem doc's relations; every other mapped field
/// becomes a `DocMeta.attributes` entry under its configured name. Custom fields
/// the map does not name are ignored.
pub(crate) fn task_to_doc(task: &ClickupTask, type_def: &TypeDef, id: &str) -> (DocMeta, String) {
    let author = task
        .creator
        .as_ref()
        .filter(|c| !c.username.is_empty())
        .map(|c| format!("@{}", c.username))
        .unwrap_or_else(|| "clickup".to_string());

    let date = task
        .date_created
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map(|dt| dt.date_naive())
        .unwrap_or_else(|| Utc::now().date_naive());

    let mut attributes = std::collections::BTreeMap::new();
    if let Some(priority) = &task.priority {
        attributes.insert(
            "priority".to_string(),
            AttrValue::Str(priority.priority.clone()),
        );
    }
    if let Some(estimate) = task.time_estimate {
        attributes.insert("estimate".to_string(), AttrValue::Int(estimate));
    }
    if let Some(due) = task.due_date {
        attributes.insert("due".to_string(), AttrValue::Int(due));
    }

    let related = decode_custom_fields(task, type_def, &mut attributes);

    let body = if !task.markdown_description.trim().is_empty() {
        task.markdown_description.clone()
    } else {
        task.text_content.clone()
    };

    let meta = DocMeta {
        path: std::path::PathBuf::new(),
        title: task.name.clone(),
        doc_type: DocType::new(&type_def.name),
        status: Status::new(&task.status.status),
        author,
        date,
        tags: task.tags.iter().map(|t| t.name.clone()).collect(),
        provenance: vec![],
        related,
        validate_ignore: false,
        virtual_doc: false,
        // Inherit the task's native assignee (STORY-222 AC3): the first entry's
        // username (multi maps to first), `None` when unassigned. Remote is the
        // source of truth, so sync overwrites any local value.
        assignee: task.assignees.first().map(|a| a.username.clone()),
        id: id.to_string(),
        attributes,
    };

    (meta, body)
}

/// Decode a task's custom fields against the type's `clickup_custom_field_map`
/// (RFC-056 §Field mapping), the *read* direction of the resolver. Each custom
/// field whose uuid the map names is dispatched by its configured name: the
/// reserved [`CLICKUP_RELATIONS_FIELD`] name yields the decoded relations
/// (returned); any other name inserts a non-native attribute into `attributes`.
/// Unmapped fields and unparseable values are skipped -- a malformed relations
/// blob or an odd custom-field payload never fails the materialize.
fn decode_custom_fields(
    task: &ClickupTask,
    type_def: &TypeDef,
    attributes: &mut std::collections::BTreeMap<String, AttrValue>,
) -> Vec<Relation> {
    let mut related = Vec::new();
    for field in &task.custom_fields {
        let Some(name) = type_def.clickup_field_name(&field.id) else {
            continue;
        };
        let Some(value) = &field.value else { continue };
        if name == CLICKUP_RELATIONS_FIELD {
            if let Some(text) = value.as_str() {
                related.extend(decode_relations_block(text));
            }
        } else if let Some(attr) = json_value_to_attr(value) {
            attributes.insert(name.to_string(), attr);
        }
    }
    related
}

/// Parse a serialized relations block (the `issue_body.rs` YAML shape, a
/// sequence of single-key `- <rel-type>: <target>` mappings) into [`Relation`]s.
/// Reuses [`document::parse_relation`] so a ClickUp-decoded relation is shaped
/// identically to one read from a filesystem doc's frontmatter. A blank blob or
/// a parse failure yields no relations rather than an error.
fn decode_relations_block(text: &str) -> Vec<Relation> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let entries: Vec<serde_yaml::Value> = match serde_yaml::from_str(text) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    entries
        .iter()
        .filter_map(|entry| document::parse_relation(entry).ok())
        .collect()
}

/// Serialize a doc's relations into the YAML relations block the read path
/// ([`decode_relations_block`]) parses -- a sequence of single-key
/// `- <rel-type>: <target>` mappings, the same shape `issue_body::serialize`
/// embeds in a GitHub issue body. This is the *write* direction of the relation
/// round-trip (RFC-056 §Relations): the whole block replaces the configured text
/// custom field's value on every link/unlink (a full replace, no add/rem diff),
/// so an empty relation set serializes to the empty string -- which
/// [`decode_relations_block`] decodes back to no relations, closing the loop.
pub(crate) fn encode_relations_block(related: &[Relation]) -> String {
    related
        .iter()
        .map(|rel| format!("- {}: {}", rel.rel_type, rel.target))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Coerce a ClickUp custom-field JSON value into an [`AttrValue`] for a
/// non-native attribute. Scalars map to their typed variant; `null` yields
/// `None` (the attribute is absent); a composite value falls back to its JSON
/// text so the attribute still carries something rather than being dropped.
fn json_value_to_attr(value: &serde_json::Value) -> Option<AttrValue> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(s) => Some(AttrValue::Str(s.clone())),
        serde_json::Value::Bool(b) => Some(AttrValue::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(AttrValue::Int(i))
            } else {
                n.as_f64().map(AttrValue::Float)
            }
        }
        other => Some(AttrValue::Str(other.to_string())),
    }
}

/// Map a lazyspec doc's title, body, status and native attributes to a ClickUp
/// [`TaskCreate`] payload -- the *write* direction, asymmetric with the read
/// mapping in [`task_to_doc`] (RFC-056 §Field mapping):
///
/// - `name` <- title (always sent);
/// - `markdown_content` <- body, omitted when the body is blank so ClickUp does
///   not clobber a task with an empty description;
/// - `status` <- the raw status string, omitted (`None`) when the caller has no
///   status to push, so ClickUp assigns the List's default;
/// - `priority` <- the priority *name* mapped to ClickUp's bare integer
///   (`urgent=1 high=2 normal=3 low=4`); an unrecognized name is dropped;
/// - `due_date`/`time_estimate` <- the integer epoch-ms / duration-ms attributes.
/// - `custom_item_id` <- `task_type` (the type's configured `clickup_task_type`),
///   omitted when `None` so ClickUp defaults the task to the List's native "Task"
///   type; set, it stamps the new task with the bound custom task type (RFC-056).
///
/// A `create` has no attributes yet (its signature carries only title/author/
/// body), so it passes an empty map and only `name`/`markdown_content` (plus the
/// stamped `custom_item_id`, when set) are sent; the attribute mapping is
/// exercised by the write path directly.
pub(crate) fn build_task_create(
    title: &str,
    body: &str,
    status: Option<&str>,
    attributes: &std::collections::BTreeMap<String, AttrValue>,
    task_type: Option<i64>,
) -> TaskCreate {
    let priority = match attributes.get("priority") {
        Some(AttrValue::Str(name)) => priority_name_to_int(name),
        _ => None,
    };
    let due_date = match attributes.get("due") {
        Some(AttrValue::Int(ms)) => Some(*ms),
        _ => None,
    };
    let time_estimate = match attributes.get("estimate") {
        Some(AttrValue::Int(ms)) => Some(*ms),
        _ => None,
    };

    TaskCreate {
        name: title.to_string(),
        markdown_content: if body.trim().is_empty() {
            None
        } else {
            Some(body.to_string())
        },
        status: status.map(|s| s.to_string()),
        priority,
        due_date,
        start_date: None,
        time_estimate,
        custom_item_id: task_type,
    }
}

/// Map a lazyspec `update`'s `(key, value)` changes to a ClickUp [`TaskUpdate`]
/// payload -- the *edit* write direction (RFC-056 §Field mapping). Only the keys
/// with a ClickUp *native* home are mapped; a key absent from `updates` stays
/// `None` so the `PUT /task/{id}` leaves that field untouched (a partial edit,
/// never a blanket overwrite):
///
/// - `title` -> `name`;
/// - `body`  -> `markdown_content`;
/// - `status` -> `status`, the raw ClickUp status string pushed verbatim on an
///   `advance` (RFC-056 §Status handling). lazyspec derives no local transition
///   edges for a ClickUp-backed type, so the CLI does not gate the move; ClickUp
///   validates the target and rejects an illegal one;
/// - `priority` -> the priority *name* mapped to the bare integer
///   (`urgent=1 high=2 normal=3 low=4`); an unrecognized name drops the field;
/// - `due` -> `due_date`, `estimate` -> `time_estimate` (integer epoch-ms /
///   duration-ms; an unparseable value drops the field);
/// - `assignee` -> `assignees` add/rem delta of ClickUp user ids (STORY-222 AC4;
///   the value is a numeric user id -- username->id mapping is out of scope).
///
/// Any other key (a non-native attribute or a relation) has no native field and
/// routes to a custom field in a later RFC-056 story; it is ignored here.
pub(crate) fn build_task_update(updates: &[(&str, &str)]) -> TaskUpdate {
    let mut payload = TaskUpdate::default();
    for &(key, value) in updates {
        match key {
            "title" => payload.name = Some(value.to_string()),
            "body" => payload.markdown_content = Some(value.to_string()),
            "status" => payload.status = Some(value.to_string()),
            "priority" => payload.priority = priority_name_to_int(value),
            "due" => payload.due_date = value.trim().parse::<i64>().ok(),
            "estimate" => payload.time_estimate = value.trim().parse::<i64>().ok(),
            // Assignee maps to ClickUp's add/rem delta of user ids. Cross-identity
            // mapping (username -> id) is out of scope (STORY-222), so the value
            // must be a numeric ClickUp user id; a non-numeric value drops the
            // field rather than sending an invalid payload.
            "assignee" => {
                payload.assignees = value
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .map(|id| TaskAssigneeUpdate {
                        add: vec![id],
                        rem: vec![],
                    })
            }
            _ => {}
        }
    }
    payload
}

/// Map a ClickUp priority *name* to the bare integer the write API expects
/// (`urgent=1 high=2 normal=3 low=4`, case-insensitive). Returns `None` for an
/// unrecognized name so the caller omits the field rather than sending a guess.
fn priority_name_to_int(name: &str) -> Option<i64> {
    match name.trim().to_ascii_lowercase().as_str() {
        "urgent" => Some(1),
        "high" => Some(2),
        "normal" => Some(3),
        "low" => Some(4),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::clickup::FakeClickupClient;
    use crate::engine::config::{StoreBackend, TypeDef};
    use crate::engine::document::RelationType;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn clickup_type() -> TypeDef {
        let mut td = TypeDef::test_fixture("task", StoreBackend::ClickupTasks);
        td.prefix = "TASK".to_string();
        td.clickup_list_id = Some("list123".to_string());
        td
    }

    /// A ClickUp type whose custom-field map names a relations text field
    /// (uuid `uuid-rel`) and a non-native `owner` attribute (uuid `uuid-owner`).
    fn clickup_type_with_field_map() -> TypeDef {
        let mut td = clickup_type();
        let mut map = HashMap::new();
        map.insert(CLICKUP_RELATIONS_FIELD.to_string(), "uuid-rel".to_string());
        map.insert("owner".to_string(), "uuid-owner".to_string());
        td.clickup_custom_field_map = Some(map);
        td
    }

    fn task_from_json(json: &str) -> ClickupTask {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn materializes_task_to_cache_in_github_issues_shape() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type();

        let task = task_from_json(
            r#"{
                "id": "86abc",
                "name": "Wire the reader",
                "status": {"status": "in progress"},
                "priority": {"priority": "high"},
                "due_date": "1748541600000",
                "time_estimate": "3600000",
                "date_created": "1700000000000",
                "markdown_description": "the body",
                "creator": {"username": "Jack"},
                "tags": [{"name": "backend"}]
            }"#,
        );
        let client = FakeClickupClient::with_tasks(vec![task]);
        let mut task_map = TaskMap::load(root).unwrap();

        let result = fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();

        assert_eq!(result.fetched, 1);
        assert_eq!(result.new, 1);
        assert_eq!(result.removed, 0);

        let cache_file = root.join(".lazyspec/cache/task/TASK-86abc.md");
        let content = std::fs::read_to_string(&cache_file).unwrap();
        assert!(
            content.contains("title: Wire the reader"),
            "got:\n{content}"
        );
        // Raw ClickUp status verbatim, no mapping table.
        assert!(content.contains("status: in progress"), "got:\n{content}");
        assert!(content.contains("priority: high"), "got:\n{content}");
        assert!(content.contains("estimate: 3600000"), "got:\n{content}");
        assert!(content.contains("due: 1748541600000"), "got:\n{content}");
        assert!(content.contains("backend"), "got:\n{content}");
        assert!(content.contains("the body"), "got:\n{content}");

        // Round-trips through DocMeta::parse like any cache doc.
        let meta = DocMeta::parse(&content).unwrap();
        assert_eq!(meta.status.as_str(), "in progress");
    }

    #[test]
    fn task_map_records_task_id_and_updated_at() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type();

        let task = task_from_json(
            r#"{"id":"86abc","name":"n","status":{"status":"open"},"date_updated":"1774587145901"}"#,
        );
        let client = FakeClickupClient::with_tasks(vec![task]);
        let mut task_map = TaskMap::load(root).unwrap();

        fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();

        let entry = task_map.get("TASK-86abc").unwrap();
        assert_eq!(entry.task_id, "86abc");
        assert_eq!(entry.updated_at, "1774587145901");
    }

    #[test]
    fn removes_cache_and_map_entry_for_task_gone_from_list() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type();

        // First fetch: two tasks.
        let t1 = task_from_json(r#"{"id":"a","name":"A","status":{"status":"open"}}"#);
        let t2 = task_from_json(r#"{"id":"b","name":"B","status":{"status":"open"}}"#);
        let client = FakeClickupClient::with_tasks(vec![t1.clone(), t2]);
        let mut task_map = TaskMap::load(root).unwrap();
        fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();
        assert!(root.join(".lazyspec/cache/task/TASK-b.md").exists());

        // Second fetch: task B gone (archived / deleted upstream).
        let client = FakeClickupClient::with_tasks(vec![t1]);
        let result = fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();

        assert_eq!(result.fetched, 1);
        assert_eq!(result.new, 0);
        assert_eq!(result.removed, 1);
        assert!(root.join(".lazyspec/cache/task/TASK-a.md").exists());
        assert!(!root.join(".lazyspec/cache/task/TASK-b.md").exists());
        assert!(task_map.get("TASK-b").is_none());
        assert!(task_map.get("TASK-a").is_some());
    }

    // STORY-209 AC3: a fetch that fails partway through its writes (injected
    // here by making the cache root refuse new entries, so creating the staging
    // dir fails) leaves the previously cached docs and the task map intact.
    #[cfg(unix)]
    #[test]
    fn fetch_write_failure_leaves_previous_cache_intact() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type();

        // Seed a previous successful fetch.
        let old_task = task_from_json(
            r#"{"id":"a","name":"Old name","status":{"status":"open"},"date_updated":"111"}"#,
        );
        let client = FakeClickupClient::with_tasks(vec![old_task]);
        let mut task_map = TaskMap::load(root).unwrap();
        fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();
        let doc_path = root.join(".lazyspec/cache/task/TASK-a.md");
        let old_doc = std::fs::read_to_string(&doc_path).unwrap();

        // Inject the write failure.
        let cache_root = root.join(".lazyspec/cache");
        let mut perms = std::fs::metadata(&cache_root).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(&cache_root, perms).unwrap();

        let new_task = task_from_json(
            r#"{"id":"a","name":"New name","status":{"status":"open"},"date_updated":"222"}"#,
        );
        let client = FakeClickupClient::with_tasks(vec![new_task]);
        let result = fetch_tasks(root, &td, &client, "pk_x", &mut task_map);

        // Restore permissions before asserting so TempDir cleanup works.
        let mut perms = std::fs::metadata(&cache_root).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&cache_root, perms).unwrap();

        assert!(result.is_err(), "interrupted fetch must error");
        assert_eq!(
            std::fs::read_to_string(&doc_path).unwrap(),
            old_doc,
            "previous cache doc intact after failed fetch"
        );
        assert_eq!(
            task_map.get("TASK-a").unwrap().updated_at,
            "111",
            "task map not advanced by a failed fetch"
        );
    }

    #[test]
    fn fetch_keeps_only_tasks_matching_configured_task_type() {
        // A type bound to custom_item_id 1001 materializes only the tasks whose
        // custom_item_id matches; a task of another type drops out of the cache.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let mut td = clickup_type();
        td.clickup_task_type = Some(1001);

        let matching = task_from_json(
            r#"{"id":"a","name":"A","status":{"status":"open"},"custom_item_id":1001}"#,
        );
        let other = task_from_json(
            r#"{"id":"b","name":"B","status":{"status":"open"},"custom_item_id":2002}"#,
        );
        let client = FakeClickupClient::with_tasks(vec![matching, other]);
        let mut task_map = TaskMap::load(root).unwrap();

        let result = fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();

        assert_eq!(result.fetched, 1);
        assert_eq!(result.new, 1);
        assert!(root.join(".lazyspec/cache/task/TASK-a.md").exists());
        assert!(!root.join(".lazyspec/cache/task/TASK-b.md").exists());
        assert!(task_map.get("TASK-a").is_some());
        assert!(task_map.get("TASK-b").is_none());
    }

    #[test]
    fn fetch_without_task_type_materializes_all_tasks() {
        // Field unset: every task materializes regardless of custom_item_id (no
        // behavior change from before task-type filtering existed).
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type();
        assert!(td.clickup_task_type.is_none());

        let t1 = task_from_json(
            r#"{"id":"a","name":"A","status":{"status":"open"},"custom_item_id":1001}"#,
        );
        let t2 = task_from_json(
            r#"{"id":"b","name":"B","status":{"status":"open"},"custom_item_id":2002}"#,
        );
        let client = FakeClickupClient::with_tasks(vec![t1, t2]);
        let mut task_map = TaskMap::load(root).unwrap();

        let result = fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();

        assert_eq!(result.fetched, 2);
        assert!(root.join(".lazyspec/cache/task/TASK-a.md").exists());
        assert!(root.join(".lazyspec/cache/task/TASK-b.md").exists());
    }

    #[test]
    fn build_task_create_stamps_custom_item_id_when_task_type_set() {
        let payload = build_task_create("t", "b", None, &Default::default(), Some(1001));
        assert_eq!(payload.custom_item_id, Some(1001));
    }

    #[test]
    fn build_task_create_omits_custom_item_id_when_task_type_unset() {
        let payload = build_task_create("t", "b", None, &Default::default(), None);
        assert_eq!(payload.custom_item_id, None);
    }

    #[test]
    fn body_falls_back_to_text_content_when_markdown_empty() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type();

        let task = task_from_json(
            r#"{"id":"a","name":"A","status":{"status":"open"},"markdown_description":"","text_content":"plain body"}"#,
        );
        let client = FakeClickupClient::with_tasks(vec![task]);
        let mut task_map = TaskMap::load(root).unwrap();
        fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();

        let content = std::fs::read_to_string(root.join(".lazyspec/cache/task/TASK-a.md")).unwrap();
        assert!(content.contains("plain body"), "got:\n{content}");
    }

    #[test]
    fn errors_when_list_id_missing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let mut td = clickup_type();
        td.clickup_list_id = None;

        let client = FakeClickupClient::with_tasks(vec![]);
        let mut task_map = TaskMap::load(root).unwrap();
        let err = fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap_err();
        assert!(err.to_string().contains("clickup_list_id"), "got: {err}");
    }

    fn status(name: &str, orderindex: i64, ty: &str) -> ClickupStatus {
        colored_status(name, orderindex, ty, "")
    }

    fn colored_status(name: &str, orderindex: i64, ty: &str, color: &str) -> ClickupStatus {
        ClickupStatus {
            status: name.to_string(),
            orderindex,
            status_type: ty.to_string(),
            color: color.to_string(),
        }
    }

    #[test]
    fn derive_lifecycle_states_follow_orderindex_with_no_edges() {
        // Deliberately out of order to prove the derivation sorts by orderindex.
        let statuses = vec![
            status("done", 2, "closed"),
            status("to do", 0, "open"),
            status("in progress", 1, "custom"),
        ];
        let lifecycle = derive_lifecycle(&statuses);
        assert_eq!(lifecycle.states, vec!["to do", "in progress", "done"]);
        // No local edges or gating -- ClickUp owns the transition rules.
        assert!(lifecycle.edges.is_empty());
    }

    #[test]
    fn derive_lifecycle_lowercases_status_names() {
        // The list_statuses API returns display casing ("Closed") while task
        // payloads carry the lowercased form ("closed"); the lifecycle must use
        // the task-payload form so doc statuses match the derived states.
        let statuses = vec![status("Closed", 1, "closed"), status("To Do", 0, "open")];
        let lifecycle = derive_lifecycle(&statuses);
        assert_eq!(lifecycle.states, vec!["to do", "closed"]);
    }

    #[test]
    fn derive_lifecycle_empty_when_list_has_no_statuses() {
        let lifecycle = derive_lifecycle(&[]);
        assert!(lifecycle.states.is_empty());
        assert!(lifecycle.edges.is_empty());
    }

    #[test]
    fn derive_status_colors_maps_name_to_hex() {
        let statuses = vec![
            colored_status("to do", 0, "open", "#d3d3d3"),
            colored_status("in progress", 1, "custom", "#5f55ee"),
            colored_status("done", 2, "closed", "#008844"),
        ];
        let colors = derive_status_colors(&statuses);
        assert_eq!(colors.len(), 3);
        assert_eq!(colors.get("to do").map(String::as_str), Some("#d3d3d3"));
        assert_eq!(
            colors.get("in progress").map(String::as_str),
            Some("#5f55ee")
        );
        assert_eq!(colors.get("done").map(String::as_str), Some("#008844"));
    }

    #[test]
    fn derive_status_colors_lowercases_status_names() {
        // Colour keys must match the lowercased status in task payloads, or the
        // case-sensitive StatusColors lookup misses ("Closed" vs "closed").
        let statuses = vec![colored_status("Closed", 0, "closed", "#008844")];
        let colors = derive_status_colors(&statuses);
        assert_eq!(colors.get("closed").map(String::as_str), Some("#008844"));
        assert!(!colors.contains_key("Closed"));
    }

    #[test]
    fn derive_status_colors_skips_empty_color() {
        let statuses = vec![
            colored_status("open", 0, "open", "#d3d3d3"),
            status("no colour", 1, "custom"),
        ];
        let colors = derive_status_colors(&statuses);
        assert_eq!(colors.len(), 1);
        assert!(!colors.contains_key("no colour"));
    }

    #[test]
    fn fetch_lifecycle_and_colors_returns_both_from_one_fetch() {
        let client = FakeClickupClient::with_tasks(vec![]).with_statuses(vec![
            colored_status("open", 0, "open", "#d3d3d3"),
            colored_status("closed", 1, "closed", "#008844"),
        ]);
        let (lifecycle, colors) = fetch_lifecycle_and_colors(&client, "pk_x", "list123").unwrap();
        assert_eq!(lifecycle.states, vec!["open", "closed"]);
        assert!(lifecycle.edges.is_empty());
        assert_eq!(colors.get("open").map(String::as_str), Some("#d3d3d3"));
        assert_eq!(colors.get("closed").map(String::as_str), Some("#008844"));
    }

    #[test]
    fn fetch_lifecycle_and_colors_propagates_client_error() {
        use crate::engine::clickup::ClickupError;
        let client = FakeClickupClient::with_tasks(vec![])
            .failing_statuses(ClickupError::InvalidToken { status: 401 });
        let err = fetch_lifecycle_and_colors(&client, "pk_x", "list123").unwrap_err();
        assert!(
            err.to_string().contains("fetching ClickUp list statuses"),
            "got: {err}"
        );
    }

    #[test]
    fn build_task_create_maps_title_and_body_only_for_a_bare_create() {
        // A create carries no attributes and no status; only name + body are sent.
        let payload = build_task_create("My task", "the body", None, &Default::default(), None);
        assert_eq!(payload.name, "My task");
        assert_eq!(payload.markdown_content, Some("the body".to_string()));
        assert_eq!(payload.status, None);
        assert_eq!(payload.priority, None);
        assert_eq!(payload.due_date, None);
        assert_eq!(payload.time_estimate, None);
    }

    #[test]
    fn build_task_create_omits_markdown_content_when_body_blank() {
        let payload = build_task_create("t", "   ", None, &Default::default(), None);
        assert_eq!(payload.markdown_content, None);
    }

    #[test]
    fn build_task_create_maps_native_attributes_to_write_shape() {
        // The asymmetric write mapping: priority name -> bare int, epoch/duration
        // attributes -> integer fields, raw status string passed through.
        let mut attrs = std::collections::BTreeMap::new();
        attrs.insert("priority".to_string(), AttrValue::Str("high".to_string()));
        attrs.insert("due".to_string(), AttrValue::Int(1_748_541_600_000));
        attrs.insert("estimate".to_string(), AttrValue::Int(3_600_000));

        let payload = build_task_create("t", "b", Some("in progress"), &attrs, None);
        assert_eq!(payload.status, Some("in progress".to_string()));
        assert_eq!(payload.priority, Some(2));
        assert_eq!(payload.due_date, Some(1_748_541_600_000));
        assert_eq!(payload.time_estimate, Some(3_600_000));
    }

    #[test]
    fn build_task_update_maps_native_field_changes_to_partial_edit() {
        // The edit write mapping: title/body/native attrs to the partial shape.
        let updates = [
            ("title", "New title"),
            ("body", "new body"),
            ("priority", "high"),
            ("due", "1748541600000"),
            ("estimate", "3600000"),
        ];
        let payload = build_task_update(&updates);
        assert_eq!(payload.name, Some("New title".to_string()));
        assert_eq!(payload.markdown_content, Some("new body".to_string()));
        assert_eq!(payload.priority, Some(2));
        assert_eq!(payload.due_date, Some(1_748_541_600_000));
        assert_eq!(payload.time_estimate, Some(3_600_000));
        assert_eq!(payload.start_date, None);
    }

    #[test]
    fn build_task_update_omits_untouched_fields() {
        // Only priority changed: every other field stays None so the PUT leaves
        // the task's name/body/due/estimate untouched.
        let payload = build_task_update(&[("priority", "urgent")]);
        assert_eq!(payload.priority, Some(1));
        assert_eq!(payload.name, None);
        assert_eq!(payload.markdown_content, None);
        assert_eq!(payload.due_date, None);
        assert_eq!(payload.time_estimate, None);
    }

    #[test]
    fn build_task_update_drops_unrecognized_priority_and_unparseable_numbers() {
        let payload = build_task_update(&[
            ("priority", "bogus"),
            ("due", "not-a-number"),
            ("estimate", "also-bad"),
        ]);
        assert_eq!(payload.priority, None);
        assert_eq!(payload.due_date, None);
        assert_eq!(payload.time_estimate, None);
    }

    #[test]
    fn build_task_update_maps_status_verbatim() {
        // Advance pushes the raw ClickUp status string through the edit payload;
        // lazyspec does not gate or map it (RFC-056 §Status handling).
        let payload = build_task_update(&[("status", "in progress")]);
        assert_eq!(payload.status, Some("in progress".to_string()));
        // No other field is touched by a status-only advance.
        assert_eq!(payload.name, None);
        assert_eq!(payload.markdown_content, None);
        assert_eq!(payload.priority, None);
    }

    // STORY-222 AC3: a task's native assignee is inherited into
    // `DocMeta.assignee` (first entry's username when multiple); an unassigned
    // task yields None.
    #[test]
    fn task_to_doc_inherits_first_assignee_username() {
        let td = clickup_type();
        let task = task_from_json(
            r#"{
                "id": "1",
                "name": "T",
                "status": {"status": "open"},
                "assignees": [{"username": "alice"}, {"username": "dave"}]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert_eq!(meta.assignee, Some("alice".to_string()));
    }

    #[test]
    fn task_to_doc_unassigned_task_yields_none() {
        let td = clickup_type();
        let task = task_from_json(r#"{"id":"1","name":"T","status":{"status":"open"}}"#);
        assert!(task.assignees.is_empty());
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert_eq!(meta.assignee, None);
    }

    // STORY-222 AC4: an assignee update maps to ClickUp's `assignees {add, rem}`
    // delta of user ids (the value is a numeric ClickUp user id).
    #[test]
    fn build_task_update_maps_assignee_to_add_rem_user_id() {
        let payload = build_task_update(&[("assignee", "183")]);
        assert_eq!(
            payload.assignees,
            Some(crate::engine::clickup::TaskAssigneeUpdate {
                add: vec![183],
                rem: vec![],
            })
        );
        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["assignees"]["add"], serde_json::json!([183]));
        assert_eq!(json["assignees"]["rem"], serde_json::json!([]));
    }

    #[test]
    fn build_task_update_drops_non_numeric_assignee() {
        // Username -> id mapping is out of scope; a non-numeric value drops the
        // field rather than sending an invalid payload.
        let payload = build_task_update(&[("assignee", "alice")]);
        assert_eq!(payload.assignees, None);
    }

    #[test]
    fn build_task_update_ignores_non_native_keys() {
        // A non-native attr routes to a custom field in a later story; it has no
        // native field here.
        let payload = build_task_update(&[("owner", "jkaloger")]);
        assert_eq!(payload, TaskUpdate::default());
    }

    #[test]
    fn build_task_create_drops_unrecognized_priority_name() {
        let mut attrs = std::collections::BTreeMap::new();
        attrs.insert("priority".to_string(), AttrValue::Str("bogus".to_string()));
        let payload = build_task_create("t", "b", None, &attrs, None);
        assert_eq!(payload.priority, None);
    }

    #[test]
    fn propagates_client_error() {
        use crate::engine::clickup::ClickupError;
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type();

        let client = FakeClickupClient::failing_tasks(ClickupError::InvalidToken { status: 401 });
        let mut task_map = TaskMap::load(root).unwrap();
        let err = fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap_err();
        assert!(
            err.to_string().contains("fetching ClickUp tasks"),
            "got: {err}"
        );
    }

    // --- ITERATION-275: decode custom-field relations + non-native attrs ---

    #[test]
    fn resolver_resolves_field_map_by_name_and_by_id() {
        // AC: clickup_custom_field_map resolves a relation name and an attr name
        // both by name (name -> uuid, write direction) and by id (uuid -> name,
        // decode direction).
        let td = clickup_type_with_field_map();
        // Forward: name -> uuid.
        assert_eq!(
            td.clickup_field_id(CLICKUP_RELATIONS_FIELD),
            Some("uuid-rel")
        );
        assert_eq!(td.clickup_field_id("owner"), Some("uuid-owner"));
        assert_eq!(td.clickup_field_id("unmapped"), None);
        // Reverse: uuid -> name.
        assert_eq!(
            td.clickup_field_name("uuid-rel"),
            Some(CLICKUP_RELATIONS_FIELD)
        );
        assert_eq!(td.clickup_field_name("uuid-owner"), Some("owner"));
        assert_eq!(td.clickup_field_name("uuid-nope"), None);
    }

    #[test]
    fn resolver_returns_none_when_no_field_map_configured() {
        let td = clickup_type();
        assert_eq!(td.clickup_field_id(CLICKUP_RELATIONS_FIELD), None);
        assert_eq!(td.clickup_field_name("uuid-rel"), None);
    }

    #[test]
    fn decodes_relation_from_configured_text_field_into_docmeta() {
        // AC: a ClickUp doc whose configured relation custom field holds
        // `implements: RFC-056` yields a related entry {implements, RFC-056} in
        // the same shape a filesystem doc's relation has.
        let td = clickup_type_with_field_map();
        let task = task_from_json(
            r#"{
                "id": "86abc",
                "name": "T",
                "status": {"status": "open"},
                "custom_fields": [
                    {"id": "uuid-rel", "name": "relations", "value": "- implements: RFC-056"}
                ]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-86abc");
        assert_eq!(meta.related.len(), 1);
        assert_eq!(meta.related[0].rel_type, RelationType::new("implements"));
        assert_eq!(meta.related[0].target, "RFC-056");
    }

    #[test]
    fn decodes_multiple_relations_from_block() {
        let td = clickup_type_with_field_map();
        let task = task_from_json(
            r#"{
                "id": "1",
                "name": "T",
                "status": {"status": "open"},
                "custom_fields": [
                    {"id": "uuid-rel", "name": "relations", "value": "- implements: RFC-056\n- blocks: RFC-010"}
                ]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert_eq!(meta.related.len(), 2);
        assert_eq!(meta.related[0].rel_type, RelationType::new("implements"));
        assert_eq!(meta.related[0].target, "RFC-056");
        assert_eq!(meta.related[1].rel_type, RelationType::new("blocks"));
        assert_eq!(meta.related[1].target, "RFC-010");
    }

    #[test]
    fn decodes_non_native_attr_by_configured_name() {
        // A custom field the map names (not the relations key) becomes a
        // non-native attribute under its configured name.
        let td = clickup_type_with_field_map();
        let task = task_from_json(
            r#"{
                "id": "1",
                "name": "T",
                "status": {"status": "open"},
                "custom_fields": [
                    {"id": "uuid-owner", "name": "owner", "value": "jkaloger"}
                ]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert_eq!(
            meta.attributes.get("owner"),
            Some(&AttrValue::Str("jkaloger".to_string()))
        );
        // The relations field is absent, so no relations are decoded.
        assert!(meta.related.is_empty());
    }

    #[test]
    fn unmapped_custom_field_is_ignored() {
        let td = clickup_type_with_field_map();
        let task = task_from_json(
            r#"{
                "id": "1",
                "name": "T",
                "status": {"status": "open"},
                "custom_fields": [
                    {"id": "uuid-unknown", "name": "sprint", "value": "S1"}
                ]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert!(meta.related.is_empty());
        assert!(!meta.attributes.contains_key("sprint"));
    }

    #[test]
    fn no_field_map_leaves_relations_and_custom_attrs_empty() {
        // Without a configured map, custom fields are inert -- the read path (276)
        // is not regressed.
        let td = clickup_type();
        let task = task_from_json(
            r#"{
                "id": "1",
                "name": "T",
                "status": {"status": "open"},
                "custom_fields": [
                    {"id": "uuid-rel", "name": "relations", "value": "- implements: RFC-056"}
                ]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert!(meta.related.is_empty());
    }

    #[test]
    fn malformed_relations_blob_yields_no_relations() {
        // A garbled field value must not fail the materialize; it decodes to no
        // relations.
        let td = clickup_type_with_field_map();
        let task = task_from_json(
            r#"{
                "id": "1",
                "name": "T",
                "status": {"status": "open"},
                "custom_fields": [
                    {"id": "uuid-rel", "name": "relations", "value": "not: a: valid: block"}
                ]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert!(meta.related.is_empty());
    }

    #[test]
    fn null_relation_field_value_yields_no_relations() {
        let td = clickup_type_with_field_map();
        let task = task_from_json(
            r#"{
                "id": "1",
                "name": "T",
                "status": {"status": "open"},
                "custom_fields": [{"id": "uuid-rel", "name": "relations", "value": null}]
            }"#,
        );
        let (meta, _) = task_to_doc(&task, &td, "TASK-1");
        assert!(meta.related.is_empty());
    }

    #[test]
    fn fetch_materializes_relations_into_cache_frontmatter() {
        // End-to-end: the decoded relations land in the cache doc's `related:`
        // block and round-trip through DocMeta::parse the same as a filesystem
        // doc -- so `context --json` resolves them identically.
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = clickup_type_with_field_map();

        let task = task_from_json(
            r#"{
                "id": "86abc",
                "name": "Wire the reader",
                "status": {"status": "open"},
                "custom_fields": [
                    {"id": "uuid-rel", "name": "relations", "value": "- implements: RFC-056"},
                    {"id": "uuid-owner", "name": "owner", "value": "jkaloger"}
                ]
            }"#,
        );
        let client = FakeClickupClient::with_tasks(vec![task]);
        let mut task_map = TaskMap::load(root).unwrap();
        fetch_tasks(root, &td, &client, "pk_x", &mut task_map).unwrap();

        let content =
            std::fs::read_to_string(root.join(".lazyspec/cache/task/TASK-86abc.md")).unwrap();
        assert!(content.contains("related:"), "got:\n{content}");
        assert!(content.contains("implements: RFC-056"), "got:\n{content}");
        assert!(content.contains("owner: jkaloger"), "got:\n{content}");

        let meta = DocMeta::parse(&content).unwrap();
        assert_eq!(meta.related.len(), 1);
        assert_eq!(meta.related[0].rel_type, RelationType::new("implements"));
        assert_eq!(meta.related[0].target, "RFC-056");
    }

    // --- ITERATION-278: encode relations block (write direction) ---

    fn rel(rel_type: &str, target: &str) -> Relation {
        Relation {
            rel_type: RelationType::new(rel_type),
            target: target.to_string(),
        }
    }

    #[test]
    fn encode_relations_block_emits_single_key_mapping_lines() {
        let block =
            encode_relations_block(&[rel("implements", "RFC-056"), rel("blocks", "RFC-010")]);
        assert_eq!(block, "- implements: RFC-056\n- blocks: RFC-010");
    }

    #[test]
    fn encode_empty_relations_yields_empty_string() {
        assert_eq!(encode_relations_block(&[]), "");
    }

    #[test]
    fn encode_then_decode_round_trips_relations() {
        // ROUND-TRIP AC: relations serialized by the 278 write direction decode
        // back to the same set via the 275 read direction (decode_relations_block).
        let original = vec![
            rel("implements", "RFC-056"),
            rel("blocks", "RFC-010"),
            rel("relates-to", "STORY-200"),
        ];
        let block = encode_relations_block(&original);
        let decoded = decode_relations_block(&block);
        assert_eq!(decoded, original);
    }

    #[test]
    fn encode_then_decode_empty_round_trips_to_no_relations() {
        let block = encode_relations_block(&[]);
        assert!(decode_relations_block(&block).is_empty());
    }
}
