use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::config::Lifecycle;

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

    /// The board's `Status` option whose name matches `status` ignoring case.
    ///
    /// Case-insensitive where [`Self::option_id`] is exact, because the two are
    /// asked different questions: a `PROJECT-n.<field>` write carries the option
    /// name as the user typed it, while a lazyspec
    /// [`Status`](crate::engine::document::Status) is always lowercased, so
    /// `In Progress` can only ever arrive here as `in progress`. Lowercasing both
    /// sides (rather than `eq_ignore_ascii_case`) matches exactly the
    /// transformation [`Self::status_lifecycle`] applies to derive the states.
    pub fn status_option(&self, project_number: u64, status: &str) -> Option<&OptionId> {
        let field_id = self.field_id(project_number, "Status")?;
        let wanted = status.to_lowercase();
        self.single_select_options
            .iter()
            .find(|o| o.field_id == field_id && o.name.to_lowercase() == wanted)
    }

    /// The board's `Status` option names as the board spells them, in board
    /// order. Empty when the board (or its `Status` field) is not in the
    /// snapshot. For naming the valid columns when a requested status matches
    /// none of them.
    pub fn status_option_names(&self, project_number: u64) -> Vec<&str> {
        let Some(field_id) = self.field_id(project_number, "Status") else {
            return Vec::new();
        };
        self.single_select_options
            .iter()
            .filter(|o| o.field_id == field_id)
            .map(|o| o.name.as_str())
            .collect()
    }

    pub fn iteration_id(&self, field_id: &str, title: &str) -> Option<&str> {
        self.iterations
            .iter()
            .find(|i| i.field_id == field_id && i.title == title)
            .map(|i| i.id.as_str())
    }

    /// Derive a [`Lifecycle`] from a board's `Status` column set: the states are
    /// the option names lowercased, in board order, and there are no edges --
    /// GitHub enforces no transition rules, so lazyspec adds no local gating
    /// (the same empty-edge posture `derive_lifecycle` takes for ClickUp).
    ///
    /// Lowercasing is required, not cosmetic: [`Status`](crate::engine::document::Status)
    /// lowercases on construction and on deserialize, so a state declared as
    /// `In Progress` could never be matched by any status a doc actually carries.
    ///
    /// Board order is the stored `Vec` order and is deliberately NOT sorted --
    /// `OptionId` carries no index to sort by, so sorting would replace the
    /// board's column order with alphabetical noise.
    ///
    /// `None` (rather than an empty lifecycle) when the board has no `Status`
    /// field or that field has no options, so a caller persisting the result
    /// cannot silently wipe a type's states.
    pub fn status_lifecycle(&self, project_number: u64) -> Option<Lifecycle> {
        let field_id = self.field_id(project_number, "Status")?;
        let states: Vec<String> = self
            .single_select_options
            .iter()
            .filter(|o| o.field_id == field_id)
            .map(|o| o.name.to_lowercase())
            .collect();
        if states.is_empty() {
            return None;
        }
        Some(Lifecycle {
            states,
            edges: Vec::new(),
        })
    }

    /// Swap one board's cached field ids for a freshly fetched set. The board's
    /// prior fields and everything keyed off them go first, so a column that was
    /// renamed or deleted on GitHub does not linger alongside its replacement.
    pub fn replace_board_fields(
        &mut self,
        project_number: u64,
        fields: Vec<ProjectFieldId>,
        options: Vec<OptionId>,
        iterations: Vec<IterationId>,
    ) {
        let stale: std::collections::HashSet<String> = self
            .project_fields
            .iter()
            .filter(|f| f.project_number == project_number)
            .map(|f| f.id.clone())
            .collect();

        self.project_fields
            .retain(|f| f.project_number != project_number);
        self.single_select_options
            .retain(|o| !stale.contains(&o.field_id));
        self.iterations.retain(|i| !stale.contains(&i.field_id));

        self.project_fields.extend(fields);
        self.single_select_options.extend(options);
        self.iterations.extend(iterations);
    }
}

pub(crate) fn split_repo(repo: &str) -> Result<(&str, &str)> {
    repo.split_once('/')
        .filter(|(o, n)| !o.is_empty() && !n.is_empty())
        .with_context(|| format!("repo '{}' must be in owner/name form", repo))
}

