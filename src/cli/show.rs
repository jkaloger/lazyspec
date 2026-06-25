use crate::cli::json::doc_to_json_with_family;
use crate::cli::resolve::resolve_shorthand_or_path;
use crate::cli::style::{bold, dim, separator, styled_status};
use crate::engine::config::{Config, StoreBackend};
use crate::engine::document::DocMeta;
use crate::engine::fs::FileSystem;
use crate::engine::gh::GhIssueReader;
use crate::engine::github::resolve_repo;
use crate::engine::issue_map::IssueMap;
use crate::engine::store::{ResolveError, Store};
use anyhow::Result;
use console::colors_enabled;
use std::path::Path;

/// Read-only fetch of a document's GitHub issue comment thread as JSON values.
///
/// Filesystem-backed documents short-circuit to `vec![]` without touching `gh`.
/// Comments are fetched live and surfaced as a JSON sidecar; they never enter
/// the document body or cache, and are never written back to GitHub.
pub fn fetch_comments_for_doc(
    doc: &DocMeta,
    config: &Config,
    root: &Path,
    gh: &dyn GhIssueReader,
) -> Vec<serde_json::Value> {
    let is_github = config
        .type_by_name(doc.doc_type.as_str())
        .map(|t| t.store == StoreBackend::GithubIssues)
        .unwrap_or(false);
    if !is_github {
        return vec![];
    }

    let comments = (|| {
        let repo = resolve_repo(config, root)?;
        let number = IssueMap::load(root)?
            .get(&doc.id)
            .map(|e| e.issue_number)
            .ok_or_else(|| anyhow::anyhow!("no issue mapping for {}", doc.id))?;
        gh.issue_comments(&repo, number)
    })()
    .unwrap_or_default();

    comments
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "author": c.author,
                "body": c.body,
                "timestamp": c.timestamp,
            })
        })
        .collect()
}

