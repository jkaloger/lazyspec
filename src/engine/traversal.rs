use crate::engine::config::{Config, EdgeDef, Traversal};

/// Which concrete edges join a traversal, asked as the triple RFC-067
/// §Problem.3 says the question really is: source type, relationship, target
/// type. `targets` is genuine hierarchy for iteration -> milestone and
/// accidental hierarchy for every other pair, and only the triple can tell
/// those apart.
///
/// Both traversal roles are answered from one row table rather than one index
/// each, because a row already carries the role it assigns and the question
/// asked of it is the same question either way.
///
/// Two declarations feed this and they deliberately do NOT union. An
/// `[[edges]]` row that states a `traversal` for relationship X decides X's
/// walk membership outright, suppressing `RelationshipDef.traversal` for X; a
/// relationship no row states a traversal for keeps its global marker as a
/// blanket fallback. Unioning would make the whole change inert wherever it
/// matters most: this project marks `targets` chain globally, so a union would
/// keep every `targets` link hierarchy no matter what the table said. This is
/// not the coexistence rule `[[rules]]` and `[[edges]]` follow for findings --
/// findings stack, a walk cannot.
#[derive(Debug, Default)]
pub struct TraversalWalk {
    /// Every row that states a `traversal`, kept whole because deciding a
    /// triple needs all three selectors as well as the role assigned.
    rows: Vec<EdgeDef>,
    /// Relationship names still carrying a global `traversal` marker, because
    /// no row states a traversal for them, paired with the role it marks.
    blanket: Vec<(String, Traversal)>,
}

/// True iff any `[[edges]]` row states a traversal for `rel_name`, whatever
/// role that row assigns. Such a row suppresses `rel_name`'s global marker for
/// BOTH roles: the table has spoken about the relationship, so the blanket
/// stops applying to it -- see the type doc on why the two do not union.
fn states_a_traversal_for(config: &Config, rel_name: &str) -> bool {
    config
        .edges
        .iter()
        .filter(|edge| edge.traversal.is_some())
        .any(|edge| edge.via.matches(rel_name))
}

impl TraversalWalk {
    pub(crate) fn from_config(config: &Config) -> Self {
        TraversalWalk {
            rows: config
                .edges
                .iter()
                .filter(|edge| edge.traversal.is_some())
                .cloned()
                .collect(),
            blanket: config
                .relationships
                .iter()
                .filter(|rel| !states_a_traversal_for(config, &rel.name))
                .filter_map(|rel| rel.traversal.map(|role| (rel.name.clone(), role)))
                .collect(),
        }
    }

    /// True iff a `via` relation from a `from`-typed document to a `to`-typed
    /// document walks the parent-child chain.
    pub(crate) fn walks_chain(&self, from: &str, via: &str, to: &str) -> bool {
        self.walks(Traversal::Chain, from, via, to)
    }

    /// True iff a `via` relation from a `from`-typed document to a `to`-typed
    /// document joins the related neighbourhood. The triple is asked in the
    /// direction the relation was DECLARED, so a caller following a link
    /// backwards must still pass the declaring document's type as `from`.
    pub(crate) fn walks_related(&self, from: &str, via: &str, to: &str) -> bool {
        self.walks(Traversal::Related, from, via, to)
    }

    /// Rows are scanned per call rather than expanded into a materialised
    /// triple index, because the index would be the far bigger object: a
    /// wildcard row covers the whole cross product of declared types, while the
    /// rows themselves number a config's worth. Traversal roles compose, so any
    /// matching row suffices and load-time rejection of disagreements means
    /// there is never a tie to break.
    fn walks(&self, role: Traversal, from: &str, via: &str, to: &str) -> bool {
        self.blanket
            .iter()
            .any(|(name, marked)| name == via && *marked == role)
            || self.rows.iter().any(|row| {
                row.traversal == Some(role) && row.from.matches(from) && row.matches_target(via, to)
            })
    }

