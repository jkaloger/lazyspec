use std::path::Path;

use anyhow::bail;
use serde::Serialize;

use crate::engine::config::{
    default_lifecycle, default_rules, starter_relationships, Config, EdgeDef, RelSelector,
    RelationshipDef, Traversal, TypeSelector, ValidationRule,
};
use crate::engine::config_write::write_config_in_place;
use crate::engine::fs::FileSystem;

use super::ConfigFixResult;

/// Wrapper used solely to serialize the missing `[[relationships]]` blocks as an
/// array-of-tables, so the emitted text matches what the strict load path reads.
#[derive(Serialize)]
struct RelationshipsDoc {
    relationships: Vec<RelationshipDef>,
}

/// Wrapper used solely to serialize the missing `[[rules]]` blocks.
#[derive(Serialize)]
struct RulesDoc {
    rules: Vec<ValidationRule>,
}

fn rule_name(rule: &ValidationRule) -> &str {
    match rule {
        ValidationRule::ParentChild { name, .. } => name,
        ValidationRule::RelationExistence { name, .. } => name,
    }
}

/// The `[[edges]]` rows that a pre-RFC-067 config's `[[rules]]` blocks and
/// `[[relationships]].traversal` markers translate to, term by term per
/// ADR-032. Rule rows come first and keep the source order, so the text the
/// writer emits from this is a function of the config alone.
///
/// Each marked relationship contributes its own row naming itself in `via`,
/// never one collapsed `via = "*"` row: per ADR-035 a wildcard `via` row
/// carrying a `traversal` suppresses the global marker of *every* relationship,
/// including the ones it did not translate.
// Nothing calls this yet — ITERATION-377 wires it into `collect_config_fixes`
// once the writer that emits the rows exists.
#[allow(dead_code)]
fn translate_to_edges(config: &Config) -> anyhow::Result<Vec<EdgeDef>> {
    let mut edges: Vec<EdgeDef> = config.rules.iter().map(edge_from_rule).collect();

    for relationship in &config.relationships {
        let Some(traversal) = relationship.traversal else {
            continue;
        };
        let name = traversal_edge_name(&relationship.name);
        if edges.iter().any(|edge| edge.name == name) {
            bail!(
                "relationship \"{}\" has a traversal marker whose edge would be named \"{name}\", \
                 which the translated rule of that name already uses: rename that rule before \
                 migrating, or one of the two rows would be dropped",
                relationship.name,
            );
        }
        edges.push(EdgeDef {
            name,
            from: TypeSelector::Any,
            to: TypeSelector::Any,
            via: RelSelector::Named(relationship.name.clone()),
            required: None,
            traversal: Some(traversal),
        });
    }

    Ok(edges)
}

/// One `[[rules]]` block as an edge row. `via` is the wildcard on both shapes:
/// today's rules are satisfied by any relationship, so naming one would tighten
/// what validates and turn valid documents into findings (ADR-032 §Decision).
fn edge_from_rule(rule: &ValidationRule) -> EdgeDef {
    match rule {
        ValidationRule::ParentChild {
            name,
            child,
            parent,
            severity,
            // `require_parent_status` is dropped, not translated: ADR-033
            // abandons status-conditioned gating rather than relocating it onto
            // the edge table, so it has no destination field.
            ..
        } => EdgeDef {
            name: name.clone(),
            from: TypeSelector::Types(vec![child.clone()]),
            to: TypeSelector::Types(vec![parent.clone()]),
            via: RelSelector::Any,
            required: Some(severity.clone()),
            traversal: Some(Traversal::Chain),
        },
        ValidationRule::RelationExistence {
            name,
            doc_type,
            severity,
            ..
        } => EdgeDef {
            name: name.clone(),
            from: TypeSelector::Types(vec![doc_type.clone()]),
            to: TypeSelector::Any,
            via: RelSelector::Any,
            required: Some(severity.clone()),
            traversal: None,
        },
    }
}

/// The name for the row derived from a relationship's traversal marker. A
/// marker carries no name of its own, unlike a rule, so one is derived from the
/// relationship it speaks about. The role is left out: the row's own
/// `traversal` field states it, and a name that repeated it would go stale the
/// moment an author edited the row.
fn traversal_edge_name(relationship: &str) -> String {
    format!("{relationship}-traversal")
}