/// Remove HTML comments (`<!-- ... -->`) from a body before plaintext display.
/// The TUI renderer drops HTML events on its own; `show` prints raw, so it strips here.
/// Machine paths (`--json`, `get_body_raw`/`expanded`) keep comments verbatim.
fn strip_html_comments(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        match rest[start..].find("-->") {
            Some(end) => rest = &rest[start + end + 3..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

fn title_box(title: &str) -> String {
    if !colors_enabled() {
        return format!("# {}", title);
    }

    let padded = format!(" {} ", title);
    let width = padded.len();
    let top = format!("\u{256d}{}\u{256e}", "\u{2500}".repeat(width));
    let mid = format!("\u{2502}{}\u{2502}", bold(&padded));
    let bot = format!("\u{2570}{}\u{256f}", "\u{2500}".repeat(width));
    format!("{}\n{}\n{}", top, mid, bot)
}

pub fn run(
    store: &Store,
    id: &str,
    expand: bool,
    max_ref_lines: usize,
    fs: &dyn FileSystem,
) -> Result<()> {
    let doc = match resolve_shorthand_or_path(store, id) {
        Ok(doc) => doc,
        Err(ResolveError::Ambiguous { id, matches }) => {
            eprintln!("Ambiguous ID '{}' matches multiple documents:", id);
            for m in &matches {
                eprintln!("  {}", m.display());
            }
            eprintln!("Specify the full path to show a specific document.");
            return Ok(());
        }
        Err(ResolveError::NotFound(id)) => {
            return Err(anyhow::anyhow!("document not found: {}", id));
        }
    };

    println!("{}", title_box(&doc.title));
    println!(
        "{} {}  {} {}  {} {}",
        dim("Type:"),
        bold(&doc.doc_type.to_string()),
        dim("Status:"),
        styled_status(&doc.status),
        dim("Author:"),
        bold(&doc.author),
    );
    if !doc.tags.is_empty() {
        println!("{} {}", dim("Tags:"), doc.tags.join(", "));
    }
    if let Some(parent_path) = store.parent_of(&doc.path) {
        if let Some(parent) = store.get(parent_path) {
            println!(
                "{} {} {}",
                dim("Parent:"),
                bold(&parent.title),
                dim(&parent.path.to_string_lossy()),
            );
        }
    }
    println!("{}", separator());

    let body = if expand {
        store.get_body_expanded(&doc.path, max_ref_lines, fs)?
    } else {
        store.get_body_raw(&doc.path, fs)?
    };
    println!("{}", strip_html_comments(&body));

    let child_paths = store.children_of(&doc.path);
    if !child_paths.is_empty() {
        println!();
        println!("{}", dim("Children:"));
        for cp in child_paths {
            if let Some(child) = store.get(cp) {
                let parent_dir = cp.parent().and_then(|p| p.file_name()).unwrap_or_default();
                let file_stem = cp.file_stem().unwrap_or_default();
                let qualified_shorthand = format!(
                    "{}/{}",
                    parent_dir.to_string_lossy(),
                    file_stem.to_string_lossy()
                );
                println!("  - {}  ({})", child.title, qualified_shorthand);
            }
        }
    }

    Ok(())
}

pub fn run_json(
    store: &Store,
    id: &str,
    expand: bool,
    max_ref_lines: usize,
    fs: &dyn FileSystem,
    config: &Config,
    root: &Path,
    gh: &dyn GhIssueReader,
) -> Result<String> {
    let doc = match resolve_shorthand_or_path(store, id) {
        Ok(doc) => doc,
        Err(ResolveError::Ambiguous { id, matches }) => {
            let paths: Vec<String> = matches
                .iter()
                .map(|m| m.to_string_lossy().to_string())
                .collect();
            let error = serde_json::json!({
                "error": "ambiguous_id",
                "id": id,
                "ambiguous_matches": paths,
            });
            return Ok(serde_json::to_string_pretty(&error)?);
        }
        Err(ResolveError::NotFound(id)) => {
            return Err(anyhow::anyhow!("document not found: {}", id));
        }
    };

    let mut json = doc_to_json_with_family(doc, store);
    let body = if expand {
        store.get_body_expanded(&doc.path, max_ref_lines, fs)?
    } else {
        store.get_body_raw(&doc.path, fs)?
    };
    json["body"] = serde_json::Value::String(body);
    json["comments"] = serde_json::Value::Array(fetch_comments_for_doc(doc, config, root, gh));

    Ok(serde_json::to_string_pretty(&json)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::TypeDef;
    use crate::engine::document::{DocType, Status};
    use crate::engine::gh::test_support::MockGhClient;
    use crate::engine::gh::GhComment;
    use crate::engine::issue_map::IssueMap;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn doc(doc_type: &str) -> DocMeta {
        DocMeta {
            path: PathBuf::from(format!("docs/{}/X-1.md", doc_type)),
            title: "X".to_string(),
            doc_type: DocType::new(doc_type),
            status: Status::new("draft"),
            author: "a".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 25).unwrap(),
            tags: vec![],
            provenance: vec![],
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            attributes: Default::default(),
            id: "X-1".to_string(),
        }
    }

    fn github_config(type_name: &str) -> Config {
        let mut config = Config::default();
        config
            .documents
            .types
            .push(TypeDef::test_fixture(type_name, StoreBackend::GithubIssues));
        config.documents.github = Some(crate::engine::config::GithubConfig {
            repo: Some("owner/repo".to_string()),
            cache_ttl: 60,
        });
        config
    }

    fn comment(author: &str, body: &str) -> GhComment {
        GhComment {
            author: author.to_string(),
            body: body.to_string(),
            timestamp: "2026-06-01T00:00:00Z".to_string(),
        }
    }

    // AC1: github-backed doc surfaces each fetched comment as an
    // {author, body, timestamp} JSON object.
    #[test]
    fn fetch_comments_maps_github_comments() {
        let tmp = TempDir::new().unwrap();
        let mut map = IssueMap::load(tmp.path()).unwrap();
        map.insert("X-1", 42, "ts", "");
        map.save(tmp.path()).unwrap();

        let config = github_config("ghtype");
        let gh = MockGhClient::new()
            .with_comments(vec![comment("alice", "first"), comment("bob", "second")]);

        let out = fetch_comments_for_doc(&doc("ghtype"), &config, tmp.path(), &gh);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["author"], "alice");
        assert_eq!(out[0]["body"], "first");
        assert_eq!(out[0]["timestamp"], "2026-06-01T00:00:00Z");
        assert_eq!(out[1]["author"], "bob");
    }

    // AC4: a filesystem-backed type never triggers a comment fetch.
    #[test]
    fn fetch_comments_short_circuits_filesystem() {
        let tmp = TempDir::new().unwrap();
        let mut config = Config::default();
        config
            .documents
            .types
            .push(TypeDef::test_fixture("fstype", StoreBackend::Filesystem));
        let gh = MockGhClient::new().with_comments(vec![comment("alice", "x")]);

        let out = fetch_comments_for_doc(&doc("fstype"), &config, tmp.path(), &gh);
        assert!(out.is_empty());
        assert_eq!(gh.comments_call_count.get(), 0);
    }

    // AC5: a github-backed doc with no comments yields an empty array (present,
    // not absent).
    #[test]
    fn fetch_comments_empty_is_empty_array() {
        let tmp = TempDir::new().unwrap();
        let mut map = IssueMap::load(tmp.path()).unwrap();
        map.insert("X-1", 42, "ts", "");
        map.save(tmp.path()).unwrap();

        let config = github_config("ghtype");
        let gh = MockGhClient::new().with_comments(vec![]);

        let out = fetch_comments_for_doc(&doc("ghtype"), &config, tmp.path(), &gh);
        assert!(out.is_empty());
        assert_eq!(gh.comments_call_count.get(), 1);
    }
}

#[cfg(test)]
mod strip_tests {
    use super::strip_html_comments;

    #[test]
    fn strips_intent_and_guidance_comments() {
        let body = "<!-- intent: do a thing -->\n\n## Context\n<!-- guidance: the why -->\n\nReal prose.\n";
        let out = strip_html_comments(body);
        assert!(!out.contains("<!--"));
        assert!(!out.contains("intent:"));
        assert!(!out.contains("guidance:"));
        assert!(out.contains("## Context"));
        assert!(out.contains("Real prose."));
    }

    #[test]
    fn leaves_comment_free_body_untouched() {
        let body = "## Summary\n\nNo comments here.\n";
        assert_eq!(strip_html_comments(body), body);
    }

    #[test]
    fn tolerates_unterminated_comment() {
        let body = "before <!-- never closed";
        assert_eq!(strip_html_comments(body), "before ");
    }
}
