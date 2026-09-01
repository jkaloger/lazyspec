//! **The STORY-257 AC5 instrument.** AC5 reads "given any config migrated by
//! `fix --config`, when `context` runs before and after migration, then the
//! output is identical". `fix --config` migrates nothing until STORY-258, so
//! STORY-257 supplies the instrument and STORY-258 asserts the result against
//! it.
//!
//! The instrument is a pair of configs over the shared fixture store
//! ([`crate::common::walk_fixture`]):
//!
//! - [`pre_migration_config`] is **config A**, the legacy shape: `traversal`
//!   declared on `[[relationships]]`, no `[[edges]]` at all. This repo's own
//!   `.lazyspec.toml` is exactly this shape.
//! - [`post_migration_config`] is **config B**, the post-migration shape: no
//!   relationship carries `traversal`, and `[[edges]]` rows carry it instead.
//!
//! **`fix --config` (STORY-258) must turn A into B.** The transformation
//! between these two values is that migration's whole job, and
//! [`migrating_traversal_onto_a_wildcard_edge_table_leaves_context_identical`]
//! is the behaviour-preservation check it has to pass. STORY-258 should reuse
//! these two functions rather than writing a third pair.
//!
//! ## Config B's rows must be wildcards, not concrete pairs
//!
//! This is the crux of the migration and RFC-067 §"The traversal cost, stated
//! plainly" says it outright: a legacy `traversal = "chain"` sits on a
//! relationship NAME, so it is blanket over every type pair that relationship
//! ever joins. Its faithful edge-table equivalent is therefore a
//! wildcard-`from`/wildcard-`to` row. A concrete-`from` translation --
//! enumerating the pairs a config's documents happen to use today -- is NOT
//! behaviour-preserving, and
//! [`a_concrete_from_translation_of_a_blanket_marker_drops_documents`] shows
//! what it silently drops. STORY-258 AC4 already asks for the wildcard shape;
//! that test is the reason it cannot be relaxed.
//!
//! Two load-time rules constrain what a mechanical migration may emit here, and
//! both are satisfied by the rows below:
//!
//! - `required` on a `from = "*"` row is rejected at load (ADR-031,
//!   `config.rs`'s wildcard-source check). A blanket `traversal` marker carries
//!   no requiredness, so its translation must omit `required` — which is also
//!   what ADR-031 calls documentation-only, taking no part in requiredness
//!   resolution. The migration therefore cannot lose or invent a finding here.
//! - An `[[edges]]` row that states a traversal for a relationship suppresses
//!   that relationship's global marker (`traversal::TraversalWalk`). The two do
//!   not union, so config B must translate EVERY marked relationship or the
//!   unmarked remainder changes role.
//!
//! A relationship that carries no traversal at all (`blocks` here) translates to
//! no row, and must keep behaving as it does today: off both walks, yet still
//! surfaced by `merge_declared_related` as one of the subject's own declared
//! relations.
//!
//! ## `context --json` is not byte-stable, so "identical" is one step weaker
//!
//! STORY-258 will want to diff `--json` payloads and cannot: `context`'s
//! `forward` array is emitted in reverse-link insertion order, which comes off a
//! `HashMap` iteration in `Store::build_links`, so two reads of one unchanged
//! store under one unchanged config already disagree. [`canonical`] puts that
//! one array in path order and compares everything else verbatim; see its doc
//! comment for what that does and does not still catch. The defect is
//! pre-existing and no part of this story asks for it to be fixed.

use crate::common::walk_fixture::{
    cli_neighbourhood, shorthand, walk_fixture, Neighbourhood, SUBJECT,
};
use lazyspec::engine::config::{
    Config, EdgeDef, RelSelector, RelationshipDef, Traversal, TypeSelector,
};
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;
use std::collections::BTreeMap;
use std::path::Path;

/// The `related-to` depth `context` defaults to (`cli.rs`'s `--depth`), plus one
/// hop further, so a migration that changed how far the related walk reaches
/// cannot hide behind the default.
const DEPTHS: [usize; 2] = [1, 2];

fn relationship(name: &str, traversal: Option<Traversal>) -> RelationshipDef {
    RelationshipDef {
        name: name.to_string(),
        inverse: None,
        github_native: None,
        traversal,
    }
}

