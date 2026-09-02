use crate::engine::config::{Config, EdgeDef, Traversal};
use crate::engine::document::{DocMeta, RelationType};
use crate::engine::store::{Link, Store};
use std::path::Path;

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
/// Two declarations feed this and they deliberately do NOT union (ADR-035). An
/// `[[edges]]` row that states a `traversal` for relationship X decides X's
/// walk membership outright, suppressing `RelationshipDef.traversal` for X; a
/// relationship no row states a traversal for keeps its global marker as a
/// blanket fallback. Unioning would make the whole change inert wherever it
/// matters most: this project marks `targets` chain globally, so a union would
/// keep every `targets` link hierarchy no matter what the table said. Findings
/// from two rows stack; a walk cannot, because a triple is on one walk or the
/// other and not both.
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
///
/// Suppression is keyed by relationship NAME, which makes it broader than a row's
/// own selectors suggest: a row with `via = "*"` and any `traversal` suppresses
/// the global marker of EVERY declared relationship, with no load-time diagnostic
/// (ADR-035 §Consequences records the hazard and leaves the check to STORY-259/261).
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
    /// relationship and no types whatsoever. A config that wants an answer here
    /// states the pairs as concrete rows.
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

/// One document joined to a subject by a link the config gives a traversal
/// role, carrying the whole resolved neighbour so a caller needs no second
/// lookup to name, place or render it.
#[derive(Debug)]
pub(crate) struct Neighbour<'a> {
    pub relation: &'a RelationType,
    pub doc: &'a DocMeta,
}

/// The related-role neighbours of the document at `path`, forward links first
/// then reverse, each asked of [`TraversalWalk::walks_related`].
///
/// Direction is the whole reason this lives here rather than at its two call
/// sites (`resolve_chain`'s related BFS and the graph view's annotations). Every
/// link is asked as the document that DECLARED it stated it: `from` is
/// [`Link::declared_by`]'s type and `to` is the type at the far end of that
/// declaration, whichever end the caller happens to be standing on (ADR-034).
/// So reading a link backwards asks the same triple as reading it forwards, and
/// a link a child inherited from its parent asks the parent's triple -- both ends
/// of one declared link are neighbours of each other or neither is, an invariant
/// that only holds while one function decides it. [`chain_children`] reads the
/// same link maps in the same direction for the other role.
///
/// A link whose far end resolves to no document is not a neighbour. `to` is a
/// type selector, and a dangling target has no type, so no row naming a concrete
/// `to` could be shown to admit it; asking under a placeholder type name would
/// let wildcard rows admit it by accident and concrete rows reject it, which is
/// membership decided by a type that does not exist. `validate` reports the
/// broken link; the walk declines to invent a node for it.
///
/// THE RESULT MAY REPEAT A DOCUMENT. One declared link reaches both link maps, so
/// a pair that states the relation at both ends appears twice, as does a link a
/// nested child inherited when the subject is at its far end (once for the parent,
/// once for the child). Every caller already dedupes -- `resolve_chain`'s BFS on
/// first discovery, `graph::related_annotations` into a `BTreeSet` -- so this
/// returns the raw per-link claims rather than picking one of two identical
/// neighbours and discarding the other's relation type.
pub(crate) fn related_neighbours<'a>(store: &'a Store, path: &Path) -> Vec<Neighbour<'a>> {
    let Some(subject) = store.get(path) else {
        return Vec::new();
    };
    let walk = &store.traversal_walk;

    let forward = store
        .forward_links_for(path)
        .iter()
        .filter_map(|link| declared_edge(store, link))
        .filter(|(link, declarer, target)| {
            walk.walks_related(
                declarer.doc_type.as_str(),
                link.rel_type.as_str(),
                target.doc_type.as_str(),
            )
        });
    // `to` is the SUBJECT's type rather than anything read off the link, and that
    // is the declaration's far end only because `reverse_links` is keyed by the
    // declaration's target: an own reverse entry is filed under the target, and a
    // propagated one under the PARENT's declared target (`propagate_parent_links`
    // lends forward links only, so an inheriting child never has a reverse entry
    // filed under it). Should propagation ever lend a parent's INBOUND links to
    // its children too, a child's reverse list would hold declarations aimed at
    // the parent and `to` would have to be resolved from `declared_by`'s own
    // `related` entry instead of from the subject.
    let reverse = store
        .reverse_links_for(path)
        .iter()
        .filter_map(|link| declared_edge(store, link))
        .filter(|(link, declarer, _)| {
            walk.walks_related(
                declarer.doc_type.as_str(),
                link.rel_type.as_str(),
                subject.doc_type.as_str(),
            )
        });

    forward
        .chain(reverse)
        .map(|(link, _, doc)| Neighbour {
            relation: &link.rel_type,
            doc,
        })
        .collect()
}

