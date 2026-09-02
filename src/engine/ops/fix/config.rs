use std::path::Path;

use anyhow::bail;
use toml_edit::{DocumentMut, Item, Key, Table};

use crate::engine::config::{
    default_lifecycle, starter_edges, starter_relationships, Config, EdgeDef, RelSelector,
    RelationshipDef, Severity, Traversal, TypeSelector,
};
use crate::engine::config_write::write_config_in_place;
use crate::engine::fs::FileSystem;

use super::{ConfigFixResult, LostBlock, LostComment};

/// A `[[rules]]` block as the retired shape stated it, read here and nowhere
/// else. `Config` carries no rule field any more and the loader understands no
/// rule shape (STORY-259), so the migration reads the blocks it is about to
/// delete straight off the source text -- which is also the only place their
/// comments and their `require_parent_status` gate were ever legible
/// ([`losses_from_source`]).
///
/// Only the terms ADR-032 translates are named. `require`, and any other key a
/// legacy block carries, is ignored rather than deserialized: an edge row has no
/// position for it, so reading it would only invite someone to use it.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(tag = "shape")]
enum LegacyRule {
    #[serde(rename = "parent-child")]
    ParentChild {
        name: String,
        child: String,
        parent: String,
        severity: Severity,
    },
    #[serde(rename = "relation-existence")]
    RelationExistence {
        name: String,
        #[serde(rename = "type")]
        doc_type: String,
        severity: Severity,
    },
}

/// The `[[rules]]` blocks a source config declares, in declared order. Every
/// other key in the file is ignored, so this read is independent of whether the
/// config loads.
#[derive(Debug, Default, serde::Deserialize)]
struct LegacyRules {
    #[serde(default)]
    rules: Vec<LegacyRule>,
}

/// The status-conditioned `create` gate ADR-033 retired. It is no longer a
/// field on [`LegacyRule`], so a source config still carrying the key parses
/// without complaint and loses it without a word.
const GATE_KEY: &str = "require_parent_status";

/// The `[[relationships]]` key the rewrite removes, taking its own comments with
/// it (`config_write::update_relationship_table`).
const TRAVERSAL_KEY: &str = "traversal";

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
/// Two blocks lose decor and no others. A `[[rules]]` table is deleted whole, so
/// everything attached to it goes; a `[[relationships]]` table survives but
/// loses its `traversal` key, so the comments attached to that one key go with
/// it. Every other block keeps its own comments (STORY-258 AC8).
///
/// Relationships are read first because that is where they sit in a config: the
/// plan should list the losses in the order the file states them.
fn losses_from_source(src: &str) -> anyhow::Result<SourceLosses> {
    let doc: DocumentMut = src.parse()?;
    let mut losses = SourceLosses::default();
    losses.comments.extend(traversal_key_losses(&doc));
    collect_rule_losses(&doc, &mut losses);
    Ok(losses)
}

/// The comments that die with each `[[relationships]].traversal` key the rewrite
/// removes: the ones on their own line above it, and one trailing it. Nothing
/// else on the relationship is touched, so nothing else is reported.
fn traversal_key_losses(doc: &DocumentMut) -> Vec<LostComment> {
    let Some(relationships) = doc.get("relationships").and_then(Item::as_array_of_tables) else {
        return Vec::new();
    };
    relationships
        .iter()
        .filter_map(|table| {
            let name = table.get("name").and_then(Item::as_str)?;
            let (key, item) = table.get_key_value(TRAVERSAL_KEY)?;
            Some(comments_on_key(key, item).map(|comment| LostComment {
                block: LostBlock::Relationship,
                name: name.to_string(),
                comment,
            }))
        })
        .flatten()
        .collect()
}

