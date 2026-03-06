use anyhow::Result;
use lazyspec::engine::document::rewrite_frontmatter;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn mutates_frontmatter_and_preserves_body() -> Result<()> {
    let mut file = NamedTempFile::new()?;
    write!(
        file,
        "---\ntitle: Test\nstatus: draft\n---\nBody content\n"
    )?;

    rewrite_frontmatter(file.path(), |value| {
        value["status"] = serde_yaml::Value::String("accepted".to_string());
        Ok(())
    })?;

    let result = std::fs::read_to_string(file.path())?;

    assert!(result.starts_with("---\n"), "should start with frontmatter delimiter");
    assert!(result.contains("status: accepted"), "status should be updated to accepted");
    assert!(result.contains("title: Test"), "title should be preserved");
    assert!(!result.contains("draft"), "old status value should not remain");
    assert!(result.contains("Body content"), "body content should be preserved");

    Ok(())
}
