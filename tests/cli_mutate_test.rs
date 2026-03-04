use lazyspec::engine::document::DocMeta;
use std::fs;
use tempfile::TempDir;

fn write_doc(dir: &std::path::Path) {
    fs::create_dir_all(dir.join("docs/rfcs")).unwrap();
    fs::write(
        dir.join("docs/rfcs/RFC-001-test.md"),
        "---\ntitle: \"Test\"\ntype: rfc\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n---\n\nBody.\n",
    ).unwrap();
}

#[test]
fn update_status_in_frontmatter() {
    let dir = TempDir::new().unwrap();
    write_doc(dir.path());

    lazyspec::cli::update::run(dir.path(), "docs/rfcs/RFC-001-test.md", &[("status", "review")]).unwrap();

    let content = fs::read_to_string(dir.path().join("docs/rfcs/RFC-001-test.md")).unwrap();
    let meta = DocMeta::parse(&content).unwrap();
    assert_eq!(format!("{}", meta.status), "review");
}

#[test]
fn delete_removes_file() {
    let dir = TempDir::new().unwrap();
    write_doc(dir.path());

    let path = dir.path().join("docs/rfcs/RFC-001-test.md");
    assert!(path.exists());

    lazyspec::cli::delete::run(dir.path(), "docs/rfcs/RFC-001-test.md").unwrap();
    assert!(!path.exists());
}
