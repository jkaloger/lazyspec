//! The document set both traversal-walk suites read, and the reduction they
//! both compare.
//!
//! [`surface_parity_test`] asks whether the three surfaces agree on one
//! document's chain and neighbourhood; [`config_migration_preserves_context_test`]
//! asks whether a blanket `[[relationships]]` marker and its wildcard
//! `[[edges]]` translation agree on `context`'s whole output. Both questions need
//! the same graph -- a chain parent and child, a declared related peer, an
//! inbound related peer reachable only off the chain frontier, a relation on
//! neither walk, a cross-type pair a wildcard row admits and a concrete-`from`
//! row excludes, and a nested child inheriting its parent's chain link -- and
//! neither question is asked of the fixture's config, which each suite writes
//! itself. The expected sets stay in the suites, written out by hand there, so
//! adding a document here is meant to break both.
//!
//! [`surface_parity_test`]: crate::surface_parity_test
//! [`config_migration_preserves_context_test`]: crate::config_migration_preserves_context_test

use super::TestFixture;
use lazyspec::engine::document::DocMeta;
use lazyspec::engine::store::Store;

/// The document whose chain and neighbourhood every assertion is about. It has a
/// chain parent, a chain child, a related peer it declares, an inbound related
/// peer, a marker-less relation, and two cross-type documents pointing at it.
pub const SUBJECT: &str = "STORY-001";

pub fn doc(title: &str, doc_type: &str, related: &[&str]) -> String {
    let related_block = if related.is_empty() {
        "related: []".to_string()
    } else {
        format!("related:\n{}", related.join("\n"))
    };
    format!(
        "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: draft\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{related_block}\n---\n\n{title} body\n"
    )
}

/// Write the fixture's documents into a fresh store root. The config is the
/// caller's to write, and may be rewritten in place afterwards: no document here
/// names a relationship role, so one document set can be read under any number of
/// configs.
pub fn walk_fixture() -> TestFixture {
    let fixture = TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-chain-parent.md",
        &doc("Chain parent", "rfc", &[]),
    );
    fixture.write_doc(
        "docs/rfcs/RFC-002-related-peer.md",
        &doc("Related peer", "rfc", &[]),
    );
    fixture.write_doc(
        "docs/stories/STORY-001-subject.md",
        &doc(
            "Subject",
            "story",
            &[
                "- implements: RFC-001",
                "- related-to: RFC-002",
                "- blocks: ADR-002",
            ],
        ),
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-chain-child.md",
        &doc("Chain child", "iteration", &["- implements: STORY-001"]),
    );
    // adr -related-to-> story: on the related walk under a blanket marker or a
    // wildcard row, off it under a row naming concrete `from` types.
    fixture.write_doc(
        "docs/adrs/ADR-001-inbound-related.md",
        &doc("Inbound related", "adr", &["- related-to: STORY-001"]),
    );
    // Reached only by `blocks`, which no row and no marker in either suite covers.
    fixture.write_doc(
        "docs/adrs/ADR-002-marker-less-target.md",
        &doc("Marker-less target", "adr", &[]),
    );
    // adr -implements-> story: the same divide on the chain walk.
    fixture.write_doc(
        "docs/adrs/ADR-003-chain-child-by-adr.md",
        &doc("Chain child by adr", "adr", &["- implements: STORY-001"]),
    );
    // The one document only the walk's related arm can reach: it points at the
    // subject's chain PARENT, so it is found by stepping a related edge off the
    // chain frontier. `merge_declared_related` reads the subject's own
    // frontmatter and can never produce it.
    fixture.write_doc(
        "docs/stories/STORY-002-inbound-peer.md",
        &doc("Inbound peer", "story", &["- related-to: RFC-001"]),
    );
    // STORY-003 declares the chain link and ITERATION-002 sits beside it
    // declaring nothing, so `propagate_parent_links` lends it a copy while its
    // own `related` stays empty (ADR-034).
    fixture.write_subfolder_doc(
        "docs/stories/STORY-003-nesting-parent",
        &doc("Nesting parent", "story", &["- implements: RFC-001"]),
    );
    fixture.write_doc(
        "docs/stories/STORY-003-nesting-parent/ITERATION-002.md",
        &doc("Nested inheritor", "iteration", &[]),
    );
    fixture
}

/// The chain and neighbourhood of one document, as ids. Every surface reduces to
/// this, and the assertion is set equality between them -- ids sorted, so a
/// surface's own ordering is free to differ.
#[derive(Debug, PartialEq, Eq)]
pub struct Neighbourhood {
    pub ancestors: Vec<String>,
    pub descendants: Vec<String>,
    pub related: Vec<String>,
}

pub fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

/// The CLI's claim, read off `context --json` -- the interface an agent consumes
/// (Principle 2), not a re-run of the engine call behind it.
///
/// Only `chain` drops the subject, because only `chain` is documented to carry
/// it (the TUI and the web view drop it from their ancestor group and nowhere
/// else). `forward` and `related` are taken verbatim, so a subject that leaked
/// into either shows up as an extra id rather than being filtered away.
pub fn cli_neighbourhood(store: &Store, subject: &str) -> Neighbourhood {
    let json = lazyspec::cli::context::run_json(store, subject, 1).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ids = |key: &str| {
        sorted(
            value[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|entry| entry["id"].as_str().unwrap().to_string())
                .collect(),
        )
    };
    Neighbourhood {
        ancestors: ids("chain")
            .into_iter()
            .filter(|id| id != subject)
            .collect(),
        descendants: ids("forward"),
        related: ids("related"),
    }
}

/// The shorthand `context` resolves a document by: its bare id, except for a
/// nested child, which is only addressable as `PARENT/CHILD`.
pub fn shorthand(store: &Store, doc: &DocMeta) -> String {
    match store.parent_of(&doc.path).and_then(|p| store.get(p)) {
        Some(parent) => format!("{}/{}", parent.id, doc.id),
        None => doc.id.clone(),
    }
}