fn collect_rule_losses(doc: &DocumentMut, losses: &mut SourceLosses) {
    let Some(rules) = doc.get("rules").and_then(Item::as_array_of_tables) else {
        return;
    };
    for table in rules.iter() {
        let Some(name) = table.get("name").and_then(Item::as_str) else {
            continue;
        };
        losses
            .comments
            .extend(comments_on(table).map(|comment| LostComment {
                block: LostBlock::Rule,
                name: name.to_string(),
                comment,
            }));
        if table.contains_key(GATE_KEY) {
            losses.gates.push(name.to_string());
        }
    }
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

/// Every comment that dies with one key: the lines above it, which `toml_edit`
/// hangs off the key, and the one trailing its value.
fn comments_on_key<'a>(key: &'a Key, item: &'a Item) -> impl Iterator<Item = String> + 'a {
    let prefix = key.leaf_decor().prefix().and_then(|raw| raw.as_str());
    let suffix = item
        .as_value()
        .and_then(|value| value.decor().suffix())
        .and_then(|raw| raw.as_str());
    comment_lines(prefix).chain(comment_lines(suffix))
}

fn comment_lines(decor: Option<&str>) -> impl Iterator<Item = String> + '_ {
    decor
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .map(str::to_string)
}

fn legacy_rules_from_source(src: &str) -> anyhow::Result<Vec<LegacyRule>> {
    Ok(toml::from_str::<LegacyRules>(src)?.rules)
}

fn rule_name(rule: &LegacyRule) -> &str {
    match rule {
        LegacyRule::ParentChild { name, .. } => name,
        LegacyRule::RelationExistence { name, .. } => name,
    }
}

