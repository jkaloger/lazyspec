use std::path::Path;

use anyhow::bail;
use toml_edit::{DocumentMut, Item, Table};

use crate::engine::config::{
    default_lifecycle, default_rules, starter_relationships, Config, EdgeDef, RelSelector,
    RelationshipDef, Traversal, TypeSelector, ValidationRule,
};
use crate::engine::config_write::write_config_in_place;
use crate::engine::fs::FileSystem;

use super::{ConfigFixResult, LostComment};

/// The status-conditioned `create` gate ADR-033 retired. It is no longer a
/// field on [`ValidationRule`], so a source config still carrying the key
/// parses without complaint and loses it without a word.
const GATE_KEY: &str = "require_parent_status";

/// What the translating rewrite deletes that the parsed [`Config`] cannot
/// account for: comments, which `Config::parse_lenient` throws away, and the
/// retired gate, which it silently ignores.
#[derive(Debug, Default)]
struct SourceLosses {
    comments: Vec<LostComment>,
    gates: Vec<String>,
}

/// Read those losses off the source text's `toml_edit` decor.
///
/// Only `[[rules]]` blocks are inspected, because only they are deleted whole:
/// their decor goes with them, while every other block survives the rewrite and
/// keeps its own comments (STORY-258 AC8).
fn losses_from_source(src: &str) -> anyhow::Result<SourceLosses> {
    let doc: DocumentMut = src.parse()?;
    let Some(rules) = doc.get("rules").and_then(Item::as_array_of_tables) else {
        return Ok(SourceLosses::default());
    };

    let mut losses = SourceLosses::default();
    for table in rules.iter() {
        let Some(name) = table.get("name").and_then(Item::as_str) else {
            continue;
        };
        losses
            .comments
            .extend(comments_on(table).map(|comment| LostComment {
                rule: name.to_string(),
                comment,
            }));
        if table.contains_key(GATE_KEY) {
            losses.gates.push(name.to_string());
        }
    }
    Ok(losses)
}

/// Every comment that dies with a `[[rules]]` table: the ones above its header,
/// one trailing the header itself, and one trailing each of its keys. Reading
/// order, so the plan lists them the way the file does.
fn comments_on(table: &Table) -> impl Iterator<Item = String> + '_ {
    let decor = table.decor();
    comment_lines(decor.prefix().and_then(|raw| raw.as_str()))
        .chain(comment_lines(decor.suffix().and_then(|raw| raw.as_str())))
        .chain(table.iter().flat_map(|(_, item)| {
            let suffix = item
                .as_value()
                .and_then(|value| value.decor().suffix())
                .and_then(|raw| raw.as_str());
            comment_lines(suffix)
        }))
}

fn comment_lines(decor: Option<&str>) -> impl Iterator<Item = String> + '_ {
    decor
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .map(str::to_string)
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
fn translate_to_edges(config: &Config) -> anyhow::Result<Vec<EdgeDef>> {
    let chain = chain_relationships(config);
    let mut edges: Vec<EdgeDef> = config
        .rules
        .iter()
        .flat_map(|rule| edges_from_rule(rule, &chain))
        .collect();

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
            via: RelSelector::Named(vec![relationship.name.clone()]),
            required: None,
            traversal: Some(traversal),
        });
    }

    Ok(edges)
}

/// The relationships this config marks as chain, in declared order. They are
/// what a `parent-child` rule is satisfied through today — `validation.rs`
/// reads exactly this set off `Store::chain_relationships` — so they are what
/// its translation has to name.
fn chain_relationships(config: &Config) -> Vec<&str> {
    config
        .relationships
        .iter()
        .filter(|relationship| relationship.traversal == Some(Traversal::Chain))
        .map(|relationship| relationship.name.as_str())
        .collect()
}

