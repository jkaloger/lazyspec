use anyhow::Result;
use lazyspec::engine::document::{rewrite_frontmatter, DocMeta};
use lazyspec::engine::fs::RealFileSystem;
use std::io::Write;
use tempfile::NamedTempFile;

fn doc_with_provenance(doc_type: &str, provenance_block: &str) -> String {
    format!(
        "---\ntitle: \"Doc\"\ntype: {}\nstatus: draft\nauthor: a\ndate: 2026-01-01\ntags: []\n{}---\n\nBody.\n",
        doc_type, provenance_block,
    )
}

#[test]
fn provenance_round_trips_via_rewriter() -> Result<()> {
    let block = "provenance:\n  - \"Workshop 2026-04-12\"\n  - \"Jane Doe\"\n  - \"Privacy Act 1988\"\n";
    let content = doc_with_provenance("rfc", block);

    let mut file = NamedTempFile::new()?;
    write!(file, "{}", content)?;

    let fs = RealFileSystem;
    rewrite_frontmatter(file.path(), &fs, |_value| Ok(()))?;

    let reloaded = std::fs::read_to_string(file.path())?;
    let meta = DocMeta::parse(&reloaded)?;

    assert_eq!(
        meta.provenance,
        vec![
            "Workshop 2026-04-12".to_string(),
            "Jane Doe".to_string(),
            "Privacy Act 1988".to_string(),
        ]
    );
    Ok(())
}

#[test]
fn provenance_empty_round_trips() -> Result<()> {
    let content = doc_with_provenance("rfc", "");

    let mut file = NamedTempFile::new()?;
    write!(file, "{}", content)?;

    let fs = RealFileSystem;
    rewrite_frontmatter(file.path(), &fs, |_value| Ok(()))?;

    let reloaded = std::fs::read_to_string(file.path())?;
    let meta = DocMeta::parse(&reloaded)?;

    assert!(meta.provenance.is_empty());
    Ok(())
}

#[test]
fn provenance_works_for_each_doc_type() {
    for doc_type in ["rfc", "story", "iteration", "audit", "adr", "spec"] {
        let block = "provenance:\n  - \"Source A\"\n  - \"Source B\"\n";
        let content = doc_with_provenance(doc_type, block);

        let meta = DocMeta::parse(&content)
            .unwrap_or_else(|e| panic!("parse failed for {}: {}", doc_type, e));

        assert_eq!(
            meta.provenance,
            vec!["Source A".to_string(), "Source B".to_string()],
            "provenance mismatch for type {}",
            doc_type
        );
    }
}