fn blanket_edge(name: &str, via: &str, traversal: Traversal) -> EdgeDef {
    EdgeDef {
        name: name.to_string(),
        from: TypeSelector::Any,
        to: TypeSelector::Any,
        via: RelSelector::Named(via.to_string()),
        required: None,
        traversal: Some(traversal),
    }
}

/// **Config A**: traversal declared on `[[relationships]]`, no `[[edges]]`.
pub fn pre_migration_config() -> Config {
    Config {
        relationships: vec![
            relationship("implements", Some(Traversal::Chain)),
            relationship("related-to", Some(Traversal::Related)),
            relationship("blocks", None),
        ],
        edges: vec![],
        ..Config::default()
    }
}

/// **Config B**: the same two walks declared as wildcard `[[edges]]` rows, with
/// every global marker removed. `blocks` carried no marker, so it gets no row.
pub fn post_migration_config() -> Config {
    Config {
        relationships: vec![
            relationship("implements", None),
            relationship("related-to", None),
            relationship("blocks", None),
        ],
        edges: vec![
            blanket_edge("implements-walks-the-chain", "implements", Traversal::Chain),
            blanket_edge(
                "related-to-walks-the-neighbourhood",
                "related-to",
                Traversal::Related,
            ),
        ],
        ..Config::default()
    }
}

/// The translation a migration would produce if it enumerated the type pairs the
/// fixture's documents happen to use, instead of emitting wildcards. It reads as
/// the more precise config, and it is -- precise about pairs nobody declared it
/// should exclude.
fn concrete_pair_translation() -> Config {
    let edge = |name: &str, from: &str, to: &str, via: &str, traversal: Traversal| EdgeDef {
        name: name.to_string(),
        from: TypeSelector::Types(vec![from.to_string()]),
        to: TypeSelector::Types(vec![to.to_string()]),
        via: RelSelector::Named(via.to_string()),
        required: None,
        traversal: Some(traversal),
    };
    Config {
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
        ..post_migration_config()
    }
}

/// The shared fixture's documents read under one config, written into the store
/// root the way `fix --config` would rewrite it in place.
fn store_under(root: &Path, config: &Config) -> Store {
    std::fs::write(root.join(".lazyspec.toml"), config.to_toml().unwrap()).unwrap();
    let loaded = Config::load(root, &RealFileSystem).unwrap();
    Store::load(root, &loaded).unwrap()
}

/// A `context --json` payload with its `forward` records put in path order.
///
/// Everything else is compared verbatim. `forward` alone is reordered because
/// `context` does not currently emit it in a fixed order: it is the reverse link
/// list in the order `Store::build_links` pushed onto it, and that loop iterates
/// a `HashMap`, so two loads of ONE unchanged store and ONE unchanged config
/// disagree on it (pinned by
/// [`the_instrument_measures_the_walk_and_not_the_order_links_were_hashed_into`]).
/// That is a pre-existing determinism defect in `context`, not something the
/// migration does, and this instrument declines to report it as one. Sorting
/// keeps every field of every record -- so a migration that renumbered a
/// `distance`, changed a `relation`, moved a `via`, or moved a document between
/// `forward` and `related` is still caught -- and the `chain` array, whose order
/// IS the hierarchy, is left exactly as emitted.
fn canonical(payload: &str) -> String {
    let mut value: serde_json::Value = serde_json::from_str(payload).unwrap();
    if let Some(forward) = value.get_mut("forward").and_then(|v| v.as_array_mut()) {
        forward.sort_by_key(|record| record["path"].as_str().unwrap().to_string());
    }
    serde_json::to_string_pretty(&value).unwrap()
}

/// Every `context` payload the store can produce, keyed by the invocation that
/// produced it: one per document per depth, plus both forest modes.
///
/// "Identical" is taken at the strongest reading the command's own determinism
/// allows -- the whole `--json` payload, over every document rather than one
/// subject, rather than the id sets the surface-parity tests compare. Id sets
/// would pass a migration that reordered a chain, renumbered a `distance`, or
/// moved a document from `forward` to `related`. The config is written into the
/// SAME root each time, exactly as `fix --config` would rewrite it in place, so
/// the paths embedded in the payload are common to both sides and a difference
/// can only be a difference of walk.
fn context_outputs(root: &Path, config: &Config) -> BTreeMap<String, String> {
    let store = store_under(root, config);
    let mut outputs = BTreeMap::new();
    for doc in store.all_docs() {
        let id = shorthand(&store, doc);
        for depth in DEPTHS {
            outputs.insert(
                format!("context {id} --depth {depth} --json"),
                canonical(&lazyspec::cli::context::run_json(&store, &id, depth).unwrap()),
            );
        }
    }
    outputs.insert(
        "context --json".to_string(),
        canonical(&lazyspec::cli::context::run_forest_json(&store, None).unwrap()),
    );
    outputs.insert(
        "context --anchor story --json".to_string(),
        canonical(&lazyspec::cli::context::run_forest_json(&store, Some("story")).unwrap()),
    );
    outputs
}