/// One `[[rules]]` block as edge rows.
///
/// A `parent-child` rule becomes one row per chain-marked relationship, each
/// naming that relationship in `via` (ADR-032 §Decision, as amended). Naming it
/// is the preservation rather than a tightening: the rule is satisfied today
/// only through a chain-marked relationship, so `via = "*"` would accept links
/// the rule rejects. It is also the only shape that loads — a wildcard `via`
/// carrying `traversal = "chain"` overlaps the row translated from a `related`
/// marker on all three positions, and that pair is refused at load.
///
/// A `relation-existence` rule demands a relationship without naming one, so
/// its wildcards are the shape RFC-067 gives it rather than an imprecision.
fn edges_from_rule(rule: &ValidationRule, chain: &[&str]) -> Vec<EdgeDef> {
    match rule {
        ValidationRule::ParentChild {
            name,
            child,
            parent,
            severity,
        } => chain
            .iter()
            .map(|via| EdgeDef {
                name: parent_child_edge_name(name, chain, via),
                from: TypeSelector::Types(vec![child.clone()]),
                to: TypeSelector::Types(vec![parent.clone()]),
                via: RelSelector::Named(vec![(*via).to_string()]),
                required: Some(severity.clone()),
                traversal: Some(Traversal::Chain),
            })
            .collect(),
        ValidationRule::RelationExistence {
            name,
            doc_type,
            severity,
            ..
        } => vec![EdgeDef {
            name: name.clone(),
            from: TypeSelector::Types(vec![doc_type.clone()]),
            to: TypeSelector::Any,
            via: RelSelector::Any,
            required: Some(severity.clone()),
            traversal: None,
        }],
    }
}

