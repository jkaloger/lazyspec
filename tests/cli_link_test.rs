use lazyspec::engine::document::DocMeta;
use std::fs;
use tempfile::TempDir;

fn setup_two_docs(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("docs/rfcs")).unwrap();
    fs::create_dir_all(dir.join("docs/adrs")).unwrap();
    fs::write(
        dir.join("docs/rfcs/RFC-001-auth.md"),
        "---\ntitle: \"Auth\"\ntype: rfc\nstatus: accepted\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n",
    ).unwrap();
    fs::write(
        dir.join("docs/adrs/ADR-001-adopt-auth.md"),
        "---\ntitle: \"Adopt Auth\"\ntype: adr\nstatus: draft\nauthor: a\ndate: 2026-01-02\ntags: []\n---\n",
    ).unwrap();
}

#[test]
fn link_adds_relationship_to_frontmatter() {
    let dir = TempDir::new().unwrap();
    setup_two_docs(dir.path());

    lazyspec::cli::link::link(
        dir.path(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    let content = fs::read_to_string(dir.path().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(meta.related.len(), 1);
    assert_eq!(meta.related[0].target, "docs/rfcs/RFC-001-auth.md");
}

#[test]
fn unlink_removes_relationship() {
    let dir = TempDir::new().unwrap();
    setup_two_docs(dir.path());

    lazyspec::cli::link::link(
        dir.path(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    lazyspec::cli::link::unlink(
        dir.path(),
        "docs/adrs/ADR-001-adopt-auth.md",
        "implements",
        "docs/rfcs/RFC-001-auth.md",
    ).unwrap();

    let content = fs::read_to_string(dir.path().join("docs/adrs/ADR-001-adopt-auth.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert!(meta.related.is_empty());
}
