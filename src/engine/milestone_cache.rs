use std::path::Path;

use anyhow::Result;

use crate::engine::config::TypeDef;
use crate::engine::document::{AttrValue, DocMeta, DocType};
use crate::engine::gh::GhMilestoneApi;
use crate::engine::issue_cache::{FetchResult, IssueCache, RefreshWarning};
use crate::engine::issue_map::{EntryKind, IssueMap};
use crate::engine::store_dispatch::{milestone_state_to_status, write_cache_file};

/// Fetch all milestones for a `github-milestones` type and materialize them as
/// cache documents, mapping REST `state` to a lifecycle status. The milestone
/// number is the document id (`make_id(number)`), mirroring github-issues.
pub fn fetch_milestones(
    root: &Path,
    type_def: &TypeDef,
    gh: &impl GhMilestoneApi,
    repo: &str,
    issue_map: &mut IssueMap,
) -> Result<FetchResult> {
    let milestones = gh.milestone_list(repo)?;

    let cache = IssueCache::new(root);
    let previously: std::collections::HashSet<String> =
        cache.list_cached(&type_def.name).into_iter().collect();
    let mut fetched_ids = std::collections::HashSet::new();
    let mut new_count = 0usize;

    for m in &milestones {
        let id = type_def.make_id(m.number);
        let mut attributes: std::collections::BTreeMap<String, AttrValue> = Default::default();
        if let Some(due) = &m.due_on {
            attributes.insert("due_on".to_string(), AttrValue::Str(due.clone()));
        }
        attributes.insert(
            "open_issues".to_string(),
            AttrValue::Int(m.open_issues as i64),
        );
        attributes.insert(
            "closed_issues".to_string(),
            AttrValue::Int(m.closed_issues as i64),
        );
        let meta = DocMeta {
            path: std::path::PathBuf::new(),
            title: m.title.clone(),
            doc_type: DocType::new(&type_def.name),
            status: milestone_state_to_status(&m.state),
            author: "github".to_string(),
            date: chrono::Utc::now().date_naive(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            attributes,
            id: id.clone(),
        };

        if !previously.contains(&id) {
            new_count += 1;
        }
        write_cache_file(root, type_def, &meta, &m.description)?;
        cache.touch_lock(&id)?;
        issue_map.insert_kind(&id, m.number, "", "", EntryKind::Milestone);
        fetched_ids.insert(id);
    }

    let removed: Vec<String> = previously.difference(&fetched_ids).cloned().collect();
    for id in &removed {
        cache.remove(id, &type_def.name)?;
        issue_map.remove(id);
    }

    Ok(FetchResult {
        fetched: milestones.len(),
        new: new_count,
        removed: removed.len(),
        warnings: Vec::<RefreshWarning>::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{NumberingStrategy, StoreBackend, TypeDef};
    use crate::engine::gh::test_support::MockGhMilestoneClient;
    use crate::engine::gh::GhMilestone;
    use tempfile::TempDir;

    fn milestone_type_def() -> TypeDef {
        TypeDef {
            name: "milestone".to_string(),
            plural: "milestones".to_string(),
            dir: "docs/milestones".to_string(),
            prefix: "MILESTONE".to_string(),
            icon: None,
            numbering: NumberingStrategy::Incremental,
            subdirectory: false,
            store: StoreBackend::GithubMilestones,
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
        }
    }

    // AC1/AC3/AC6: fetch_milestones writes a cache doc per milestone with the
    // state mapped to a lifecycle status and counts stored, all via the mock seam.
    #[test]
    fn fetch_milestones_writes_cache_docs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let td = milestone_type_def();

        let gh = MockGhMilestoneClient::with_milestones(vec![
            GhMilestone {
                number: 3,
                title: "v1.0".to_string(),
                description: "first".to_string(),
                due_on: Some("2026-09-01T00:00:00Z".to_string()),
                state: "open".to_string(),
                open_issues: 7,
                closed_issues: 3,
                url: String::new(),
            },
            GhMilestone {
                number: 4,
                title: "v2.0".to_string(),
                description: "second".to_string(),
                due_on: None,
                state: "closed".to_string(),
                open_issues: 0,
                closed_issues: 5,
                url: String::new(),
            },
        ]);

        let mut issue_map = IssueMap::load(root).unwrap();
        let result = fetch_milestones(root, &td, &gh, "owner/repo", &mut issue_map).unwrap();

        assert_eq!(result.fetched, 2);
        assert_eq!(result.new, 2);

        let cache_dir = root.join(".lazyspec/cache/milestone");
        let open = std::fs::read_to_string(cache_dir.join("MILESTONE-3.md")).unwrap();
        assert!(open.contains("status: in-progress"), "{open}");
        assert!(open.contains("open_issues: 7"), "{open}");
        let closed = std::fs::read_to_string(cache_dir.join("MILESTONE-4.md")).unwrap();
        assert!(closed.contains("status: complete"), "{closed}");

        assert_eq!(issue_map.get("MILESTONE-3").unwrap().issue_number, 3);
        assert_eq!(issue_map.get("MILESTONE-4").unwrap().issue_number, 4);
    }
}
