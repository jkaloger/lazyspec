use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::gh::{GhGraphql, GqlVar};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueTypeId {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFieldId {
    pub project_number: u64,
    pub field_name: String,
    pub id: String,
    pub data_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionId {
    pub field_id: String,
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationId {
    pub field_id: String,
    pub title: String,
    pub id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GhSchemaSnapshot {
    #[serde(default)]
    pub issue_types: Vec<IssueTypeId>,
    #[serde(default)]
    pub project_fields: Vec<ProjectFieldId>,
    #[serde(default)]
    pub single_select_options: Vec<OptionId>,
    #[serde(default)]
    pub iterations: Vec<IterationId>,
    #[serde(default)]
    pub fetched_at: String,
}

pub fn snapshot_path(root: &Path) -> PathBuf {
    root.join(".lazyspec").join("cache").join("gh-schema.json")
}

impl GhSchemaSnapshot {
    /// Offline read. Missing file or parse failure yields `Default` — no network.
    pub fn load(root: &Path) -> GhSchemaSnapshot {
        let Ok(contents) = fs::read_to_string(snapshot_path(root)) else {
            return GhSchemaSnapshot::default();
        };
        serde_json::from_str(&contents).unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> Result<()> {
        let path = snapshot_path(root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating cache dir {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    pub fn issue_type_id(&self, name: &str) -> Option<&str> {
        self.issue_types
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.id.as_str())
    }

    pub fn field_id(&self, project_number: u64, field_name: &str) -> Option<&str> {
        self.project_fields
            .iter()
            .find(|f| f.project_number == project_number && f.field_name == field_name)
            .map(|f| f.id.as_str())
    }

    pub fn option_id(&self, field_id: &str, name: &str) -> Option<&str> {
        self.single_select_options
            .iter()
            .find(|o| o.field_id == field_id && o.name == name)
            .map(|o| o.id.as_str())
    }

    pub fn iteration_id(&self, field_id: &str, title: &str) -> Option<&str> {
        self.iterations
            .iter()
            .find(|i| i.field_id == field_id && i.title == title)
            .map(|i| i.id.as_str())
    }
}

const ISSUE_TYPES_QUERY: &str = "query($owner: String!) { organization(login: $owner) { issueTypes(first: 50) { nodes { id name } } } }";

const PROJECT_FIELDS_QUERY: &str = "query($owner: String!, $number: Int!) { organization(login: $owner) { projectV2(number: $number) { fields(first: 50) { nodes { __typename ... on ProjectV2FieldCommon { id name dataType } ... on ProjectV2SingleSelectField { id name dataType options { id name } } ... on ProjectV2IterationField { id name dataType configuration { iterations { id title } } } } } } } }";

/// Best-effort fetch of native field ids for `owner/name`. Currently fetches
/// org-level issue types; project field queries are keyed off a project number
/// once a project is configured.
pub fn fetch_snapshot(gh: &dyn GhGraphql, repo: &str) -> Result<GhSchemaSnapshot> {
    let (owner, _name) = split_repo(repo)?;

    let mut snapshot = GhSchemaSnapshot {
        fetched_at: Utc::now().to_rfc3339(),
        ..Default::default()
    };

    let issue_types_resp = gh.graphql(
        ISSUE_TYPES_QUERY,
        &[("owner", GqlVar::Str(owner.to_string()))],
    )?;
    snapshot.issue_types = parse_issue_types(&issue_types_resp);

    Ok(snapshot)
}

/// Fetch project field/option/iteration ids for a specific project number.
pub fn fetch_project_fields(
    gh: &dyn GhGraphql,
    repo: &str,
    project_number: u64,
) -> Result<(Vec<ProjectFieldId>, Vec<OptionId>, Vec<IterationId>)> {
    let (owner, _name) = split_repo(repo)?;
    let resp = gh.graphql(
        PROJECT_FIELDS_QUERY,
        &[
            ("owner", GqlVar::Str(owner.to_string())),
            ("number", GqlVar::Int(project_number as i64)),
        ],
    )?;
    Ok(parse_project_fields(&resp, project_number))
}

fn split_repo(repo: &str) -> Result<(&str, &str)> {
    repo.split_once('/')
        .filter(|(o, n)| !o.is_empty() && !n.is_empty())
        .with_context(|| format!("repo '{}' must be in owner/name form", repo))
}

fn parse_issue_types(resp: &serde_json::Value) -> Vec<IssueTypeId> {
    resp.pointer("/data/organization/issueTypes/nodes")
        .and_then(|n| n.as_array())
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| {
                    Some(IssueTypeId {
                        name: node.get("name")?.as_str()?.to_string(),
                        id: node.get("id")?.as_str()?.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_project_fields(
    resp: &serde_json::Value,
    project_number: u64,
) -> (Vec<ProjectFieldId>, Vec<OptionId>, Vec<IterationId>) {
    let mut fields = Vec::new();
    let mut options = Vec::new();
    let mut iterations = Vec::new();

    let Some(nodes) = resp
        .pointer("/data/organization/projectV2/fields/nodes")
        .and_then(|n| n.as_array())
    else {
        return (fields, options, iterations);
    };

    for node in nodes {
        let (Some(id), Some(name)) = (
            node.get("id").and_then(|v| v.as_str()),
            node.get("name").and_then(|v| v.as_str()),
        ) else {
            continue;
        };
        let data_type = node
            .get("dataType")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        fields.push(ProjectFieldId {
            project_number,
            field_name: name.to_string(),
            id: id.to_string(),
            data_type,
        });

        if let Some(opts) = node.get("options").and_then(|v| v.as_array()) {
            for opt in opts {
                if let (Some(oid), Some(oname)) = (
                    opt.get("id").and_then(|v| v.as_str()),
                    opt.get("name").and_then(|v| v.as_str()),
                ) {
                    options.push(OptionId {
                        field_id: id.to_string(),
                        name: oname.to_string(),
                        id: oid.to_string(),
                    });
                }
            }
        }

        if let Some(iters) = node
            .pointer("/configuration/iterations")
            .and_then(|v| v.as_array())
        {
            for it in iters {
                if let (Some(iid), Some(title)) = (
                    it.get("id").and_then(|v| v.as_str()),
                    it.get("title").and_then(|v| v.as_str()),
                ) {
                    iterations.push(IterationId {
                        field_id: id.to_string(),
                        title: title.to_string(),
                        id: iid.to_string(),
                    });
                }
            }
        }
    }

    (fields, options, iterations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::gh::test_support::MockGhClient;
    use tempfile::TempDir;

    fn issue_types_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "organization": {
                    "issueTypes": {
                        "nodes": [
                            {"id": "IT_kwABC", "name": "Bug"},
                            {"id": "IT_kwDEF", "name": "Feature"}
                        ]
                    }
                }
            }
        })
    }

    fn project_fields_response() -> serde_json::Value {
        serde_json::json!({
            "data": {
                "organization": {
                    "projectV2": {
                        "fields": {
                            "nodes": [
                                {
                                    "__typename": "ProjectV2SingleSelectField",
                                    "id": "PVTSSF_field1",
                                    "name": "Status",
                                    "dataType": "SINGLE_SELECT",
                                    "options": [
                                        {"id": "opt_todo", "name": "Todo"},
                                        {"id": "opt_done", "name": "Done"}
                                    ]
                                },
                                {
                                    "__typename": "ProjectV2IterationField",
                                    "id": "PVTIF_field2",
                                    "name": "Sprint",
                                    "dataType": "ITERATION",
                                    "configuration": {
                                        "iterations": [
                                            {"id": "iter_1", "title": "Sprint 1"}
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        })
    }

    #[test]
    fn save_load_round_trip_structural_equality() {
        let tmp = TempDir::new().unwrap();
        let snapshot = GhSchemaSnapshot {
            issue_types: vec![IssueTypeId {
                name: "Bug".into(),
                id: "IT_1".into(),
            }],
            project_fields: vec![ProjectFieldId {
                project_number: 7,
                field_name: "Status".into(),
                id: "F_1".into(),
                data_type: "SINGLE_SELECT".into(),
            }],
            single_select_options: vec![OptionId {
                field_id: "F_1".into(),
                name: "Todo".into(),
                id: "O_1".into(),
            }],
            iterations: vec![IterationId {
                field_id: "F_2".into(),
                title: "Sprint 1".into(),
                id: "I_1".into(),
            }],
            fetched_at: "2026-06-25T00:00:00+00:00".into(),
        };

        snapshot.save(tmp.path()).unwrap();
        let loaded = GhSchemaSnapshot::load(tmp.path());
        assert_eq!(snapshot, loaded);
    }

    #[test]
    fn save_creates_cache_parent_dir() {
        let tmp = TempDir::new().unwrap();
        GhSchemaSnapshot::default().save(tmp.path()).unwrap();
        assert!(tmp.path().join(".lazyspec/cache").is_dir());
        assert!(snapshot_path(tmp.path()).exists());
    }

    // AC5: offline read, missing file -> Default, no network
    #[test]
    fn load_missing_file_returns_default_no_panic() {
        let tmp = TempDir::new().unwrap();
        let snapshot = GhSchemaSnapshot::load(tmp.path());
        assert_eq!(snapshot, GhSchemaSnapshot::default());
    }

    #[test]
    fn load_corrupt_file_returns_default() {
        let tmp = TempDir::new().unwrap();
        let path = snapshot_path(tmp.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not valid json").unwrap();
        assert_eq!(
            GhSchemaSnapshot::load(tmp.path()),
            GhSchemaSnapshot::default()
        );
    }

    // AC5: resolvers return ids offline without any graphql call
    #[test]
    fn resolvers_return_ids_offline() {
        let tmp = TempDir::new().unwrap();
        let snapshot = GhSchemaSnapshot {
            issue_types: vec![IssueTypeId {
                name: "Bug".into(),
                id: "IT_1".into(),
            }],
            project_fields: vec![ProjectFieldId {
                project_number: 7,
                field_name: "Status".into(),
                id: "F_1".into(),
                data_type: "SINGLE_SELECT".into(),
            }],
            single_select_options: vec![OptionId {
                field_id: "F_1".into(),
                name: "Todo".into(),
                id: "O_1".into(),
            }],
            iterations: vec![IterationId {
                field_id: "F_2".into(),
                title: "Sprint 1".into(),
                id: "I_1".into(),
            }],
            fetched_at: String::new(),
        };
        snapshot.save(tmp.path()).unwrap();

        let loaded = GhSchemaSnapshot::load(tmp.path());
        assert_eq!(loaded.issue_type_id("Bug"), Some("IT_1"));
        assert_eq!(loaded.issue_type_id("Nope"), None);
        assert_eq!(loaded.field_id(7, "Status"), Some("F_1"));
        assert_eq!(loaded.field_id(8, "Status"), None);
        assert_eq!(loaded.option_id("F_1", "Todo"), Some("O_1"));
        assert_eq!(loaded.iteration_id("F_2", "Sprint 1"), Some("I_1"));
    }

    // AC4: fetch_snapshot persists ids (not just names)
    #[test]
    fn fetch_snapshot_captures_issue_type_ids() {
        let gh = MockGhClient::new().with_graphql_responses(vec![issue_types_response()]);
        let snapshot = fetch_snapshot(&gh, "octo-org/repo").unwrap();

        assert_eq!(snapshot.issue_types.len(), 2);
        assert_eq!(snapshot.issue_type_id("Bug"), Some("IT_kwABC"));
        assert_eq!(snapshot.issue_type_id("Feature"), Some("IT_kwDEF"));
        assert!(!snapshot.fetched_at.is_empty());

        let calls = gh.graphql_calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].1,
            vec![("owner".to_string(), GqlVar::Str("octo-org".to_string()))]
        );
    }

    #[test]
    fn fetch_project_fields_captures_field_option_iteration_ids() {
        let gh = MockGhClient::new().with_graphql_responses(vec![project_fields_response()]);
        let (fields, options, iterations) = fetch_project_fields(&gh, "octo-org/repo", 7).unwrap();

        assert_eq!(fields.len(), 2);
        assert!(fields
            .iter()
            .any(|f| f.id == "PVTSSF_field1" && f.field_name == "Status" && f.project_number == 7));
        assert!(options
            .iter()
            .any(|o| o.id == "opt_todo" && o.name == "Todo" && o.field_id == "PVTSSF_field1"));
        assert!(iterations
            .iter()
            .any(|i| i.id == "iter_1" && i.title == "Sprint 1" && i.field_id == "PVTIF_field2"));

        // number passed as typed Int var
        let calls = gh.graphql_calls.borrow();
        assert!(calls[0].1.contains(&("number".to_string(), GqlVar::Int(7))));
    }

    #[test]
    fn fetch_snapshot_rejects_bad_repo() {
        let gh = MockGhClient::new().with_graphql_responses(vec![issue_types_response()]);
        assert!(fetch_snapshot(&gh, "no-slash").is_err());
    }
}