    /// The types that hang beneath a `parent_type` document in the chain: the
    /// `from` side of every chain row whose `to` admits `parent_type`. This is
    /// the reverse index STORY-257 §Notes asks for, over the same rows
    /// [`walks_chain`](Self::walks_chain) reads forward. Related rows are no
    /// answer to it: the neighbourhood is not hierarchy, so it has no children.
    ///
    /// Wildcards filter but do not enumerate. A `to = "*"` row still admits
    /// every parent type, because asking whether a type is admitted is a
    /// membership question. A `from = "*"` row contributes nothing, because
    /// asking which types are children is an enumeration question and a
    /// wildcard spells out no type names ([`TypeSelector::names`]) — it declines
    /// to say what its sources are. Answering "every declared type" instead
    /// would make a config's one blanket row report every type as every type's
    /// child, which is no answer at all; a config author who wants concrete
    /// child types names them in `from`. The blanket relationship set is silent
    /// here for the same reason, one step further: a global marker names a
    /// relationship and no types whatsoever. While `[[rules]]` exists, its
    /// parent/child pairs are the answer for such configs, which is why the
    /// caller unions the two.
    ///
    /// [`TypeSelector::names`]: crate::engine::config::TypeSelector::names
    pub(crate) fn child_types_for(&self, parent_type: &str) -> Vec<String> {
        self.rows
            .iter()
            .filter(|row| row.traversal == Some(Traversal::Chain))
            .filter(|row| row.to.matches(parent_type))
            .flat_map(|row| row.from.names())
            .fold(Vec::new(), |mut child_types, name| {
                if !child_types.contains(name) {
                    child_types.push(name.clone());
                }
                child_types
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{RelSelector, RelationshipDef, TypeSelector};

    fn edge(
        name: &str,
        from: TypeSelector,
        to: TypeSelector,
        via: RelSelector,
        traversal: Traversal,
    ) -> EdgeDef {
        EdgeDef {
            name: name.to_string(),
            from,
            to,
            via,
            required: None,
            traversal: Some(traversal),
        }
    }

    fn chain_edge(name: &str, from: TypeSelector, to: TypeSelector, via: RelSelector) -> EdgeDef {
        edge(name, from, to, via, Traversal::Chain)
    }

    fn one_type(name: &str) -> TypeSelector {
        TypeSelector::Types(vec![name.to_string()])
    }

    fn chain_relationship(name: &str) -> RelationshipDef {
        RelationshipDef {
            name: name.to_string(),
            inverse: None,
            github_native: None,
            traversal: Some(Traversal::Chain),
        }
    }

    fn config_with(edges: Vec<EdgeDef>, relationships: Vec<RelationshipDef>) -> Config {
        Config {
            edges,
            relationships,
            ..Config::default()
        }
    }

    #[test]
    fn a_relationship_no_row_mentions_keeps_its_global_marker() {
        let walk = TraversalWalk::from_config(&config_with(
            Vec::new(),
            vec![chain_relationship("implements")],
        ));

        assert!(walk.walks_chain("iteration", "implements", "story"));
    }

    #[test]
    fn a_row_stating_traversal_suppresses_the_global_marker_for_other_pairs() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![chain_edge(
                "iterations-target-milestones",
                one_type("iteration"),
                one_type("milestone"),
                RelSelector::Named("targets".to_string()),
            )],
            vec![chain_relationship("targets")],
        ));

        assert!(!walk.walks_chain("iteration", "targets", "story"));
    }

