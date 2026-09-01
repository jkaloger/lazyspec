//! Three-surface parity for the edge-table walk: STORY-257 AC6, "given the same
//! document, when viewed via `context --json`, the TUI graph view, and the web
//! view, then all three render the same chain and neighbourhood".
//!
//! The fixture's config carries concrete-`from` `[[edges]]` rows and no global
//! `traversal` markers at all. That is the point of it: this repo's own
//! `.lazyspec.toml` declares no `[[edges]]`, so a parity check run against it
//! only ever exercises the blanket `[[relationships]]` fallback and proves
//! nothing about the table. Two documents in the shared fixture
//! ([`crate::common::walk_fixture`]) are here EXCLUDED -- `ADR-001`
//! (adr -related-to-> story) and `ADR-003` (adr -implements-> story). A wildcard
//! row or a global marker admits both, so any surface that re-derived the walk
//! instead of consuming the engine's would report a strictly larger set and fail
//! here. One document (`STORY-002`) makes the related row load-bearing: it is
//! reachable only by stepping the related arm of the walk off the chain, so no
//! amount of reading the subject's own frontmatter can produce it.
//!
//! The surfaces are reached through their own outermost interface, never through
//! each other (Principle 3): the CLI through `context`'s `--json` payload, the
//! TUI through `App::relation_sections`, the web view through a real request to
//! `GET /doc/{id}`. Each comparison is made for EVERY subject in the fixture, not
//! for `STORY-001` alone: every link in the subject's own neighbourhood was
//! declared by the document holding it, so a surface that re-derived the walk
//! from the link maps still agrees there, and only a subject whose descendant
//! inherited its link (`RFC-001`, whose child `ITERATION-002` declares nothing)
//! separates consuming the engine's walk from repeating it (ADR-034).
//!
//! The forest is a fourth rendering of the same walk, asserted at four places
//! rather than one: the engine seam every forest surface is meant to consume
//! ([`rendered_parent_edges`]), and then each surface's own emitted rows -- the
//! TUI's `App::graph_nodes` ([`tui_graph_parent_edges`]), the web `/graph`
//! response, and `context --json` with no id ([`cli_forest_parent_edges`]).
//! Pinning the seam alone leaves every one of them free to re-derive the edges
//! after the call, which is undetectable from the seam. The forest surfaces carry
//! the chain arm of the walk as tree edges; the related arm reaches the two graph
//! views as a per-row annotation, compared in the web module.

use crate::common::walk_fixture::{
    cli_neighbourhood, shorthand, sorted, walk_fixture, Neighbourhood, SUBJECT,
};
use crate::common::TestFixture;
use lazyspec::engine::config::{Config, EdgeDef, RelSelector, Traversal, TypeSelector};
use lazyspec::engine::context::{resolve_chain, resolve_forest};
use lazyspec::engine::graph::{flatten_forest, GraphSort};
use lazyspec::engine::store::Store;
use std::collections::BTreeMap;

fn one(name: &str) -> TypeSelector {
    TypeSelector::Types(vec![name.to_string()])
}

fn edge(name: &str, from: &str, to: &str, via: &str, traversal: Traversal) -> EdgeDef {
    EdgeDef {
        name: name.to_string(),
        from: one(from),
        to: one(to),
        via: RelSelector::Named(via.to_string()),
        required: None,
        traversal: Some(traversal),
    }
}

/// A relationship name with no global `traversal` marker, so the edge table is
/// the only thing that can put a relation on a walk.
fn unmarked(name: &str) -> lazyspec::engine::config::RelationshipDef {
    lazyspec::engine::config::RelationshipDef {
        name: name.to_string(),
        inverse: None,
        github_native: None,
        traversal: None,
    }
}

