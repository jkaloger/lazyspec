use lazyspec::cli::show;
use lazyspec::engine::config::Config;
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;
use lazyspec::engine::template;
use lazyspec::tui::content::gfm::{extract_gfm_segments, render_gfm_segments};
use std::fs;
use tempfile::TempDir;

/// The default types `init` materializes, each expected to gain an `## ` section
/// carrying a `<!-- guidance -->` comment plus a single `<!-- intent -->` header.
const DEFAULT_TYPES: &[&str] = &[
    "rfc",
    "story",
    "iteration",
    "adr",
    "spec",
    "convention",
    "dictum",
];

fn init_root() -> TempDir {
    let dir = TempDir::new().unwrap();
    lazyspec::cli::init::run(dir.path()).unwrap();
    dir
}

fn templates_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".lazyspec/templates")
}

// AC1: a doc created from the materialized on-disk template carries its intent
// header and a guidance comment under each section.
#[test]
fn ac1_created_doc_contains_comments() {
    let dir = init_root();
    let root = dir.path();

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    let config = Config::parse(&content).unwrap();
    let store = Store::load(root, &config).unwrap();

    let path =
        lazyspec::cli::create::run(root, &config, &store, "story", "User Auth", "agent", |_| {})
            .unwrap();

    let body = fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("<!-- intent:"),
        "created story should carry an intent header, got:\n{body}"
    );
    // Every `## ` section heading must be followed by a guidance comment.
    let lines: Vec<&str> = body.lines().collect();
    let section_count = lines.iter().filter(|l| l.starts_with("## ")).count();
    assert!(section_count > 0, "story template should have sections");
    let guidance_count = body.matches("<!-- guidance:").count();
    assert!(
        guidance_count >= section_count,
        "expected a guidance comment per section ({section_count}), got {guidance_count}:\n{body}"
    );
}

// AC2: comments are invisible on the rendered surfaces (TUI render path + plaintext
// `show`), but retained on the machine path (`show --json`).
#[test]
fn ac2_rendered_output_excludes_comments_tui() {
    let body = "<!-- intent: do a thing -->\n\n## Context\n<!-- guidance: the why -->\n\nReal prose here.\n";

    let segments = extract_gfm_segments(body);
    let rendered = render_gfm_segments(&segments, 80);

    for line in &rendered {
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            !text.contains("<!--") && !text.contains("intent:") && !text.contains("guidance:"),
            "TUI render leaked a comment: {text:?}"
        );
    }

    let joined: String = rendered
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.to_string()))
        .collect();
    assert!(
        joined.contains("Real prose here"),
        "real prose should survive rendering, got: {joined:?}"
    );
}

#[test]
fn ac2_show_json_retains_comments() {
    let dir = init_root();
    let root = dir.path();

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    let config = Config::parse(&content).unwrap();
    let store = Store::load(root, &config).unwrap();

    let path =
        lazyspec::cli::create::run(root, &config, &store, "story", "Json Body", "agent", |_| {})
            .unwrap();

    // Reload so the store sees the new doc.
    let store = Store::load(root, &config).unwrap();
    let rel = path
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .to_string();
    let json = show::run_json(&store, &rel, false, 10, &RealFileSystem).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let json_body = value["body"].as_str().unwrap();
    assert!(
        json_body.contains("<!-- intent:") && json_body.contains("<!-- guidance:"),
        "show --json body must retain comments verbatim, got:\n{json_body}"
    );
}

// AC3: substitution resolves placeholders while leaving comments byte-for-byte.
#[test]
fn ac3_substitution_intact() {
    let dir = init_root();
    let root = dir.path();

    let raw = fs::read_to_string(templates_dir(root).join("story.md")).unwrap();
    let intent_comments: Vec<&str> = raw
        .lines()
        .filter(|l| l.trim_start().starts_with("<!--"))
        .collect();
    assert!(!intent_comments.is_empty(), "fixture should carry comments");

    let vars = vec![
        ("title", "My Title"),
        ("author", "alice"),
        ("date", "2026-06-23"),
        ("type", "story"),
    ];
    let rendered = template::render_template(&raw, &vars);

    assert!(rendered.contains("title: \"My Title\""));
    assert!(rendered.contains("author: \"alice\""));
    assert!(rendered.contains("date: 2026-06-23"));
    assert!(!rendered.contains("{title}") && !rendered.contains("{author}"));

    for comment in intent_comments {
        assert!(
            rendered.contains(comment),
            "comment must survive substitution byte-for-byte: {comment:?}"
        );
    }
}

// AC4: init materializes a {type}.md per default type into the templates dir.
#[test]
fn ac4_init_writes_template_files() {
    let dir = init_root();
    let root = dir.path();
    let tdir = templates_dir(root);

    for ty in DEFAULT_TYPES {
        let path = tdir.join(format!("{ty}.md"));
        assert!(path.exists(), "init should materialize {ty}.md");
        let body = fs::read_to_string(&path).unwrap();
        assert!(!body.trim().is_empty(), "{ty}.md should not be empty");
    }
}

// AC5: an edited on-disk template wins over the embedded default.
#[test]
fn ac5_on_disk_override_wins() {
    let dir = init_root();
    let root = dir.path();

    let template_path = templates_dir(root).join("story.md");
    let mut edited = fs::read_to_string(&template_path).unwrap();
    edited.push_str("\n## Custom Marker\n\nSENTINEL-OVERRIDE\n");
    fs::write(&template_path, &edited).unwrap();

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    let config = Config::parse(&content).unwrap();
    let store = Store::load(root, &config).unwrap();

    let path = lazyspec::cli::create::run(
        root,
        &config,
        &store,
        "story",
        "Override Check",
        "agent",
        |_| {},
    )
    .unwrap();

    let body = fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("SENTINEL-OVERRIDE") && body.contains("## Custom Marker"),
        "created doc must reflect the edited on-disk template, got:\n{body}"
    );
}

// AC6: every materialized default template carries one intent header and a
// guidance comment per section.
#[test]
fn ac6_defaults_carry_comments() {
    let dir = init_root();
    let root = dir.path();
    let tdir = templates_dir(root);

    for ty in DEFAULT_TYPES {
        let body = fs::read_to_string(tdir.join(format!("{ty}.md"))).unwrap();

        let intent_count = body.matches("<!-- intent:").count();
        assert_eq!(
            intent_count, 1,
            "{ty}.md should carry exactly one intent header, got {intent_count}:\n{body}"
        );

        let section_count = body.lines().filter(|l| l.starts_with("## ")).count();
        let guidance_count = body.matches("<!-- guidance:").count();
        assert!(
            section_count > 0,
            "{ty}.md should have at least one section"
        );
        assert!(
            guidance_count >= section_count,
            "{ty}.md should carry a guidance comment per section ({section_count}), got {guidance_count}:\n{body}"
        );
    }
}