/// The `[[edges]]` rows that a pre-RFC-067 config's `[[rules]]` blocks and
/// `[[relationships]].traversal` markers translate to, term by term per
/// ADR-032, followed by the `seeded` rows a config that declared no DAG of its
/// own gets. Constraint rows come first and keep the source order, so the text
/// the writer emits from this is a function of the config alone.
///
/// Each marked relationship contributes its own row naming itself in `via`,
/// never one collapsed `via = "*"` row: per ADR-035 a wildcard `via` row
/// carrying a `traversal` suppresses the global marker of *every* relationship,
/// including the ones it did not translate.
fn translate_to_edges(
    config: &Config,
    rules: &[LegacyRule],
    seeded: &[EdgeDef],
) -> anyhow::Result<Vec<EdgeDef>> {
    let chain = chain_relationships(config);
    let mut edges: Vec<EdgeDef> = rules
        .iter()
        .map(|rule| edge_from_rule(rule, &chain))
        .collect();
    edges.extend(seeded.iter().cloned());

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
/// what a `parent-child` rule was satisfied through — the checker asked whether
/// a link used ANY chain-marked relationship — so they are what its translation
/// has to name.
fn chain_relationships(config: &Config) -> Vec<&str> {
    config
        .relationships
        .iter()
        .filter(|relationship| relationship.traversal == Some(Traversal::Chain))
        .map(|relationship| relationship.name.as_str())
        .collect()
}

/// One `[[rules]]` block as one edge row, keeping the rule's own name so the
/// findings it raises go on naming what they named before.
///
/// A `parent-child` rule becomes one row whose `via` names every chain-marked
/// relationship (ADR-032 §Decision, as twice amended). Naming them is the
/// preservation rather than a tightening: the rule is satisfied today only
/// through a chain-marked relationship, so `via = "*"` would accept links the
/// rule rejects. It is also the only shape that loads — a wildcard `via`
/// carrying `traversal = "chain"` overlaps the row translated from a `related`
/// marker on all three positions, and that pair is refused at load.
///
/// One row, not one per relationship: the set in `via` is a disjunction, which
/// is the quantifier the old checker used. A row apiece would be that many
/// independent demands of equal specificity and disjoint `via`, so none would
/// displace the others and a document would need every one of the links.
///
/// A rule the config marks no chain relationship for is satisfiable by nothing
/// today, and a rule nothing satisfies fires on every child document. Its
/// translation is the empty set — `via = []`, a row no relationship realizes —
/// so it goes on firing on every one of them (ADR-032 §Decision). Dropping the
/// rule instead would silence a whole repository's worth of findings, which is
/// the opposite of what the migration promises.
///
/// A `relation-existence` rule demands a relationship without naming one, so
/// its wildcards are the shape RFC-067 gives it rather than an imprecision.
fn edge_from_rule(rule: &LegacyRule, chain: &[&str]) -> EdgeDef {
    match rule {
        LegacyRule::ParentChild {
            name,
            child,
            parent,
            severity,
        } => EdgeDef {
            name: name.clone(),
            from: TypeSelector::Types(vec![child.clone()]),
            to: TypeSelector::Types(vec![parent.clone()]),
            via: RelSelector::Named(chain.iter().map(|via| (*via).to_string()).collect()),
            required: Some(severity.clone()),
            traversal: Some(Traversal::Chain),
        },
        LegacyRule::RelationExistence {
            name,
            doc_type,
            severity,
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

/// The standard constraints to seed into a config that declares none of its
/// own, stated as the [`starter_edges`] rows they land as. They were once
/// stated as rules and seeded through the translation; now that the retired
/// shape exists only to be read off a legacy source, the standard set is
/// declared where it belongs -- on the edge table's terms, next to the ones
/// `init` scaffolds -- and this function only decides which of them apply.
///
/// Seeded only into a config that has said nothing about its DAG — neither
/// `[[edges]]` nor `[[rules]]`. A config carrying `[[edges]]` has stated it on
/// the edge table's terms, and adding the standard set back is what the rewrite
/// exists to undo. A config carrying `[[rules]]` has stated it too, and its
/// rules may already say what a standard one says under another name: that pair
/// becomes two equally specific rows demanding one edge at different
/// severities, which fails to load.
///
/// A standard row naming a type the config does not declare is skipped for the
/// same reason — an edge row's type names are checked at load, so seeding one
/// would write a config that no longer loads. Such a row matches no document
/// anyway.
fn standard_edges_to_seed(config: &Config, rules: &[LegacyRule]) -> Vec<EdgeDef> {
    if !config.edges.is_empty() || !rules.is_empty() {
        return Vec::new();
    }
    let declared = |name: &str| config.documents.types.iter().any(|t| t.name == name);
    starter_edges()
        .into_iter()
        .filter(|edge| edge_types(edge).into_iter().all(declared))
        .collect()
}

/// The document types an edge row names. A wildcard position names none, so it
/// constrains nothing about which types the config has to declare.
fn edge_types(edge: &EdgeDef) -> Vec<&str> {
    edge.from
        .names()
        .iter()
        .chain(edge.to.names())
        .map(String::as_str)
        .collect()
}

/// Refuse to replace a config that loads with one that does not.
///
/// The rows the rewrite contributes are checked against the rows already in the
/// file only by the loader, and only once both are in one document — a
/// hand-written `via = "*"` row carrying a `traversal` overlaps every marker row
/// the append step's relationships translate to, and that pair is refused. So
/// the rendered text is parsed strictly before it is written, which covers the
/// whole class rather than the one collision anyone thought of.
///
/// The loader names the two rows that collided; this adds which of them the
/// migration wrote, since a name alone does not say whether to edit the row or
/// the file it was going to land in.
fn reject_unloadable_rewrite(rendered: &str, written: &[String]) -> anyhow::Result<()> {
    let Err(error) = Config::parse(rendered) else {
        return Ok(());
    };
    bail!(
        "migrating this config would leave it unable to load, so nothing was written: {error}\n\
         The rows the migration writes are: {}. Reconcile the row already in the file with the \
         one the migration would add — narrow it, rename it, or delete it — and run \
         `lazyspec fix --config` again.",
        written.join(", "),
    )
}

/// Plan (and optionally apply) the repairs an existing `.lazyspec.toml` needs.
///
/// The one caller of [`Config::parse_lenient`], and since STORY-259 the only
/// way into a config that declares `[[rules]]` at all: strict load refuses one,
/// naming this command as the remedy (ADR-011, ADR-012). Whatever else changes
/// here, that read stays lenient, or the remedy could not read what it repairs.
///
/// Two kinds of repair, and they are not the same shape. Adding the standard
/// `[[relationships]]` and the default lifecycles is append-only: nothing the
/// file already says is taken away, so `[github]`, comments and ordering
/// survive. The RFC-067 edge migration is a translating REWRITE (ADR-032) — the
/// source `[[rules]]` blocks and `[[relationships]].traversal` keys have to go,
/// or the config declares its DAG twice. A comment attached to either does not
/// survive, and neither does a `require_parent_status` gate on a rule; all of it
/// is read off the source by [`losses_from_source`] so the plan can name it
/// before applying. Nothing else about the file changes, but that much is lost.
///
/// The two meet on the standard constraint set, and the rewrite wins: the
/// standard constraints are seeded as `[[edges]]` — the only spelling that
/// loads. Nothing here ever writes a `[[rules]]` block, so no result field
/// claims one was added. That also makes one run enough: a second finds nothing
/// missing and nothing left to translate, and does not write at all. See
/// [`standard_edges_to_seed`] for which configs are seeded at all.
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

    // The retired blocks are read off the SOURCE. `Config` has no rule field
    // any more (STORY-259), so the lenient load says nothing about them.
    let legacy_rules = legacy_rules_from_source(&existing)?;
    let seeded_edges = standard_edges_to_seed(&config, &legacy_rules);

    let relationships_added: Vec<String> = missing_relationships
        .iter()
        .map(|r| r.name.clone())
        .collect();
    let lifecycles_added: Vec<String> = config
        .documents
        .types
        .iter()
        .filter(|t| t.lifecycle.states.is_empty())
        .map(|t| t.name.clone())
        .collect();

    // The seeded rows and the appended relationships' markers were never in the
    // file, so nothing of theirs is being removed from it.
    let rules_removed: Vec<String> = legacy_rules
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
    // block the append fixes supply, so a rule translated in this run can name
    // a relationship the same run appends.
    let mut buffer = config.clone();
    buffer.relationships.extend(missing_relationships.clone());
    let translated = translate_to_edges(&buffer, &legacy_rules, &seeded_edges)?;
    let edges_written: Vec<String> = translated.iter().map(|e| e.name.clone()).collect();

    let nothing_to_do = missing_relationships.is_empty()
        && seeded_edges.is_empty()
        && lifecycles_added.is_empty()
        && rules_removed.is_empty()
        && traversal_removed.is_empty();

    let written = if dry_run || nothing_to_do {
        false
    } else {
        for relationship in &mut buffer.relationships {
            relationship.traversal = None;
        }
        buffer.edges.extend(translated);
        for type_def in &mut buffer.documents.types {
            if type_def.lifecycle.states.is_empty() {
                type_def.lifecycle = default_lifecycle();
            }
        }
        let rendered = write_config_in_place(&existing, &buffer)?;
        reject_unloadable_rewrite(&rendered, &edges_written)?;
        fs.write(&path, &rendered)?;
        true
    };

    Ok(ConfigFixResult {
        relationships_added,
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

    fn parent_child_rule(name: &str, child: &str, parent: &str, severity: Severity) -> LegacyRule {
        LegacyRule::ParentChild {
            name: name.to_string(),
            child: child.to_string(),
            parent: parent.to_string(),
            severity,
        }
    }

    fn relation_existence_rule(name: &str, doc_type: &str, severity: Severity) -> LegacyRule {
        LegacyRule::RelationExistence {
            name: name.to_string(),
            doc_type: doc_type.to_string(),
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

    /// What the migration holds while it plans: the config as it loaded, and
    /// the `[[rules]]` blocks read off the source text beside it.
    struct Source {
        config: Config,
        rules: Vec<LegacyRule>,
    }

    fn config_with(rules: Vec<LegacyRule>, relationships: Vec<RelationshipDef>) -> Source {
        Source {
            config: Config {
                relationships,
                ..Config::default()
            },
            rules,
        }
    }

    /// [`translate_to_edges`] with nothing seeded, which is every translation
    /// test here: seeding is [`standard_edges_to_seed`]'s decision, exercised
    /// through `collect_config_fixes` in `cli_fix_config_test`.
    fn translate(source: &Source) -> anyhow::Result<Vec<EdgeDef>> {
        translate_to_edges(&source.config, &source.rules, &[])
    }

    #[test]
    fn parent_child_rule_becomes_a_chain_edge_from_child_to_parent() {
        let source = config_with(
            vec![parent_child_rule(
                "iteration-implements-story",
                "iteration",
                "story",
                Severity::Error,
            )],
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let edges = translate(&source).expect("translation succeeds");

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
        let source = config_with(
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

        let edges = translate(&source).expect("translation succeeds");

        assert_eq!(
            edges[0].via,
            RelSelector::Named(vec!["implements".to_string()])
        );
        assert_eq!(edges[0].required, Some(Severity::Warning));
    }

    /// ADR-032's second amendment: the old checker is satisfied by ANY chain
    /// relationship, so the set in `via` carries that disjunction on one row. A
    /// row apiece would be two independent demands — a conjunction — and every
    /// story would be warned for the relationship it did not use.
    #[test]
    fn a_parent_child_rule_becomes_one_row_naming_every_chain_relationship() {
        let source = config_with(
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

        let edges = translate(&source).expect("translation succeeds");

        let rows: Vec<(&str, &RelSelector)> = edges
            .iter()
            .filter(|edge| edge.name.starts_with("story-parent"))
            .map(|edge| (edge.name.as_str(), &edge.via))
            .collect();
        assert_eq!(
            rows,
            vec![(
                "story-parent",
                &RelSelector::Named(vec!["implements".to_string(), "targets".to_string()])
            )]
        );
    }

    /// A rule the config marks no chain relationship for is satisfiable by
    /// nothing today, which means it fires on every child document. The empty
    /// `via` set is the row that goes on doing that; no row at all would
    /// silence the rule, not preserve it.
    #[test]
    fn a_parent_child_rule_with_no_chain_relationship_becomes_a_row_no_relationship_satisfies() {
        let source = config_with(
            vec![parent_child_rule(
                "story-parent",
                "story",
                "rfc",
                Severity::Warning,
            )],
            vec![marked_relationship("related-to", Traversal::Related)],
        );

        let edges = translate(&source).expect("translation succeeds");

        assert_eq!(
            edges[0],
            EdgeDef {
                name: "story-parent".to_string(),
                from: TypeSelector::Types(vec!["story".to_string()]),
                to: TypeSelector::Types(vec!["rfc".to_string()]),
                via: RelSelector::Named(vec![]),
                required: Some(Severity::Warning),
                traversal: Some(Traversal::Chain),
            }
        );
        assert!(
            !edges[0].via.matches("related-to"),
            "an empty via is satisfied by no relationship at all"
        );
    }

    /// The shape the whole migration turns on: a chain rule row and the row
    /// translated from a `related` marker must not read as a traversal
    /// contradiction. Under `via = "*"` they did, and no default config loaded.
    #[test]
    fn a_chain_rule_row_and_a_related_marker_row_load_together() {
        let source = Source {
            config: Config {
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
            },
            rules: vec![parent_child_rule(
                "stories-need-rfcs",
                "story",
                "rfc",
                Severity::Warning,
            )],
        };
        let mut migrated = source.config.clone();
        migrated.edges = translate(&source).expect("translation succeeds");
        for relationship in &mut migrated.relationships {
            relationship.traversal = None;
        }

        let text = migrated.to_toml().expect("serializes");

        Config::parse(&text).expect("the migrated config strict-loads");
    }

    #[test]
    fn relation_existence_rule_becomes_a_wildcard_target_edge() {
        let source = config_with(
            vec![relation_existence_rule(
                "iteration-needs-a-relation",
                "iteration",
                Severity::Error,
            )],
            vec![],
        );

        let edges = translate(&source).expect("translation succeeds");

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
        let source = config_with(
            vec![],
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let edges = translate(&source).expect("translation succeeds");

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
        let source = config_with(
            vec![],
            vec![marked_relationship("related-to", Traversal::Related)],
        );

        let edges = translate(&source).expect("translation succeeds");

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
        let source = config_with(
            vec![],
            vec![
                marked_relationship("implements", Traversal::Chain),
                marked_relationship("related-to", Traversal::Related),
            ],
        );

        let edges = translate(&source).expect("translation succeeds");

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
        let source = config_with(
            vec![],
            vec![
                unmarked_relationship("blocks"),
                marked_relationship("implements", Traversal::Chain),
            ],
        );

        let edges = translate(&source).expect("translation succeeds");

        let names: Vec<&str> = edges.iter().map(|edge| edge.name.as_str()).collect();
        assert_eq!(names, vec!["implements-traversal"]);
    }

    /// The empty vector is the "nothing to migrate" signal: a config with no
    /// rules and no traversal markers is already on the edge table's terms.
    #[test]
    fn a_config_with_no_rules_and_no_markers_translates_to_nothing() {
        let source = config_with(vec![], vec![unmarked_relationship("blocks")]);

        let edges = translate(&source).expect("translation succeeds");

        assert_eq!(edges, vec![]);
    }

    #[test]
    fn rule_rows_precede_traversal_rows() {
        let source = config_with(
            vec![parent_child_rule(
                "a-rule",
                "iteration",
                "story",
                Severity::Error,
            )],
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let edges = translate(&source).expect("translation succeeds");

        let names: Vec<&str> = edges.iter().map(|edge| edge.name.as_str()).collect();
        assert_eq!(names, vec!["a-rule", "implements-traversal"]);
    }

    const RULE_WITH_COMMENTS: &str = r#"# a comment about the whole file

[[relationships]]
name = "implements"
# the spine of the hierarchy
traversal = "chain" # and the marker goes

[[relationships]]
name = "blocks"
inverse = "blocked-by" # this key stays, so this comment does

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

    fn reported(losses: &SourceLosses) -> Vec<(&'static str, &str, &str)> {
        losses
            .comments
            .iter()
            .map(|c| (c.block.label(), c.name.as_str(), c.comment.as_str()))
            .collect()
    }

    #[test]
    fn a_rules_block_loses_the_comments_above_it_and_the_ones_trailing_its_keys() {
        let losses = losses_from_source(RULE_WITH_COMMENTS).expect("the source is valid TOML");

        assert!(reported(&losses).contains(&(
            "rule",
            "stories-need-rfcs",
            "# every story traces to an rfc"
        )));
        assert!(reported(&losses).contains(&(
            "rule",
            "stories-need-rfcs",
            "# loud enough to notice"
        )));
    }

    /// ITERATION-378: the rewrite removes the `traversal` key from a
    /// relationship it otherwise leaves alone, and the comments attached to that
    /// one key die with it. Undisclosed destruction is what the plan exists to
    /// prevent, so the loss is reported against the relationship it belongs to.
    #[test]
    fn a_relationship_loses_the_comments_attached_to_its_traversal_key() {
        let losses = losses_from_source(RULE_WITH_COMMENTS).expect("the source is valid TOML");

        assert_eq!(
            reported(&losses)
                .into_iter()
                .filter(|(block, ..)| *block == "relationship")
                .collect::<Vec<_>>(),
            vec![
                ("relationship", "implements", "# the spine of the hierarchy"),
                ("relationship", "implements", "# and the marker goes"),
            ]
        );
    }

    /// A key the rewrite does not touch keeps its comment, so nothing is
    /// reported for it. A plan that warned about every comment in the file
    /// would teach the reader to skip the warning.
    #[test]
    fn a_relationship_key_the_rewrite_keeps_loses_no_comment() {
        let losses = losses_from_source(RULE_WITH_COMMENTS).expect("the source is valid TOML");

        assert!(
            !reported(&losses)
                .iter()
                .any(|(_, _, comment)| comment.contains("this key stays")),
            "{:?}",
            reported(&losses)
        );
    }

    /// The plan lists losses in the order the file states them, and a config
    /// states its relationships before its rules.
    #[test]
    fn relationship_losses_are_listed_before_rule_losses() {
        let losses = losses_from_source(RULE_WITH_COMMENTS).expect("the source is valid TOML");

        let blocks: Vec<&str> = reported(&losses)
            .into_iter()
            .map(|(block, ..)| block)
            .collect();
        assert_eq!(
            blocks,
            vec!["relationship", "relationship", "rule", "rule"],
            "{:?}",
            reported(&losses)
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
            .all(|c| c.name != "adrs-need-relations"));
    }

    /// The retired blocks are read off the source text, not off the loaded
    /// config: `Config` has no rule field for them to land in (STORY-259). The
    /// keys an edge row has no position for are ignored rather than refused, or
    /// the migration would choke on the very configs it exists to repair.
    #[test]
    fn the_retired_rules_are_read_off_the_source_text() {
        let rules = legacy_rules_from_source(RULE_WITH_COMMENTS).expect("the source is valid TOML");

        assert_eq!(
            rules,
            vec![
                parent_child_rule("stories-need-rfcs", "story", "rfc", Severity::Warning),
                relation_existence_rule("adrs-need-relations", "adr", Severity::Error),
            ]
        );
    }

    /// A severity outside the closed set is a rule the migration cannot
    /// translate — an edge row's `required` has nowhere to put it — so the read
    /// fails rather than guessing one. This is the one part of the retired
    /// shape still validated anywhere.
    #[test]
    fn a_rule_with_an_unknown_severity_is_refused() {
        let source = r#"[[rules]]
name = "bad-rule"
shape = "parent-child"
child = "iteration"
parent = "story"
severity = "fatal"
"#;

        legacy_rules_from_source(source).expect_err("\"fatal\" is not a severity");
    }

    #[test]
    fn a_source_declaring_no_rules_reads_as_none() {
        let rules = legacy_rules_from_source("[[relationships]]\nname = \"implements\"\n")
            .expect("the source is valid TOML");

        assert!(rules.is_empty());
    }

    /// The standard set is seeded as `[[edges]]`, and only into a config that
    /// declared no DAG of its own -- so a config that still states one as
    /// `[[rules]]` gets its own rows translated and nothing added beside them.
    #[test]
    fn nothing_is_seeded_into_a_config_that_declares_rules_of_its_own() {
        let source = config_with(
            vec![parent_child_rule(
                "stories-need-rfcs",
                "story",
                "rfc",
                Severity::Error,
            )],
            vec![],
        );

        assert!(standard_edges_to_seed(&source.config, &source.rules).is_empty());
    }

    /// A standard row naming a type the config never declares would not load,
    /// so it is not seeded. `Config::default()` declares no `spike`, and the
    /// starter set names none either, so the filter is exercised by removing a
    /// type the set does name.
    #[test]
    fn a_standard_row_naming_an_undeclared_type_is_not_seeded() {
        let mut source = config_with(vec![], vec![]);
        source.config.documents.types.retain(|t| t.name != "adr");

        let names: Vec<String> = standard_edges_to_seed(&source.config, &source.rules)
            .into_iter()
            .map(|edge| edge.name)
            .collect();

        assert_eq!(
            names,
            vec![
                "stories-need-rfcs".to_string(),
                "iterations-need-stories".to_string()
            ]
        );
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
        let source = config_with(
            vec![parent_child_rule(
                "implements-traversal",
                "iteration",
                "story",
                Severity::Error,
            )],
            vec![marked_relationship("implements", Traversal::Chain)],
        );

        let error = translate(&source).expect_err("the collision is rejected");

        let message = error.to_string();
        assert!(
            message.contains("implements-traversal") && message.contains("implements"),
            "the error must name the collision, got: {message}"
        );
    }
}