/// The chain children of the document at `path`: every document holding a link
/// whose far end is `path` and whose triple [`TraversalWalk::walks_chain`]
/// admits. Only the reverse map is read, because a document's own chain PARENTS
/// come from its declared `related` (see `context::chain_parents`) and never from
/// the link maps.
///
/// The triple is asked exactly as [`related_neighbours`] asks it -- `from` is the
/// declaring document's type, `to` the type at the far end of that declaration --
/// so a chain row naming a concrete `from` admits the copies of its links that
/// nested children inherited, and a row naming the inheritor's type admits
/// neither end (ADR-034). Deriving `from` from the link's own endpoint instead
/// would drop an inheriting child out of the chain that a blanket marker keeps,
/// which is why this hop lives here beside the related one rather than at its
/// call site.
pub(crate) fn chain_children<'a>(store: &'a Store, path: &Path) -> Vec<Neighbour<'a>> {
    let Some(subject) = store.get(path) else {
        return Vec::new();
    };
    let walk = &store.traversal_walk;

    // `to` is the subject's type on the same invariant [`related_neighbours`]
    // stands on: `reverse_links` is keyed by the declaration's target, so the
    // subject IS that target -- including for a link a nested child inherited,
    // which is filed under the parent's declared target because propagation lends
    // forward links only.
    store
        .reverse_links_for(path)
        .iter()
        .filter_map(|link| declared_edge(store, link))
        .filter(|(link, declarer, _)| {
            walk.walks_chain(
                declarer.doc_type.as_str(),
                link.rel_type.as_str(),
                subject.doc_type.as_str(),
            )
        })
        .map(|(link, _, doc)| Neighbour {
            relation: &link.rel_type,
            doc,
        })
        .collect()
}