/// Plan (and optionally apply) the config migration: append the standard
/// relationships/rules that the existing `.lazyspec.toml` is missing.
///
/// Append-only by design: the existing file is preserved byte-for-byte and only
/// the missing `[[relationships]]` / `[[rules]]` blocks are appended. This keeps
/// every user section (`[github]`, comments, ordering) intact
/// and is idempotent — when nothing is missing the file is left untouched.
pub fn collect_config_fixes(
    root: &Path,
    dry_run: bool,
    fs: &dyn FileSystem,
) -> anyhow::Result<ConfigFixResult> {
    let path = root.join(".lazyspec.toml");
    let existing = fs.read_to_string(&path)?;

    // Lenient read: tolerate a missing [[relationships]] block.
    let config = Config::parse_lenient(&existing)?;

    let existing_rel_names: Vec<&str> = config
        .relationships
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    let missing_relationships: Vec<RelationshipDef> = starter_relationships()
        .into_iter()
        .filter(|r| !existing_rel_names.contains(&r.name.as_str()))
        .collect();

    let existing_rule_names: Vec<&str> = config.rules.iter().map(rule_name).collect();
    let missing_rules: Vec<ValidationRule> = default_rules()
        .into_iter()
        .filter(|r| !existing_rule_names.contains(&rule_name(r)))
        .collect();

    let relationships_added: Vec<String> = missing_relationships
        .iter()
        .map(|r| r.name.clone())
        .collect();
    let rules_added: Vec<String> = missing_rules
        .iter()
        .map(|r| rule_name(r).to_string())
        .collect();

    let lifecycles_added: Vec<String> = config
        .documents
        .types
        .iter()
        .filter(|t| t.lifecycle.states.is_empty())
        .map(|t| t.name.clone())
        .collect();

    let nothing_missing =
        missing_relationships.is_empty() && missing_rules.is_empty() && lifecycles_added.is_empty();

    let written = if dry_run || nothing_missing {
        false
    } else {
        let appended = append_blocks(&existing, &missing_relationships, &missing_rules)?;
        let migrated = if lifecycles_added.is_empty() {
            appended
        } else {
            let mut buffer = Config::parse_lenient(&appended)?;
            for type_def in &mut buffer.documents.types {
                if type_def.lifecycle.states.is_empty() {
                    type_def.lifecycle = default_lifecycle();
                }
            }
            write_config_in_place(&appended, &buffer)?
        };
        fs.write(&path, &migrated)?;
        true
    };

    Ok(ConfigFixResult {
        relationships_added,
        rules_added,
        lifecycles_added,
        written,
    })
}