/// The edge table under test: two concrete chain rows and one concrete related
/// row, over relationships that carry no marker of their own.
fn edge_table_config() -> Config {
    Config {
        relationships: vec![
            unmarked("implements"),
            unmarked("related-to"),
            unmarked("blocks"),
        ],
        edges: vec![
            edge(
                "stories-implement-rfcs",
                "story",
                "rfc",
                "implements",
                Traversal::Chain,
            ),
            edge(
                "iterations-implement-stories",
                "iteration",
                "story",
                "implements",
                Traversal::Chain,
            ),
            edge(
                "stories-relate-to-rfcs",
                "story",
                "rfc",
                "related-to",
                Traversal::Related,
            ),
        ],
        ..Config::default()
    }
}

/// The shared fixture store read under the edge table above, loaded from a real
/// `.lazyspec.toml` so the rows are proven expressible in config and not just in
/// Rust.
fn edge_table_fixture() -> (TestFixture, Store, Config) {
    let fixture = walk_fixture();
    std::fs::write(
        fixture.root().join(".lazyspec.toml"),
        edge_table_config().to_toml().unwrap(),
    )
    .unwrap();

    let loaded = Config::load(fixture.root(), &lazyspec::engine::fs::RealFileSystem).unwrap();
    let store = Store::load(fixture.root(), &loaded).unwrap();
    (fixture, store, loaded)
}

/// Every document a surface can be asked about. All three address a document by
/// its bare id -- the TUI relations tab and the web route have no other handle on
/// one -- and only a root document resolves from a bare id, so the nested
/// inheritor `ITERATION-002` is not a subject here. That leaves it covered rather
/// than skipped: it is a chain descendant of `RFC-001`, which IS a subject, and
/// the descendant sets below are where it has to appear.
fn parity_subjects(store: &Store) -> Vec<String> {
    let mut ids: Vec<String> = store
        .all_docs()
        .iter()
        .filter(|doc| store.parent_of(&doc.path).is_none())
        .map(|doc| doc.id.clone())
        .collect();
    ids.sort();
    ids
}

/// The CLI's claim for every subject, keyed by the subject's id.
fn cli_neighbourhoods(store: &Store) -> BTreeMap<String, Neighbourhood> {
    parity_subjects(store)
        .into_iter()
        .map(|id| (id.clone(), cli_neighbourhood(store, &id)))
        .collect()
}

