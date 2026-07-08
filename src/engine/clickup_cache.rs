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

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::engine::clickup::{ClickupClient, ClickupStatus, ClickupTask, TaskCreate, TaskUpdate};
use crate::engine::config::{Lifecycle, TypeDef};
use crate::engine::document::{AttrValue, DocMeta, DocType, Status};
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

    let cache_dir = root.join(".lazyspec/cache").join(&type_def.name);
    let previously_cached: HashSet<String> = list_cached_ids(&cache_dir);

    // A full fetch owns the whole type directory; wipe it so a task removed from
    // the List leaves no stale cache file behind.
    if cache_dir.exists() {
        for entry in std::fs::read_dir(&cache_dir)?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                std::fs::remove_dir_all(&path)?;
            } else {
                std::fs::remove_file(&path)?;
            }
        }
    }

    let mut new_count = 0usize;
    let mut fetched_ids = HashSet::new();

    for task in &tasks {
        let id = type_def.make_id(&task.id);
        let (meta, body) = task_to_doc(task, type_def, &id);
        write_cache_file(root, type_def, &meta, &body)?;

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

/// Fetch the bound List's status workflow and derive the type's effective
/// [`Lifecycle`] from it (RFC-056 §Status handling). Called at sync time so the
/// lifecycle always reflects the live List, never a hardcoded config value.
pub fn fetch_lifecycle(
    client: &dyn ClickupClient,
    token: &str,
    list_id: &str,
) -> Result<Lifecycle> {
    let statuses = client
        .list_statuses(token, list_id)
        .with_context(|| format!("fetching ClickUp list statuses for list {}", list_id))?;
    Ok(derive_lifecycle(&statuses))
}

/// Derive a type's effective [`Lifecycle`] from a bound List's status set: the
/// states are the status names in ClickUp workflow order (ascending
/// `orderindex`), and there are no edges. ClickUp enforces its own transition
/// rules, so lazyspec adds no local edges or gating -- the same empty-edge
/// posture the `ticket` type takes.
pub fn derive_lifecycle(statuses: &[ClickupStatus]) -> Lifecycle {
    let mut ordered: Vec<&ClickupStatus> = statuses.iter().collect();
    ordered.sort_by_key(|s| s.orderindex);
    Lifecycle {
        states: ordered.into_iter().map(|s| s.status.clone()).collect(),
        edges: Vec::new(),
    }
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
        related: vec![],
        validate_ignore: false,
        virtual_doc: false,
        id: id.to_string(),
        attributes,
    };

    (meta, body)
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
///
/// A `create` has no attributes yet (its signature carries only title/author/
/// body), so it passes an empty map and only `name`/`markdown_content` are sent;
/// the attribute mapping is exercised by the write path directly.
pub(crate) fn build_task_create(
    title: &str,
    body: &str,
    status: Option<&str>,
    attributes: &std::collections::BTreeMap<String, AttrValue>,
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
/// - `priority` -> the priority *name* mapped to the bare integer
///   (`urgent=1 high=2 normal=3 low=4`); an unrecognized name drops the field;
/// - `due` -> `due_date`, `estimate` -> `time_estimate` (integer epoch-ms /
///   duration-ms; an unparseable value drops the field).
///
/// `status` is deliberately *not* mapped here: a ClickUp status change is pushed
/// via `advance` (ITERATION-272), and the derived lifecycle carries no edges so
/// the CLI rejects a `--status` update before it reaches this mapper. Any other
/// key (a non-native attribute or a relation) has no native field and routes to
/// a custom field in a later RFC-056 story; it is ignored here.
pub(crate) fn build_task_update(updates: &[(&str, &str)]) -> TaskUpdate {
    let mut payload = TaskUpdate::default();
    for &(key, value) in updates {
        match key {
            "title" => payload.name = Some(value.to_string()),
            "body" => payload.markdown_content = Some(value.to_string()),
            "priority" => payload.priority = priority_name_to_int(value),
            "due" => payload.due_date = value.trim().parse::<i64>().ok(),
            "estimate" => payload.time_estimate = value.trim().parse::<i64>().ok(),
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
    use tempfile::TempDir;

    fn clickup_type() -> TypeDef {
        let mut td = TypeDef::test_fixture("task", StoreBackend::ClickupTasks);
        td.prefix = "TASK".to_string();
        td.clickup_list_id = Some("list123".to_string());
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
        ClickupStatus {
            status: name.to_string(),
            orderindex,
            status_type: ty.to_string(),
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
    fn derive_lifecycle_empty_when_list_has_no_statuses() {
        let lifecycle = derive_lifecycle(&[]);
        assert!(lifecycle.states.is_empty());
        assert!(lifecycle.edges.is_empty());
    }

    #[test]
    fn fetch_lifecycle_derives_from_client_status_set() {
        let client = FakeClickupClient::with_tasks(vec![]).with_statuses(vec![
            status("open", 0, "open"),
            status("closed", 1, "closed"),
        ]);
        let lifecycle = fetch_lifecycle(&client, "pk_x", "list123").unwrap();
        assert_eq!(lifecycle.states, vec!["open", "closed"]);
        assert!(lifecycle.edges.is_empty());
    }

    #[test]
    fn fetch_lifecycle_propagates_client_error() {
        use crate::engine::clickup::ClickupError;
        let client = FakeClickupClient::with_tasks(vec![])
            .failing_statuses(ClickupError::InvalidToken { status: 401 });
        let err = fetch_lifecycle(&client, "pk_x", "list123").unwrap_err();
        assert!(
            err.to_string().contains("fetching ClickUp list statuses"),
            "got: {err}"
        );
    }

    #[test]
    fn build_task_create_maps_title_and_body_only_for_a_bare_create() {
        // A create carries no attributes and no status; only name + body are sent.
        let payload = build_task_create("My task", "the body", None, &Default::default());
        assert_eq!(payload.name, "My task");
        assert_eq!(payload.markdown_content, Some("the body".to_string()));
        assert_eq!(payload.status, None);
        assert_eq!(payload.priority, None);
        assert_eq!(payload.due_date, None);
        assert_eq!(payload.time_estimate, None);
    }

    #[test]
    fn build_task_create_omits_markdown_content_when_body_blank() {
        let payload = build_task_create("t", "   ", None, &Default::default());
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

        let payload = build_task_create("t", "b", Some("in progress"), &attrs);
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
    fn build_task_update_ignores_status_and_non_native_keys() {
        // Status is pushed via advance (272); a non-native attr routes to a
        // custom field in a later story. Neither has a native field here.
        let payload = build_task_update(&[("status", "in progress"), ("owner", "jkaloger")]);
        assert_eq!(payload, TaskUpdate::default());
    }

    #[test]
    fn build_task_create_drops_unrecognized_priority_name() {
        let mut attrs = std::collections::BTreeMap::new();
        attrs.insert("priority".to_string(), AttrValue::Str("bogus".to_string()));
        let payload = build_task_create("t", "b", None, &attrs);
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
}
