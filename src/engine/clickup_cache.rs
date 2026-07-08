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

use crate::engine::clickup::{ClickupClient, ClickupTask};
use crate::engine::config::TypeDef;
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
fn task_to_doc(task: &ClickupTask, type_def: &TypeDef, id: &str) -> (DocMeta, String) {
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