/// The TUI's claim for every subject, read off the relations tab's section model.
fn tui_neighbourhoods(store: Store, config: &Config) -> BTreeMap<String, Neighbourhood> {
    let app = lazyspec::tui::state::App::new(
        store,
        config,
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    let ids = |paths: Vec<std::path::PathBuf>| {
        sorted(
            paths
                .iter()
                .map(|p| app.store.get(p).unwrap().id.clone())
                .collect(),
        )
    };
    parity_subjects(&app.store)
        .into_iter()
        .map(|id| {
            let sections = app.relation_sections(app.store.resolve_shorthand(&id).unwrap());
            let neighbourhood = Neighbourhood {
                ancestors: ids(sections.chain),
                descendants: ids(sections.children),
                related: ids(sections.related),
            };
            (id, neighbourhood)
        })
        .collect()
}

/// The tree edge each graph row is drawn under, as `child id -> parent ids`: in
/// a flattened, depth-tagged row list a row at depth `d` hangs beneath the
/// nearest preceding row at depth `d - 1`. Every graph rendering in the project
/// is such a list, so this one reduction reads the drawn edges off any of them.
fn parent_edges_of(
    rows: impl IntoIterator<Item = (String, usize)>,
) -> BTreeMap<String, Vec<String>> {
    let mut edges: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut path_to_root: Vec<String> = Vec::new();

    for (id, depth) in rows {
        path_to_root.truncate(depth);
        let parents = edges.entry(id.clone()).or_default();
        if let Some(parent) = path_to_root.last() {
            if !parents.contains(parent) {
                parents.push(parent.clone());
            }
        }
        path_to_root.push(id);
    }

    edges.into_iter().map(|(id, ps)| (id, sorted(ps))).collect()
}

/// The forest as the ENGINE seam hands it over: `resolve_forest` +
/// `flatten_forest`, the pair both graph surfaces are supposed to consume. This
/// pins the seam only; what each surface does with it is pinned separately by
/// [`tui_graph_parent_edges`] and the web module's `web_graph_parent_edges`,
/// because a surface is free to re-derive the edges after the call and this
/// helper would never notice.
fn rendered_parent_edges(store: &Store) -> BTreeMap<String, Vec<String>> {
    let rows = flatten_forest(&resolve_forest(store, None), store, &GraphSort::default());
    parent_edges_of(
        rows.iter()
            .map(|row| (store.get(&row.path).unwrap().id.clone(), row.depth)),
    )
}

/// The edges the TUI graph view actually draws, read off `App::graph_nodes` --
/// the row list `tui/views/graph.rs` renders -- after the state transition the
/// view depends on (`rebuild_graph`), through the public state API (DICTUM-007).
fn tui_graph_parent_edges(store: Store, config: &Config) -> BTreeMap<String, Vec<String>> {
    let mut app = lazyspec::tui::state::App::new(
        store,
        config,
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    app.rebuild_graph();
    parent_edges_of(
        app.graph_nodes
            .iter()
            .map(|row| (app.store.get(&row.path).unwrap().id.clone(), row.depth)),
    )
}

/// The edges `context --json` with no id draws, read off `run_forest_json`'s
/// `implements_in_context` -- the CLI's own rendering of the forest, and the
/// fourth surface to consume it. Reduced straight to `child id -> parent ids`
/// rather than through [`parent_edges_of`], because the CLI emits each node's
/// parent paths outright instead of a depth-tagged row list.
///
/// Pinned for the same reason the two graph views are pinned separately from the
/// engine seam: the CLI is free to re-derive the edges after the `resolve_forest`
/// call, and [`rendered_parent_edges`] would never notice. Its only other
/// coverage runs under blanket and wildcard configs, where `walks_chain` is
/// symmetric in `from` and `to` and an endpoint swap is therefore invisible.
fn cli_forest_parent_edges(store: &Store) -> BTreeMap<String, Vec<String>> {
    let json = lazyspec::cli::context::run_forest_json(store, None).unwrap();
    let forest: serde_json::Value = serde_json::from_str(&json).unwrap();
    forest["forest"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            let parents = node["implements_in_context"]
                .as_array()
                .unwrap()
                .iter()
                .map(|path| {
                    store
                        .get(std::path::Path::new(path.as_str().unwrap()))
                        .unwrap()
                        .id
                        .clone()
                })
                .collect();
            (node["id"].as_str().unwrap().to_string(), sorted(parents))
        })
        .collect()
}

/// The same edges as the chain walk `context` runs gives them: for each
/// document, the parents recorded on its own node by `resolve_chain`.
fn walked_parent_edges(store: &Store) -> BTreeMap<String, Vec<String>> {
    store
        .all_docs()
        .iter()
        .map(|doc| {
            let resolved = resolve_chain(store, &shorthand(store, doc), 1).unwrap();
            let node = resolved
                .nodes
                .iter()
                .find(|n| n.doc.path == doc.path)
                .unwrap();
            let parents = node
                .parents
                .iter()
                .map(|p| store.get(p).unwrap().id.clone())
                .collect();
            (doc.id.clone(), sorted(parents))
        })
        .collect()
}

// The concrete rows bite: the sets below are what the table declares, computed
// from the rows by hand. Every id a wildcard row would add is named in the
// fixture and absent here.
#[test]
fn the_edge_table_walk_admits_only_the_triples_its_rows_declare() {
    let (_fixture, store, _config) = edge_table_fixture();

    assert_eq!(
        cli_neighbourhood(&store, SUBJECT),
        Neighbourhood {
            ancestors: vec!["RFC-001".to_string()],
            descendants: vec!["ITERATION-001".to_string()],
            related: vec![
                "ADR-002".to_string(),
                "RFC-002".to_string(),
                "STORY-002".to_string(),
            ],
        }
    );
}

#[test]
fn the_cli_and_the_tui_render_the_same_chain_and_neighbourhood_for_every_document() {
    let (_fixture, store, config) = edge_table_fixture();
    let expected = cli_neighbourhoods(&store);

    assert_eq!(
        tui_neighbourhoods(store, &config),
        expected,
        "the relations tab consumes the walk `context` runs; one that re-derived \
         either arm from the link maps would part company with it on a subject \
         whose descendant inherited its chain link"
    );
}

#[test]
fn the_graph_forest_draws_every_row_under_the_parents_the_chain_walk_gives_it() {
    let (_fixture, store, _config) = edge_table_fixture();

    assert_eq!(
        rendered_parent_edges(&store),
        walked_parent_edges(&store),
        "the graph forest's parent edges are the chain walk's parents: a forest \
         that derived them itself, rather than consuming `resolve_chain`'s, would \
         be free to hang a row somewhere `context` does not"
    );
}

#[test]
fn the_tui_graph_view_draws_every_row_under_the_parents_the_chain_walk_gives_it() {
    let (_fixture, store, config) = edge_table_fixture();
    let expected = walked_parent_edges(&store);

    assert_eq!(
        tui_graph_parent_edges(store, &config),
        expected,
        "the rows the graph view draws hang under the chain walk's parents: \
         `rebuild_graph` re-deriving them after the engine call -- asking \
         `walks_chain` with the from- and to-types swapped, say (ADR-034) -- \
         draws every document as a root while `context` still reports its parent"
    );
}

#[test]
fn the_cli_forest_json_draws_every_node_under_the_parents_the_chain_walk_gives_it() {
    let (_fixture, store, _config) = edge_table_fixture();

    assert_eq!(
        cli_forest_parent_edges(&store),
        walked_parent_edges(&store),
        "`context --json` with no id emits each node's parents as the chain walk \
         gives them: a payload re-deriving them after the engine call -- asking \
         `walks_chain` with the from- and to-types swapped, say (ADR-034) -- emits \
         every document as a root while `context <id>` still reports its parent"
    );
}

// A DELIBERATE, PRE-EXISTING disagreement, pinned here so no reader mistakes it
// for a case the parity tests above missed. `context`'s forward set reads the
// link maps, into which `propagate_parent_links` copies a parent's links for
// every nested child; the forest's parent edges read each document's own
// `related` frontmatter, which for such a child is empty. So an inheritor is a
// chain child of its parent's chain parent while having no chain parents of its
// own -- it is a forest ROOT. This predates the edge table and STORY-257 does not
// ask for it to change; both readings are the ones their own callers want, and
// reconciling them is a decision for whoever owns nested-document inheritance.
#[test]
fn a_nested_inheritor_is_a_forward_chain_child_yet_a_parentless_forest_root() {
    let (_fixture, store, _config) = edge_table_fixture();

    assert_eq!(
        cli_neighbourhood(&store, "RFC-001").descendants,
        vec![
            "ITERATION-002".to_string(),
            "STORY-001".to_string(),
            "STORY-003".to_string()
        ],
        "`context RFC-001` forward lists the inheritor ITERATION-002 beside the \
         two stories that declared the link themselves"
    );
    assert_eq!(
        rendered_parent_edges(&store).get("ITERATION-002"),
        Some(&Vec::new()),
        "and the graph forest draws it as a root, because its own `related` \
         declares no chain parent -- the disagreement is deliberate, not a gap in \
         the_graph_forest_draws_every_row_under_the_parents_the_chain_walk_gives_it"
    );
}

#[cfg(feature = "web")]
mod web {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use lazyspec::engine::issue_map::IssueMap;
    use lazyspec::web::server::{router, AppState, SharedStore};
    use std::sync::Arc;
    use tower::ServiceExt;

    /// The ids linked inside the doc page's `data-direction="{group}"` context
    /// group, or an empty list when the template omitted the group.
    fn linked_ids(html: &str, group: &str) -> Vec<String> {
        let opening = format!("data-direction=\"{group}\"");
        let Some(start) = html.find(&opening) else {
            return Vec::new();
        };
        let rest = &html[start..];
        let end = rest.find("</div>").unwrap_or(rest.len());
        sorted(
            rest[..end]
                .split("href=\"/doc/")
                .skip(1)
                .map(|tail| tail[..tail.find('"').unwrap()].to_string())
                .collect(),
        )
    }

    fn state_for(store: Store, config: &Config) -> AppState {
        AppState {
            store: SharedStore::new(store),
            config: Arc::new(config.clone()),
            coords: None,
            issue_map: Arc::new(IssueMap::default()),
            repo_name: "testrepo".into(),
            branch: None,
        }
    }

    async fn body_of(state: &AppState, uri: &str) -> String {
        let response = router(state.clone())
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    /// The web view's claim for every subject, each read off a real
    /// `GET /doc/{id}` response so the route's own resolution is what is under
    /// test.
    async fn web_neighbourhoods(store: Store, config: &Config) -> BTreeMap<String, Neighbourhood> {
        let subjects = parity_subjects(&store);
        let state = state_for(store, config);

        let mut rendered = BTreeMap::new();
        for id in subjects {
            let html = body_of(&state, &format!("/doc/{id}")).await;
            rendered.insert(
                id,
                Neighbourhood {
                    ancestors: linked_ids(&html, "ancestors"),
                    descendants: linked_ids(&html, "descendants"),
                    related: linked_ids(&html, "related"),
                },
            );
        }
        rendered
    }

    #[tokio::test]
    async fn the_web_doc_page_renders_the_same_chain_and_neighbourhood_as_the_cli_for_every_document(
    ) {
        let (_fixture, store, config) = edge_table_fixture();
        let expected = cli_neighbourhoods(&store);

        assert_eq!(
            web_neighbourhoods(store, &config).await,
            expected,
            "the doc page consumes the walk `context` runs; one that re-derived \
             either arm from the link maps would part company with it on a \
             subject whose descendant inherited its chain link"
        );
    }

    /// The rows the `/graph` page draws, in document order, as
    /// `(id, depth)` -- one per `<li data-id data-depth>` the graph-node template
    /// emits, which is the whole of the rendered tree.
    fn graph_rows(html: &str) -> Vec<(String, usize)> {
        html.split("<li data-id=\"")
            .skip(1)
            .map(|tail| {
                let (id, rest) = tail.split_once('"').unwrap();
                let (_, depth) = rest.split_once("data-depth=\"").unwrap();
                let (depth, _) = depth.split_once('"').unwrap();
                (id.to_string(), depth.parse().unwrap())
            })
            .collect()
    }

    /// The edges the web graph page actually draws, read off a real
    /// `GET /graph` response so the route's own forest is what is under test.
    async fn web_graph_parent_edges(
        store: Store,
        config: &Config,
    ) -> BTreeMap<String, Vec<String>> {
        let html = body_of(&state_for(store, config), "/graph").await;
        parent_edges_of(graph_rows(&html))
    }

    /// The chain walk's edges with the one key `/graph` spells differently.
    ///
    /// `web::render::GraphTreeNode::id_of` names a row from its markdown file's
    /// stem, so the fixture's folder-style document
    /// (`STORY-003-nesting-parent/index.md`) is drawn -- and linked -- as
    /// `index`, and `GET /doc/index` answers 404. Every other web surface names a
    /// document by the store's `id`. That is a DEFECT in how the page names a
    /// row, surfaced by this test and deliberately not fixed by it: STORY-257
    /// asks for traversal parity, and the row's EDGE is drawn correctly (under
    /// `RFC-001`, exactly where `context` puts it). Renaming the key rather than
    /// dropping the row keeps the comparison total -- all ten rows and every
    /// edge still have to match, so traversal drift is still caught -- and the
    /// rename is key-side only because the folder document is never drawn as
    /// another row's parent here.
    fn keyed_as_the_graph_page_spells_it(
        mut edges: BTreeMap<String, Vec<String>>,
    ) -> BTreeMap<String, Vec<String>> {
        let folder_doc = edges
            .remove("STORY-003")
            .expect("the fixture's folder-style document");
        edges.insert("index".to_string(), folder_doc);
        edges
    }

    /// The related-role annotation the TUI graph view draws on each row, read off
    /// the same `App::graph_nodes` list [`tui_graph_parent_edges`] reads its edges
    /// from. `GraphNode::related` is the node's own depth-1 cross-cutting set,
    /// which `tui/views/graph.rs` draws as `┄▷ <id>` beside the row -- the tree
    /// edges are the chain arm of the walk, this annotation is the related arm, and
    /// only comparing it puts the related arm of the graph surfaces under test.
    fn tui_graph_related(store: Store, config: &Config) -> BTreeMap<String, Vec<String>> {
        let mut app = lazyspec::tui::state::App::new(
            store,
            config,
            ratatui_image::picker::Picker::halfblocks(),
            Box::new(lazyspec::engine::fs::RealFileSystem),
        );
        app.rebuild_graph();
        app.graph_nodes
            .iter()
            .map(|row| {
                (
                    app.store.get(&row.path).unwrap().id.clone(),
                    sorted(row.related.clone()),
                )
            })
            .collect()
    }

    /// The related-role annotation the `/graph` page draws on each row, as
    /// `id -> annotated ids`: the links inside the row's `class="graph-related"`
    /// span, which is the whole of what `graph_node.html` emits for the
    /// cross-cutting set. A row carrying no annotation maps to an empty list, so
    /// the comparison covers every drawn row rather than only the annotated ones.
    fn web_graph_related(html: &str) -> BTreeMap<String, Vec<String>> {
        html.split("<li data-id=\"")
            .skip(1)
            .map(|tail| {
                let (id, rest) = tail.split_once('"').unwrap();
                let row = &rest[..rest.find("</li>").unwrap_or(rest.len())];
                let anno = match row.find("class=\"graph-related\"") {
                    Some(i) => sorted(
                        row[i..]
                            .split("href=\"/doc/")
                            .skip(1)
                            .map(|t| t[..t.find('"').unwrap()].to_string())
                            .collect(),
                    ),
                    None => Vec::new(),
                };
                (id.to_string(), anno)
            })
            .collect()
    }

    /// The two graph surfaces' RELATED arm, the counterpart to the chain-arm
    /// comparisons above: the tree edges pin `walks_chain`, and only this pins
    /// `walks_related` across the two drawn graphs. The fixture makes it
    /// non-vacuous -- the concrete `stories-relate-to-rfcs` row annotates
    /// `STORY-001` with `RFC-002` and leaves `ADR-001`'s inbound `related-to` off,
    /// so a surface re-deriving the annotation from the link maps reports a
    /// different set here rather than an empty one on both sides.
    ///
    /// Each surface gets its own load of the one fixture root because `App::new`
    /// and [`state_for`] both take a `Store` by value and `Store` is not `Clone`.
    /// The two loads read the same documents under the same config and the
    /// comparison is keyed by doc id, so the pair is one graph.
    #[tokio::test]
    async fn the_graph_surfaces_annotate_the_same_related_neighbours_on_every_row() {
        let (fixture, web_store, config) = edge_table_fixture();
        let tui_store = Store::load(fixture.root(), &config).unwrap();
        let expected = keyed_as_the_graph_page_spells_it(tui_graph_related(tui_store, &config));

        let html = body_of(&state_for(web_store, &config), "/graph").await;

        assert_eq!(
            web_graph_related(&html),
            expected,
            "both graph views annotate each row with the walk's related-role \
             neighbours: one that re-derived them after the engine call would \
             annotate a row the other leaves bare"
        );
    }

    #[tokio::test]
    async fn the_web_graph_page_draws_every_row_under_the_parents_the_chain_walk_gives_it() {
        let (_fixture, store, config) = edge_table_fixture();
        let expected = keyed_as_the_graph_page_spells_it(walked_parent_edges(&store));

        assert_eq!(
            web_graph_parent_edges(store, &config).await,
            expected,
            "the rows `/graph` draws hang under the chain walk's parents: a route \
             re-deriving them after the engine call -- asking `walks_chain` with \
             the from- and to-types swapped, say (ADR-034) -- draws every document \
             as a root while `context` still reports its parent"
        );
    }
}