    #[test]
    fn a_row_declares_the_triple_it_names() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![chain_edge(
                "iterations-target-milestones",
                one_type("iteration"),
                one_type("milestone"),
                RelSelector::Named("targets".to_string()),
            )],
            vec![chain_relationship("targets")],
        ));

        assert!(walk.walks_chain("iteration", "targets", "milestone"));
    }

    // A row stating `traversal = "related"` still suppresses the global chain
    // marker: the row has assigned the relationship a role, and the two
    // declarations do not union.
    #[test]
    fn a_row_giving_a_relationship_the_related_role_suppresses_its_chain_marker() {
        let related_row = edge(
            "targets-are-related",
            TypeSelector::Any,
            TypeSelector::Any,
            RelSelector::Named("targets".to_string()),
            Traversal::Related,
        );

        let walk = TraversalWalk::from_config(&config_with(
            vec![related_row],
            vec![chain_relationship("targets")],
        ));

        assert!(!walk.walks_chain("iteration", "targets", "milestone"));
    }

    #[test]
    fn a_relationship_with_neither_a_row_nor_a_marker_never_walks() {
        let walk = TraversalWalk::from_config(&config_with(Vec::new(), Vec::new()));

        assert!(!walk.walks_chain("iteration", "blocks", "story"));
        assert!(!walk.walks_related("iteration", "blocks", "story"));
    }

    // --- the related role ---------------------------------------------------

    fn related_relationship(name: &str) -> RelationshipDef {
        RelationshipDef {
            name: name.to_string(),
            inverse: None,
            github_native: None,
            traversal: Some(Traversal::Related),
        }
    }

    #[test]
    fn a_relationship_no_row_mentions_keeps_its_global_related_marker() {
        let walk = TraversalWalk::from_config(&config_with(
            Vec::new(),
            vec![related_relationship("related-to")],
        ));

        assert!(walk.walks_related("iteration", "related-to", "story"));
    }

    #[test]
    fn a_related_row_declares_only_the_triple_it_names() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![edge(
                "iterations-relate-to-stories",
                one_type("iteration"),
                one_type("story"),
                RelSelector::Named("related-to".to_string()),
                Traversal::Related,
            )],
            Vec::new(),
        ));

        assert!(walk.walks_related("iteration", "related-to", "story"));
        assert!(!walk.walks_related("rfc", "related-to", "story"));
        assert!(!walk.walks_related("iteration", "related-to", "rfc"));
    }

    #[test]
    fn a_related_row_does_not_give_its_triple_the_chain_role() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![edge(
                "iterations-relate-to-stories",
                one_type("iteration"),
                one_type("story"),
                RelSelector::Named("related-to".to_string()),
                Traversal::Related,
            )],
            Vec::new(),
        ));

        assert!(!walk.walks_chain("iteration", "related-to", "story"));
    }

    // The suppression a row performs is role-blind: a CHAIN row naming
    // `related-to` still silences `related-to`'s global RELATED marker, because
    // the table has spoken about the relationship. Pinned rather than endorsed
    // -- RFC-067 has no spelling for "state one role and keep the other".
    #[test]
    fn a_chain_row_suppresses_the_relationships_global_related_marker() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![chain_edge(
                "iterations-relate-to-milestones",
                one_type("iteration"),
                one_type("milestone"),
                RelSelector::Named("related-to".to_string()),
            )],
            vec![related_relationship("related-to")],
        ));

        assert!(!walk.walks_related("rfc", "related-to", "story"));
    }

    #[test]
    fn child_types_are_the_from_side_of_rows_pointing_at_the_type() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![
                chain_edge(
                    "iterations-implement-stories",
                    one_type("iteration"),
                    one_type("story"),
                    RelSelector::Named("implements".to_string()),
                ),
                chain_edge(
                    "stories-implement-rfcs",
                    one_type("story"),
                    one_type("rfc"),
                    RelSelector::Named("implements".to_string()),
                ),
            ],
            Vec::new(),
        ));

        assert_eq!(walk.child_types_for("story"), vec!["iteration".to_string()]);
        assert_eq!(walk.child_types_for("rfc"), vec!["story".to_string()]);
    }

    #[test]
    fn one_row_naming_several_from_types_yields_all_of_them() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![chain_edge(
                "work-implements-stories",
                TypeSelector::Types(vec!["iteration".to_string(), "spike".to_string()]),
                one_type("story"),
                RelSelector::Any,
            )],
            Vec::new(),
        ));

        assert_eq!(
            walk.child_types_for("story"),
            vec!["iteration".to_string(), "spike".to_string()]
        );
    }

    #[test]
    fn two_rows_naming_the_same_from_type_report_it_once() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![
                chain_edge(
                    "iterations-implement-stories",
                    one_type("iteration"),
                    one_type("story"),
                    RelSelector::Named("implements".to_string()),
                ),
                chain_edge(
                    "iterations-target-stories",
                    one_type("iteration"),
                    one_type("story"),
                    RelSelector::Named("targets".to_string()),
                ),
            ],
            Vec::new(),
        ));

        assert_eq!(walk.child_types_for("story"), vec!["iteration".to_string()]);
    }

    #[test]
    fn a_wildcard_to_reports_its_from_types_as_every_types_children() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![chain_edge(
                "iterations-implement-anything",
                one_type("iteration"),
                TypeSelector::Any,
                RelSelector::Named("implements".to_string()),
            )],
            Vec::new(),
        ));

        assert_eq!(walk.child_types_for("story"), vec!["iteration".to_string()]);
        assert_eq!(walk.child_types_for("rfc"), vec!["iteration".to_string()]);
    }

    #[test]
    fn a_wildcard_from_names_no_types_so_it_yields_no_child_types() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![chain_edge(
                "anything-implements-stories",
                TypeSelector::Any,
                one_type("story"),
                RelSelector::Named("implements".to_string()),
            )],
            vec![chain_relationship("implements")],
        ));

        assert!(walk.child_types_for("story").is_empty());
    }

    #[test]
    fn a_blanket_relationship_yields_no_child_types() {
        let walk = TraversalWalk::from_config(&config_with(
            Vec::new(),
            vec![chain_relationship("implements")],
        ));

        assert!(walk.child_types_for("story").is_empty());
    }

    #[test]
    fn a_row_giving_a_relationship_the_related_role_yields_no_child_types() {
        let walk = TraversalWalk::from_config(&config_with(
            vec![edge(
                "iterations-relate-to-stories",
                one_type("iteration"),
                one_type("story"),
                RelSelector::Named("relates-to".to_string()),
                Traversal::Related,
            )],
            Vec::new(),
        ));

        assert!(walk.child_types_for("story").is_empty());
    }
}