/// Read one board's `ProjectV2.fields` connection nodes into the three id sets
/// the snapshot stores. Shared by every caller that resolves a board schema, so
/// the composed round and any future reader agree on the shape.
pub(crate) fn parse_project_fields(
    nodes: &serde_json::Value,
    project_number: u64,
) -> (Vec<ProjectFieldId>, Vec<OptionId>, Vec<IterationId>) {
    let mut fields = Vec::new();
    let mut options = Vec::new();
    let mut iterations = Vec::new();

    let Some(nodes) = nodes.as_array() else {
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
    use tempfile::TempDir;

    /// A one-board `fields` connection whose `Status` single-select carries
    /// `options` in the given order. `field_id` distinguishes boards within one
    /// snapshot.
    fn status_field_nodes(field_id: &str, options: &[&str]) -> serde_json::Value {
        let opts: Vec<serde_json::Value> = options
            .iter()
            .map(|name| {
                serde_json::json!({
                    "id": format!("opt_{}", name.to_lowercase().replace(' ', "_")),
                    "name": name
                })
            })
            .collect();
        serde_json::json!([{
            "__typename": "ProjectV2SingleSelectField",
            "id": field_id,
            "name": "Status",
            "dataType": "SINGLE_SELECT",
            "options": opts
        }])
    }

    fn sprint_only_nodes() -> serde_json::Value {
        serde_json::json!([{
            "__typename": "ProjectV2IterationField",
            "id": "PVTIF_sprint",
            "name": "Sprint",
            "dataType": "ITERATION",
            "configuration": {"iterations": [{"id": "iter_1", "title": "Sprint 1"}]}
        }])
    }

    /// Build a snapshot by parsing each board's canned field nodes, so the
    /// vector order under test is the order GraphQL actually produced.
    fn snapshot_with_boards(boards: &[(u64, serde_json::Value)]) -> GhSchemaSnapshot {
        let mut snapshot = GhSchemaSnapshot::default();
        for (number, nodes) in boards {
            let (fields, options, iterations) = parse_project_fields(nodes, *number);
            snapshot.project_fields.extend(fields);
            snapshot.single_select_options.extend(options);
            snapshot.iterations.extend(iterations);
        }
        snapshot
    }

    #[test]
    fn status_lifecycle_lowercases_option_names_in_board_order() {
        let snapshot = snapshot_with_boards(&[(
            7,
            status_field_nodes(
                "PVTSSF_b7",
                &["Ready To Start", "In Progress", "Review", "Done"],
            ),
        )]);

        let lifecycle = snapshot.status_lifecycle(7).expect("board 7 has a Status");

        assert_eq!(
            lifecycle.states,
            vec!["ready to start", "in progress", "review", "done"]
        );
        assert!(lifecycle.edges.is_empty());
    }

    // AC5: the write path recovers the option id from a status that has already
    // been lowercased, so the match must ignore the board's display casing.
    #[test]
    fn status_option_matches_a_lowercased_status_against_a_display_cased_column() {
        let snapshot = snapshot_with_boards(&[(
            7,
            status_field_nodes("PVTSSF_b7", &["Ready To Start", "In Progress"]),
        )]);

        let option = snapshot
            .status_option(7, "in progress")
            .expect("the lowercased status resolves its column");

        assert_eq!(option.id, "opt_in_progress");
        assert_eq!(option.field_id, "PVTSSF_b7");
        // The value as typed on the command line resolves the same option.
        assert_eq!(snapshot.status_option(7, "In Progress"), Some(option));
    }

    #[test]
    fn status_option_misses_an_unknown_column_and_an_unknown_board() {
        let snapshot = snapshot_with_boards(&[(
            7,
            status_field_nodes("PVTSSF_b7", &["Ready To Start", "In Progress"]),
        )]);

        assert_eq!(snapshot.status_option(7, "blocked"), None);
        assert_eq!(snapshot.status_option(9, "in progress"), None);
    }

    // AC6: the rejection names the columns as the board spells them, in board
    // order, so the message is something a user can copy.
    #[test]
    fn status_option_names_lists_the_boards_columns_in_board_order() {
        let snapshot = snapshot_with_boards(&[
            (
                7,
                status_field_nodes("PVTSSF_b7", &["Ready To Start", "In Progress", "Done"]),
            ),
            (9, status_field_nodes("PVTSSF_b9", &["Triage"])),
        ]);

        assert_eq!(
            snapshot.status_option_names(7),
            vec!["Ready To Start", "In Progress", "Done"]
        );
        assert_eq!(snapshot.status_option_names(9), vec!["Triage"]);
        assert!(snapshot.status_option_names(11).is_empty());
    }

    #[test]
    fn status_lifecycle_returns_none_when_board_has_no_status_field() {
        let snapshot = snapshot_with_boards(&[(7, sprint_only_nodes())]);
        assert_eq!(snapshot.status_lifecycle(7), None);
    }

    #[test]
    fn status_lifecycle_returns_none_when_status_field_has_no_options() {
        let snapshot = snapshot_with_boards(&[(7, status_field_nodes("PVTSSF_b7", &[]))]);
        assert_eq!(snapshot.status_lifecycle(7), None);
    }

    #[test]
    fn status_lifecycle_reads_only_the_named_boards_status() {
        let snapshot = snapshot_with_boards(&[
            (7, status_field_nodes("PVTSSF_b7", &["Review", "Done"])),
            (9, status_field_nodes("PVTSSF_b9", &["Triage", "Shipped"])),
        ]);

        assert_eq!(
            snapshot.status_lifecycle(7).unwrap().states,
            vec!["review", "done"]
        );
        assert_eq!(
            snapshot.status_lifecycle(9).unwrap().states,
            vec!["triage", "shipped"]
        );
    }

    fn project_field_nodes() -> serde_json::Value {
        serde_json::json!([
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
                "configuration": {"iterations": [{"id": "iter_1", "title": "Sprint 1"}]}
            }
        ])
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

    #[test]
    fn parse_project_fields_captures_field_option_iteration_ids() {
        let (fields, options, iterations) = parse_project_fields(&project_field_nodes(), 7);

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
    }

    // Anything that is not a `nodes` array yields nothing rather than a partial
    // read: the caller decides whether that is a known-empty board or a failure.
    #[test]
    fn parse_project_fields_of_a_non_array_is_empty() {
        let (fields, options, iterations) = parse_project_fields(&serde_json::Value::Null, 7);
        assert!(fields.is_empty() && options.is_empty() && iterations.is_empty());
    }
}