/// The translation still emits one row per chain relationship, so a rule
/// translated against two of them becomes two rows, and two rows may not share a
/// name. A rule that becomes exactly one row keeps its own name, so the ordinary
/// config's findings go on naming what they named before. (`via` is set-valued
/// as of ADR-032's second amendment; folding the fan-out into one row that names
/// the whole set is ITERATION-404's job, and retires this name suffix with it.)
fn parent_child_edge_name(rule: &str, chain: &[&str], via: &str) -> String {
    match chain {
        [_] => rule.to_string(),
        _ => format!("{rule}-via-{via}"),
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

/// The standard constraints to seed into a config that declares none of its own.
///
/// Seeded only into a config that has said nothing about its DAG — neither
/// `[[edges]]` nor `[[rules]]`. A config carrying `[[edges]]` has stated it on
/// the edge table's terms, and adding the legacy set back is what the rewrite
/// exists to undo. A config carrying `[[rules]]` has stated it too, and its
/// rules may already say what a standard one says under another name: seeded
/// through the translation, that pair becomes two equally specific rows
/// demanding one edge at different severities, which fails to load.
///
/// A standard rule naming a type the config does not declare is skipped for the
/// same reason — an edge row's type names are checked at load while a
/// `[[rules]]` block's are not, so seeding one would write a config that no
/// longer loads. Such a rule matches no document anyway.
fn standard_rules_to_seed(config: &Config) -> Vec<ValidationRule> {
    if !config.edges.is_empty() || !config.rules.is_empty() {
        return Vec::new();
    }
    let declared = |name: &str| config.documents.types.iter().any(|t| t.name == name);
    default_rules()
        .into_iter()
        .filter(|rule| rule_types(rule).into_iter().all(declared))
        .collect()
}

/// The document types a rule names, which are the type names its translated
/// edge row will carry.
fn rule_types(rule: &ValidationRule) -> Vec<&str> {
    match rule {
        ValidationRule::ParentChild { child, parent, .. } => vec![child, parent],
        ValidationRule::RelationExistence { doc_type, .. } => vec![doc_type],
    }
}

/// Plan (and optionally apply) the repairs an existing `.lazyspec.toml` needs.
///
/// Two kinds of repair, and they are not the same shape. Adding the standard
/// `[[relationships]]` and the default lifecycles is append-only: nothing the
/// file already says is taken away, so `[github]`, comments and ordering
/// survive. The RFC-067 edge migration is a translating REWRITE (ADR-032) — the
/// source `[[rules]]` blocks and `[[relationships]].traversal` keys have to go,
/// or the config declares its DAG twice. A comment attached to a block the
/// migration translates does not survive it, and neither does a
/// `require_parent_status` gate on one; both are read off the source by
/// [`losses_from_source`] so the plan can name them before applying. Nothing
/// else about the file changes, but that much is lost.
///
/// The two meet on the standard rule set, and the rewrite wins: `default_rules`
/// is seeded through the translation, so the standard constraints land as
/// `[[edges]]` rather than as `[[rules]]` this run would write and the next
/// would delete. That also makes one run enough — a second finds nothing
/// missing and nothing left to translate, and does not write at all. See
/// [`standard_rules_to_seed`] for which configs are seeded at all.
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

    let missing_rules = standard_rules_to_seed(&config);

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

    // What the rewrite takes away is read off the SOURCE: the seeded rules and
    // the appended relationships' markers were never in the file, so nothing of
    // theirs is being removed from it.
    let rules_removed: Vec<String> = config
        .rules
        .iter()
        .map(|r| rule_name(r).to_string())
        .collect();
    let traversal_removed: Vec<String> = config
        .relationships
        .iter()
        .filter(|r| r.traversal.is_some())
        .map(|r| r.name.clone())
        .collect();
    let SourceLosses {
        comments: comments_lost,
        gates: gates_dropped,
    } = losses_from_source(&existing)?;

    // The buffer the whole repair is planned against: the source plus every
    // block the append fixes supply, so the seeded rules translate in this run.
    let mut buffer = config.clone();
    buffer.relationships.extend(missing_relationships.clone());
    buffer.rules.extend(missing_rules.clone());
    let translated = translate_to_edges(&buffer)?;
    let edges_written: Vec<String> = translated.iter().map(|e| e.name.clone()).collect();

    let nothing_to_do = missing_relationships.is_empty()
        && missing_rules.is_empty()
        && lifecycles_added.is_empty()
        && rules_removed.is_empty()
        && traversal_removed.is_empty();

    let written = if dry_run || nothing_to_do {
        false
    } else {
        buffer.rules.clear();
        for relationship in &mut buffer.relationships {
            relationship.traversal = None;
        }
        buffer.edges.extend(translated);
        for type_def in &mut buffer.documents.types {
            if type_def.lifecycle.states.is_empty() {
                type_def.lifecycle = default_lifecycle();
            }
        }
        fs.write(&path, &write_config_in_place(&existing, &buffer)?)?;
        true
    };

    Ok(ConfigFixResult {
        relationships_added,
        rules_added,
        lifecycles_added,
        edges_written,
        rules_removed,
        traversal_removed,
        comments_lost,
        gates_dropped,
        written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        DocumentConfig, EdgeDef, RelSelector, Severity, StoreBackend, Traversal, TypeDef,
        TypeSelector,
    };

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
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(
            edges[0],
            EdgeDef {
                name: "iteration-implements-story".to_string(),
                from: TypeSelector::Types(vec!["iteration".to_string()]),
                to: TypeSelector::Types(vec!["story".to_string()]),
                via: RelSelector::Named(vec!["implements".to_string()]),
                required: Some(Severity::Error),
                traversal: Some(Traversal::Chain),
            }
        );
    }

    /// ADR-032 as amended: the rule is satisfied today only through a
    /// chain-marked relationship, so naming that relationship preserves what
    /// validates and `via = "*"` would widen it to any relationship at all.
    #[test]
    fn translated_parent_child_edge_names_the_chain_relationship_rather_than_the_wildcard() {
        let config = config_with(
            vec![parent_child_rule(
                "story-parent",
                "story",
                "rfc",
                Severity::Warning,
            )],
            vec![
                marked_relationship("implements", Traversal::Chain),
                unmarked_relationship("blocks"),
            ],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        assert_eq!(
            edges[0].via,
            RelSelector::Named(vec!["implements".to_string()])
        );
        assert_eq!(edges[0].required, Some(Severity::Warning));
    }

    /// `via` holds one name, so a second chain relationship is a second row —
    /// and the two must not collide on `name`.
    #[test]
    fn a_parent_child_rule_becomes_one_row_per_chain_relationship() {
        let config = config_with(
            vec![parent_child_rule(
                "story-parent",
                "story",
                "rfc",
                Severity::Warning,
            )],
            vec![
                marked_relationship("implements", Traversal::Chain),
                marked_relationship("targets", Traversal::Chain),
            ],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        let rows: Vec<(&str, &RelSelector)> = edges
            .iter()
            .filter(|edge| edge.name.starts_with("story-parent"))
            .map(|edge| (edge.name.as_str(), &edge.via))
            .collect();
        assert_eq!(
            rows,
            vec![
                (
                    "story-parent-via-implements",
                    &RelSelector::Named(vec!["implements".to_string()])
                ),
                (
                    "story-parent-via-targets",
                    &RelSelector::Named(vec!["targets".to_string()])
                ),
            ]
        );
    }

    /// A rule the config gives no chain relationship to is satisfiable by
    /// nothing today, so it names no edge.
    #[test]
    fn a_parent_child_rule_with_no_chain_relationship_becomes_no_row() {
        let config = config_with(
            vec![parent_child_rule(
                "story-parent",
                "story",
                "rfc",
                Severity::Warning,
            )],
            vec![marked_relationship("related-to", Traversal::Related)],
        );

        let edges = translate_to_edges(&config).expect("translation succeeds");

        let names: Vec<&str> = edges.iter().map(|edge| edge.name.as_str()).collect();
        assert_eq!(names, vec!["related-to-traversal"]);
    }

    /// The shape the whole migration turns on: a chain rule row and the row
    /// translated from a `related` marker must not read as a traversal
    /// contradiction. Under `via = "*"` they did, and no default config loaded.
    #[test]
    fn a_chain_rule_row_and_a_related_marker_row_load_together() {
        let config = Config {
            rules: vec![parent_child_rule(
                "stories-need-rfcs",
                "story",
                "rfc",
                Severity::Warning,
            )],
            relationships: vec![
                marked_relationship("implements", Traversal::Chain),
                marked_relationship("related-to", Traversal::Related),
            ],
            documents: DocumentConfig {
                types: vec![
                    TypeDef::test_fixture("story", StoreBackend::Filesystem),
                    TypeDef::test_fixture("rfc", StoreBackend::Filesystem),
                ],
                ..Config::default().documents
            },
            ..Config::default()
        };
        let mut migrated = config.clone();
        migrated.edges = translate_to_edges(&config).expect("translation succeeds");
        migrated.rules.clear();
        for relationship in &mut migrated.relationships {
            relationship.traversal = None;
        }

        let text = migrated.to_toml().expect("serializes");

        Config::parse(&text).expect("the migrated config strict-loads");
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
                via: RelSelector::Named(vec!["implements".to_string()]),
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
                via: RelSelector::Named(vec!["related-to".to_string()]),
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
                &RelSelector::Named(vec!["implements".to_string()]),
                &RelSelector::Named(vec!["related-to".to_string()]),
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

    const RULE_WITH_COMMENTS: &str = r#"# a comment about the whole file

[[relationships]]
name = "implements"
traversal = "chain" # not a rules block

# every story traces to an rfc
[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning" # loud enough to notice

[[rules]]
name = "adrs-need-relations"
shape = "relation-existence"
type = "adr"
require = "any-relation"
severity = "error"
"#;

    #[test]
    fn a_rules_block_loses_the_comments_above_it_and_the_ones_trailing_its_keys() {
        let losses = losses_from_source(RULE_WITH_COMMENTS).expect("the source is valid TOML");

        assert_eq!(
            losses
                .comments
                .iter()
                .map(|c| (c.rule.as_str(), c.comment.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("stories-need-rfcs", "# every story traces to an rfc"),
                ("stories-need-rfcs", "# loud enough to notice"),
            ]
        );
    }

    /// The whole point of naming the block: a warning on every rule is a
    /// warning the reader learns to skip.
    #[test]
    fn a_rules_block_with_no_comments_loses_none() {
        let losses = losses_from_source(RULE_WITH_COMMENTS).expect("the source is valid TOML");

        assert!(losses
            .comments
            .iter()
            .all(|c| c.rule != "adrs-need-relations"));
    }

    #[test]
    fn a_config_with_no_rules_block_loses_nothing() {
        let losses = losses_from_source("[[relationships]]\nname = \"implements\"\n")
            .expect("the source is valid TOML");

        assert!(losses.comments.is_empty());
        assert!(losses.gates.is_empty());
    }

    /// ADR-033 retired the gate with no successor, and `Config::parse_lenient`
    /// drops the key without a word, so the source text is the only place the
    /// plan can learn it was there.
    #[test]
    fn a_rule_carrying_require_parent_status_reports_a_dropped_gate() {
        let source = r#"[[rules]]
name = "stories-need-rfcs"
shape = "parent-child"
child = "story"
parent = "rfc"
severity = "warning"
require_parent_status = "accepted"

[[rules]]
name = "adrs-need-relations"
shape = "relation-existence"
type = "adr"
require = "any-relation"
severity = "error"
"#;

        let losses = losses_from_source(source).expect("the source is valid TOML");

        assert_eq!(losses.gates, vec!["stories-need-rfcs".to_string()]);
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