/// Append the missing blocks to the existing config text, separated by a blank
/// line, leaving the original content untouched.
fn append_blocks(
    existing: &str,
    relationships: &[RelationshipDef],
    rules: &[ValidationRule],
) -> anyhow::Result<String> {
    let mut out = existing.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }

    if !relationships.is_empty() {
        let block = toml::to_string(&RelationshipsDoc {
            relationships: relationships.to_vec(),
        })?;
        out.push('\n');
        out.push_str(&block);
    }

    if !rules.is_empty() {
        let block = toml::to_string(&RulesDoc {
            rules: rules.to_vec(),
        })?;
        out.push('\n');
        out.push_str(&block);
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{EdgeDef, RelSelector, Severity, Traversal, TypeSelector};

    fn parent_child_rule(
        name: &str,
        child: &str,
        parent: &str,
        severity: Severity,
    ) -> ValidationRule {
        ValidationRule::ParentChild {
            name: name.to_string(),
            child: child.to_string(),
            parent: parent.to_string(),
            severity,
            require_parent_status: None,
        }
    }

    fn relation_existence_rule(name: &str, doc_type: &str, severity: Severity) -> ValidationRule {
        ValidationRule::RelationExistence {
            name: name.to_string(),
            doc_type: doc_type.to_string(),
            require: "implements".to_string(),
            severity,
        }
    }

    fn marked_relationship(name: &str, traversal: Traversal) -> RelationshipDef {
        RelationshipDef {
            name: name.to_string(),
            inverse: None,
            github_native: None,
            traversal: Some(traversal),
        }
    }

    fn unmarked_relationship(name: &str) -> RelationshipDef {
        RelationshipDef {
            name: name.to_string(),
            inverse: None,
            github_native: None,
            traversal: None,
        }
    }

    fn config_with(rules: Vec<ValidationRule>, relationships: Vec<RelationshipDef>) -> Config {
        Config {
            rules,
            relationships,
            ..Config::default()
        }
    }

    #[test]
    fn parent_child_rule_becomes_a_chain_edge_from_child_to_parent() {
        let config = config_with(
            vec![parent_child_rule(
                "iteration-implements-story",
                "iteration",
                "story",
                Severity::Error,
            )],
            vec![],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(
            edges,
            vec![EdgeDef {
                name: "iteration-implements-story".to_string(),
                from: TypeSelector::Types(vec!["iteration".to_string()]),
                to: TypeSelector::Types(vec!["story".to_string()]),
                via: RelSelector::Any,
                required: Some(Severity::Error),
                traversal: Some(Traversal::Chain),
            }]
        );
    }

    #[test]
    fn translated_parent_child_edge_accepts_any_relationship_rather_than_implements() {
        let config = config_with(
            vec![parent_child_rule(
                "story-parent",
                "story",
                "rfc",
                Severity::Warning,
            )],
            vec![],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(edges[0].via, RelSelector::Any);
        assert_eq!(edges[0].required, Some(Severity::Warning));
    }

    #[test]
    fn relation_existence_rule_becomes_a_wildcard_target_edge() {
        let config = config_with(
            vec![relation_existence_rule(
                "iteration-needs-a-relation",
                "iteration",
                Severity::Error,
            )],
            vec![],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(
            edges,
            vec![EdgeDef {
                name: "iteration-needs-a-relation".to_string(),
                from: TypeSelector::Types(vec!["iteration".to_string()]),
                to: TypeSelector::Any,
                via: RelSelector::Any,
                required: Some(Severity::Error),
                traversal: None,
            }]
        );
    }

    #[test]
    fn relationship_marked_chain_becomes_a_wildcard_row_via_that_relationship() {
        let config = config_with(
            vec![],
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(
            edges,
            vec![EdgeDef {
                name: "implements-traversal".to_string(),
                from: TypeSelector::Any,
                to: TypeSelector::Any,
                via: RelSelector::Named("implements".to_string()),
                required: None,
                traversal: Some(Traversal::Chain),
            }]
        );
    }

    #[test]
    fn relationship_marked_related_becomes_a_wildcard_row_via_that_relationship() {
        let config = config_with(
            vec![],
            vec![marked_relationship("related-to", Traversal::Related)],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(
            edges,
            vec![EdgeDef {
                name: "related-to-traversal".to_string(),
                from: TypeSelector::Any,
                to: TypeSelector::Any,
                via: RelSelector::Named("related-to".to_string()),
                required: None,
                traversal: Some(Traversal::Related),
            }]
        );
    }

    /// ADR-035: a single `via = "*"` row would suppress the global marker of
    /// every relationship it did not translate, so each marked relationship
    /// gets its own row naming itself.
    #[test]
    fn each_marked_relationship_gets_its_own_row() {
        let config = config_with(
            vec![],
            vec![
                marked_relationship("implements", Traversal::Chain),
                marked_relationship("related-to", Traversal::Related),
            ],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        let vias: Vec<&RelSelector> = edges.iter().map(|edge| &edge.via).collect();
        assert_eq!(
            vias,
            vec![
                &RelSelector::Named("implements".to_string()),
                &RelSelector::Named("related-to".to_string()),
            ]
        );
    }

    #[test]
    fn an_unmarked_relationship_contributes_no_row() {
        let config = config_with(
            vec![],
            vec![
                unmarked_relationship("blocks"),
                marked_relationship("implements", Traversal::Chain),
            ],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        let names: Vec<&str> = edges.iter().map(|edge| edge.name.as_str()).collect();
        assert_eq!(names, vec!["implements-traversal"]);
    }

    /// The empty vector is the "nothing to migrate" signal: a config with no
    /// rules and no traversal markers is already on the edge table's terms.
    #[test]
    fn a_config_with_no_rules_and_no_markers_translates_to_nothing() {
        let config = config_with(vec![], vec![unmarked_relationship("blocks")]);

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(edges, vec![]);
    }

    /// ADR-033 abandons status-conditioned gating outright rather than moving it
    /// onto the edge table, so the rule's `require_parent_status` has nowhere to
    /// go and the edge is the one the same rule without it produces.
    #[test]
    fn require_parent_status_is_dropped_rather_than_translated() {
        let gated = ValidationRule::ParentChild {
            name: "iteration-implements-story".to_string(),
            child: "iteration".to_string(),
            parent: "story".to_string(),
            severity: Severity::Error,
            require_parent_status: Some("accepted".to_string()),
        };
        let ungated = parent_child_rule(
            "iteration-implements-story",
            "iteration",
            "story",
            Severity::Error,
        );

        let from_gated =
            translate_to_edges(&config_with(vec![gated], vec![])).expect("translation succeeds");
        let from_ungated =
            translate_to_edges(&config_with(vec![ungated], vec![])).expect("translation succeeds");

        assert_eq!(from_gated, from_ungated);
    }

    #[test]
    fn rule_rows_precede_traversal_rows() {
        let config = config_with(
            vec![parent_child_rule(
                "a-rule",
                "iteration",
                "story",
                Severity::Error,
            )],
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        let names: Vec<&str> = edges.iter().map(|edge| edge.name.as_str()).collect();
        assert_eq!(names, vec!["a-rule", "implements-traversal"]);
    }

    #[test]
    fn a_derived_name_colliding_with_a_translated_rule_name_is_an_error() {
        let config = config_with(
            vec![parent_child_rule(
                "implements-traversal",
                "iteration",
                "story",
                Severity::Error,
            )],
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let error = translate_to_edges(&config).expect_err("the collision is rejected");

        let message = error.to_string();
        assert!(
            message.contains("implements-traversal") && message.contains("implements"),
            "the error must name the collision, got: {message}"
        );
    }
}
