use crate::engine::sequencing::{Graph, LayeredLayout, NodeRef, Scope};
use crate::engine::store::Store;
use std::collections::HashSet;

/// Which scope command initiated the in-progress text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeInputMode {
    Under,
    After,
}

/// TUI state for the Sequencing view.
///
/// Holds the layered layout of the docs DAG plus the current scope filter
/// (`Under` / `After` / `All`) and the membership set used by the renderer
/// to dim out-of-scope nodes. Edits are deferred (STORY-119); this state is
/// read-only relative to the underlying `Store`.
pub struct SequencingState {
    pub scope: Scope,
    pub layout: LayeredLayout,
    pub in_scope: HashSet<NodeRef>,
    /// In-progress scope input: which mode was opened plus the buffered text.
    pub scope_input: Option<(ScopeInputMode, String)>,
    /// User-visible rejection error (e.g. iteration id rejected).
    pub error: Option<String>,
    /// User-visible info message (e.g. "read-only screen").
    pub info: Option<String>,
}

impl SequencingState {
    pub fn rebuild(store: &Store, scope: Scope) -> Self {
        let graph = Graph::from_store(store);
        let layout = graph.layered_layout();
        let in_scope = graph.scope_membership(&scope);
        SequencingState {
            scope,
            layout,
            in_scope,
            scope_input: None,
            error: None,
            info: None,
        }
    }

    /// Set scope to `Under(id)`. Rejects iteration ids; on rejection, the
    /// previous scope state is preserved and `error` carries a user-visible
    /// message.
    pub fn set_scope_under(&mut self, id: &str, store: &Store) {
        let graph = Graph::from_store(store);
        if graph.is_iteration(id) {
            self.error = Some(format!(
                "Cannot scope to iteration '{}'; pick a story or RFC instead",
                id
            ));
            return;
        }
        self.apply(graph, Scope::Under(id.to_string()));
    }

    /// Set scope to `After(id)`. Rejects iteration ids; on rejection, the
    /// previous scope state is preserved and `error` carries a user-visible
    /// message.
    pub fn set_scope_after(&mut self, id: &str, store: &Store) {
        let graph = Graph::from_store(store);
        if graph.is_iteration(id) {
            self.error = Some(format!(
                "Cannot scope to iteration '{}'; pick a story or RFC instead",
                id
            ));
            return;
        }
        self.apply(graph, Scope::After(id.to_string()));
    }

    pub fn clear_scope(&mut self, store: &Store) {
        let graph = Graph::from_store(store);
        self.apply(graph, Scope::All);
    }

    fn apply(&mut self, graph: Graph, scope: Scope) {
        let layout = graph.layered_layout();
        let in_scope = graph.scope_membership(&scope);
        self.scope = scope;
        self.layout = layout;
        self.in_scope = in_scope;
        self.error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::Config;
    use std::path::Path;
    use tempfile::TempDir;

    fn write(root: &Path, rel: &str, body: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn make_store() -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
        std::fs::create_dir_all(root.join("docs/stories")).unwrap();
        std::fs::create_dir_all(root.join("docs/iterations")).unwrap();

        // RFC-1 — root
        write(
            root,
            "docs/rfcs/RFC-1-root.md",
            "---\ntitle: \"Root\"\ntype: rfc\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\n---\n",
        );
        // STORY-1 implements RFC-1
        write(
            root,
            "docs/stories/STORY-1-feat.md",
            "---\ntitle: \"Feat\"\ntype: story\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\npriority: should\nrelated:\n- implements: RFC-1\n---\n",
        );
        // ITERATION-1 implements STORY-1
        write(
            root,
            "docs/iterations/ITERATION-1-impl.md",
            "---\ntitle: \"Impl\"\ntype: iteration\nstatus: draft\nauthor: \"t\"\ndate: 2026-01-01\ntags: []\npriority: should\nrelated:\n- implements: STORY-1\n---\n",
        );

        let cfg = Config::default();
        let store = Store::load(root, &cfg).unwrap();
        (tmp, store)
    }

    /// AC5: with no scope set (Scope::All), every node placed in the layout
    /// is in the scope membership set, so the renderer dims none of them.
    #[test]
    fn rebuild_with_scope_all_marks_every_layout_node_in_scope() {
        let (_tmp, store) = make_store();
        let state = SequencingState::rebuild(&store, Scope::All);

        assert_eq!(state.scope, Scope::All);
        assert!(state.error.is_none());

        let mut total = 0usize;
        for layer in &state.layout.layers {
            for node in layer {
                assert!(
                    state.in_scope.contains(node),
                    "node {:?} missing from in_scope under Scope::All",
                    node
                );
                total += 1;
            }
        }
        assert!(total > 0, "expected layout to contain placed nodes");
    }

    /// AC6: scope anchor pointing at an iteration is rejected. Prior scope is
    /// preserved and a user-readable error is set.
    #[test]
    fn set_scope_under_rejects_iteration_id_and_preserves_prior_scope() {
        let (_tmp, store) = make_store();
        let mut state = SequencingState::rebuild(&store, Scope::Under("STORY-1".to_string()));
        let prior_scope = state.scope.clone();
        let prior_in_scope = state.in_scope.clone();

        state.set_scope_under("ITERATION-1", &store);

        assert_eq!(state.scope, prior_scope, "scope must not change");
        assert_eq!(state.in_scope, prior_in_scope, "membership must not change");
        let err = state.error.as_ref().expect("expected error message");
        assert!(
            err.to_lowercase().contains("iteration"),
            "error should mention iteration: {:?}",
            err
        );
    }

    /// AC6: same rule applies to `After`.
    #[test]
    fn set_scope_after_rejects_iteration_id_and_preserves_prior_scope() {
        let (_tmp, store) = make_store();
        let mut state = SequencingState::rebuild(&store, Scope::All);
        let prior_scope = state.scope.clone();
        let prior_in_scope = state.in_scope.clone();

        state.set_scope_after("ITERATION-1", &store);

        assert_eq!(state.scope, prior_scope);
        assert_eq!(state.in_scope, prior_in_scope);
        let err = state.error.as_ref().expect("expected error message");
        assert!(err.to_lowercase().contains("iteration"));
    }

    #[test]
    fn set_scope_under_accepts_story_id_and_updates_membership() {
        let (_tmp, store) = make_store();
        let mut state = SequencingState::rebuild(&store, Scope::All);

        state.set_scope_under("STORY-1", &store);

        assert_eq!(state.scope, Scope::Under("STORY-1".to_string()));
        assert!(state.error.is_none());
        assert!(state.in_scope.contains(&NodeRef("STORY-1".to_string())));
    }

    #[test]
    fn clear_scope_resets_to_all_and_clears_error() {
        let (_tmp, store) = make_store();
        let mut state = SequencingState::rebuild(&store, Scope::Under("STORY-1".to_string()));
        state.set_scope_under("ITERATION-1", &store);
        assert!(state.error.is_some());

        state.clear_scope(&store);

        assert_eq!(state.scope, Scope::All);
        assert!(state.error.is_none());
    }
}