/// The link with both of the documents deciding its triple resolved: the one
/// that declared it, and the one at its far end. `None` when either is absent
/// from the store, since a triple cannot be asked about a type that is not there.
fn declared_edge<'a>(
    store: &'a Store,
    link: &'a Link,
) -> Option<(&'a Link, &'a DocMeta, &'a DocMeta)> {
    let declarer = store.get(&link.declared_by)?;
    let endpoint = store.get(&link.endpoint)?;
    Some((link, declarer, endpoint))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{RelSelector, RelationshipDef, TypeSelector};
    use crate::engine::store::test_support::{
        doc_md, store_from_with_config, stories_mention_rfcs,
    };

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
                RelSelector::Named(vec!["targets".to_string()]),
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
                RelSelector::Named(vec!["targets".to_string()]),
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
            RelSelector::Named(vec!["targets".to_string()]),
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
                RelSelector::Named(vec!["related-to".to_string()]),
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
                RelSelector::Named(vec!["related-to".to_string()]),
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
                RelSelector::Named(vec!["related-to".to_string()]),
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
                    RelSelector::Named(vec!["implements".to_string()]),
                ),
                chain_edge(
                    "stories-implement-rfcs",
                    one_type("story"),
                    one_type("rfc"),
                    RelSelector::Named(vec!["implements".to_string()]),
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
                    RelSelector::Named(vec!["implements".to_string()]),
                ),
                chain_edge(
                    "iterations-target-stories",
                    one_type("iteration"),
                    one_type("story"),
                    RelSelector::Named(vec!["targets".to_string()]),
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
                RelSelector::Named(vec!["implements".to_string()]),
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
                RelSelector::Named(vec!["implements".to_string()]),
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
                RelSelector::Named(vec!["relates-to".to_string()]),
                Traversal::Related,
            )],
            Vec::new(),
        ));

        assert!(walk.child_types_for("story").is_empty());
    }

    // --- related_neighbours: the direction wiring ---------------------------

    /// Each neighbour as (relationship, doc id), the whole resolved claim,
    /// sorted so the assertion pins the set and not the map iteration order.
    fn neighbours_at(store: &Store, path: &str) -> Vec<(String, String)> {
        let mut neighbours: Vec<(String, String)> = related_neighbours(store, Path::new(path))
            .into_iter()
            .map(|n| (n.relation.to_string(), n.doc.id.clone()))
            .collect();
        neighbours.sort();
        neighbours
    }

    fn neighbours_of(store: &Store, id: &str) -> Vec<(String, String)> {
        let path = store.resolve_shorthand(id).unwrap().path.clone();
        neighbours_at(store, &path.to_string_lossy())
    }

    /// STORY-001 -mentions-> RFC-001 is the row's triple; RFC-002 -mentions->
    /// STORY-002 is the same relationship declared the other way round, which no
    /// row covers.
    fn mentions_both_ways() -> (tempfile::TempDir, Store) {
        store_from_with_config(
            &[
                (
                    "docs/stories/STORY-001-a.md",
                    &doc_md("A", "story", "- mentions: RFC-001"),
                ),
                ("docs/rfcs/RFC-001-b.md", &doc_md("B", "rfc", "[]")),
                (
                    "docs/rfcs/RFC-002-c.md",
                    &doc_md("C", "rfc", "- mentions: STORY-002"),
                ),
                ("docs/stories/STORY-002-d.md", &doc_md("D", "story", "[]")),
            ],
            &stories_mention_rfcs(),
        )
    }

    #[test]
    fn a_forward_link_is_asked_with_the_subject_as_from() {
        let (_tmp, store) = mentions_both_ways();

        assert_eq!(
            neighbours_of(&store, "STORY-001"),
            vec![("mentions".to_string(), "RFC-001".to_string())]
        );
        assert!(
            neighbours_of(&store, "RFC-002").is_empty(),
            "rfc -mentions-> story is not the declared triple"
        );
    }

    #[test]
    fn a_reverse_link_is_asked_with_the_neighbour_as_from() {
        let (_tmp, store) = mentions_both_ways();

        assert_eq!(
            neighbours_of(&store, "RFC-001"),
            vec![("mentions".to_string(), "STORY-001".to_string())],
            "reading the link backwards still asks story -mentions-> rfc"
        );
        assert!(
            neighbours_of(&store, "STORY-002").is_empty(),
            "the undeclared triple is not a neighbour from either end"
        );
    }

    #[test]
    fn a_link_whose_target_resolves_to_no_document_is_not_a_neighbour() {
        // A blanket `related-to` marker names no types, so nothing but the
        // missing target itself can exclude the dangling neighbour.
        let (_tmp, store) = store_from_with_config(
            &[(
                "docs/rfcs/RFC-001-a.md",
                &doc_md("A", "rfc", "- related-to: NOPE-001"),
            )],
            &Config::default(),
        );

        assert!(neighbours_of(&store, "RFC-001").is_empty());
    }

    // --- related_neighbours: links a child inherits from its parent ---------

    const PARENT: &str = "docs/stories/STORY-001-parent/index.md";
    const INHERITING_CHILD: &str = "docs/stories/STORY-001-parent/CHILD-001.md";
    const MENTIONED: &str = "docs/rfcs/RFC-001-b.md";

    /// The parent `index.md` declares `mentions: RFC-001`; the nested child
    /// beside it declares nothing and inherits the link through
    /// [`Store::propagate_parent_links`]. The two are deliberately different
    /// types, which is what makes the row's `from` selector able to tell the
    /// declaring document from the inheriting one.
    fn parent_declares_child_inherits(from: TypeSelector) -> (tempfile::TempDir, Store) {
        let config = Config {
            relationships: Vec::new(),
            edges: vec![edge(
                "mentions-rfcs",
                from,
                one_type("rfc"),
                RelSelector::Named(vec!["mentions".to_string()]),
                Traversal::Related,
            )],
            ..Config::default()
        };
        store_from_with_config(
            &[
                (PARENT, &doc_md("Parent", "story", "- mentions: RFC-001")),
                (INHERITING_CHILD, &doc_md("Child", "iteration", "[]")),
                (MENTIONED, &doc_md("Mentioned", "rfc", "[]")),
            ],
            &config,
        )
    }

    #[test]
    fn an_inherited_link_is_asked_with_the_declaring_parents_type_as_from() {
        let (_tmp, store) = parent_declares_child_inherits(one_type("story"));

        assert_eq!(
            neighbours_at(&store, INHERITING_CHILD),
            vec![("mentions".to_string(), "RFC-001".to_string())],
            "the parent stated the relation, so the parent's edge is the one that admits it"
        );
        assert_eq!(
            neighbours_at(&store, MENTIONED),
            vec![
                ("mentions".to_string(), "CHILD-001".to_string()),
                ("mentions".to_string(), "STORY-001".to_string()),
            ],
            "read backwards, an inherited link asks the same triple as the parent's own"
        );
    }

    #[test]
    fn an_inherited_link_is_not_asked_with_the_inheriting_childs_type_as_from() {
        let (_tmp, store) = parent_declares_child_inherits(one_type("iteration"));

        assert!(
            neighbours_at(&store, INHERITING_CHILD).is_empty(),
            "iteration -mentions-> rfc is an edge no document declared"
        );
        assert!(
            neighbours_at(&store, MENTIONED).is_empty(),
            "and it admits neither end of the link, from either side"
        );
        assert!(
            neighbours_at(&store, PARENT).is_empty(),
            "a row naming only some other type admits the declared link nowhere"
        );
    }

    #[test]
    fn with_no_rows_a_parents_blanket_related_link_still_reaches_its_child() {
        let (_tmp, store) = store_from_with_config(
            &[
                (PARENT, &doc_md("Parent", "story", "- related-to: RFC-001")),
                (INHERITING_CHILD, &doc_md("Child", "iteration", "[]")),
                (MENTIONED, &doc_md("Mentioned", "rfc", "[]")),
            ],
            &Config::default(),
        );

        assert_eq!(
            neighbours_at(&store, INHERITING_CHILD),
            vec![("related-to".to_string(), "RFC-001".to_string())],
            "a global marker names no types, so no reading of `from` can change the answer"
        );
    }
}