fn neighbourhood_of(root: &Path, config: &Config, subject: &str) -> Neighbourhood {
    cli_neighbourhood(&store_under(root, config), subject)
}

fn owned(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|i| i.to_string()).collect()
}

/// What the blanket markers actually reach, written out by hand from config A's
/// markers. Without this the equivalence could hold over two empty walks.
#[test]
fn the_pre_migration_config_walks_every_type_pair_its_global_markers_blanket() {
    let fixture = walk_fixture();

    assert_eq!(
        neighbourhood_of(fixture.root(), &pre_migration_config(), SUBJECT),
        Neighbourhood {
            ancestors: owned(&["RFC-001"]),
            // ADR-003 is here only because `implements` is marked blanket: no
            // rule anywhere says an adr may be a story's chain child.
            descendants: owned(&["ADR-003", "ITERATION-001"]),
            // ADR-001 for the same reason on the related walk; ADR-002 through
            // `blocks`, which is on neither walk; STORY-002 by stepping the
            // related arm off the chain parent.
            related: owned(&["ADR-001", "ADR-002", "RFC-002", "STORY-002"]),
        }
    );
}

/// AC5's claim, over the pair `fix --config` must implement.
#[test]
fn migrating_traversal_onto_a_wildcard_edge_table_leaves_context_identical() {
    let fixture = walk_fixture();

    let before = context_outputs(fixture.root(), &pre_migration_config());
    let after = context_outputs(fixture.root(), &post_migration_config());

    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "the migration must leave the set of addressable documents alone"
    );
    for (invocation, before_output) in &before {
        assert_eq!(
            after.get(invocation).map(String::as_str),
            Some(before_output.as_str()),
            "`{invocation}` is not behaviour-preserved by the migration"
        );
    }
}

/// The finding STORY-258 cannot afford to miss: enumerating concrete pairs
/// instead of emitting wildcards loses documents from `context`, silently.
#[test]
fn a_concrete_from_translation_of_a_blanket_marker_drops_documents() {
    let fixture = walk_fixture();
    let blanket = neighbourhood_of(fixture.root(), &pre_migration_config(), SUBJECT);

    let concrete = neighbourhood_of(fixture.root(), &concrete_pair_translation(), SUBJECT);

    assert_eq!(
        concrete,
        Neighbourhood {
            ancestors: owned(&["RFC-001"]),
            descendants: owned(&["ITERATION-001"]),
            related: owned(&["ADR-002", "RFC-002", "STORY-002"]),
        },
        "a concrete-pair translation drops ADR-003 from the chain and ADR-001 \
         from the neighbourhood: both are adr-sourced pairs the blanket marker \
         admitted and no enumerated row names"
    );
    assert_ne!(
        concrete, blanket,
        "if these ever agree the fixture has stopped exercising the wildcard \
         question and this test proves nothing"
    );
}

/// The instrument's own calibration: read twice under ONE config, the payloads
/// must agree. Without this a green equivalence above could mean nothing was
/// being compared but noise, and if [`canonical`] ever stops covering
/// `context`'s unordered output this fails first and names itself rather than
/// making the equivalence flaky.
#[test]
fn the_instrument_measures_the_walk_and_not_the_order_links_were_hashed_into() {
    let fixture = walk_fixture();

    let first = context_outputs(fixture.root(), &pre_migration_config());
    let second = context_outputs(fixture.root(), &pre_migration_config());

    for (invocation, payload) in &first {
        assert_eq!(
            second.get(invocation).map(String::as_str),
            Some(payload.as_str()),
            "`{invocation}` differs between two reads of one unchanged config, so \
             the comparison this file makes is not a comparison of walks"
        );
    }
}
