use lazyspec::cli::show;
use lazyspec::engine::config::Config;
use lazyspec::engine::fs::RealFileSystem;
use lazyspec::engine::store::Store;
use lazyspec::engine::template;
use lazyspec::tui::content::gfm::{extract_gfm_segments, render_gfm_segments};
use std::fs;
use tempfile::TempDir;

fn init_root() -> TempDir {
    let dir = TempDir::new().unwrap();
    lazyspec::cli::init::run(dir.path()).unwrap();
    dir
}

fn templates_dir(root: &std::path::Path) -> std::path::PathBuf {
    root.join(".lazyspec/templates")
}

fn load_config(root: &std::path::Path) -> Config {
    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    Config::parse(&content).unwrap()
}

// A doc created from the materialized template carries its intent header and a
// guidance comment under each section, regardless of type.
#[test]
fn created_doc_contains_comments() {
    let dir = init_root();
    let root = dir.path();
    let config = load_config(root);
    let store = Store::load(root, &config).unwrap();

    let path =
        lazyspec::cli::create::run(root, &config, &store, "story", "User Auth", "agent", |_| {})
            .unwrap();

    let body = fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("<!-- intent:"),
        "created doc should carry an intent header, got:\n{body}"
    );
    let section_count = body.lines().filter(|l| l.starts_with("## ")).count();
    assert!(section_count > 0, "template should have sections");
    let guidance_count = body.matches("<!-- guidance:").count();
    assert!(
        guidance_count >= section_count,
        "expected a guidance comment per section ({section_count}), got {guidance_count}:\n{body}"
    );
}

// `{type}` is substituted per document, so the single template serves every type.
#[test]
fn created_doc_substitutes_type() {
    let dir = init_root();
    let root = dir.path();
    let config = load_config(root);
    let store = Store::load(root, &config).unwrap();

    let story =
        lazyspec::cli::create::run(root, &config, &store, "story", "Slice", "agent", |_| {})
            .unwrap();
    let iteration =
        lazyspec::cli::create::run(root, &config, &store, "iteration", "Build", "agent", |_| {})
            .unwrap();

    assert!(fs::read_to_string(&story).unwrap().contains("type: story"));
    assert!(fs::read_to_string(&iteration)
        .unwrap()
        .contains("type: iteration"));
}

// Comments are invisible on the rendered surfaces (TUI render path), but retained
// on the machine path (`show --json`).
#[test]
fn rendered_output_excludes_comments_tui() {
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
fn show_json_retains_comments() {
    let dir = init_root();
    let root = dir.path();
    let config = load_config(root);
    let store = Store::load(root, &config).unwrap();

    let path =
        lazyspec::cli::create::run(root, &config, &store, "story", "Json Body", "agent", |_| {})
            .unwrap();

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

// Substitution resolves placeholders while leaving comments byte-for-byte.
#[test]
fn substitution_intact() {
    let dir = init_root();
    let root = dir.path();

    let raw = fs::read_to_string(templates_dir(root).join("template.md")).unwrap();
    let comments: Vec<&str> = raw
        .lines()
        .filter(|l| l.trim_start().starts_with("<!--"))
        .collect();
    assert!(!comments.is_empty(), "template should carry comments");

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
    assert!(rendered.contains("type: story"));
    assert!(!rendered.contains("{title}") && !rendered.contains("{type}"));

    for comment in comments {
        assert!(
            rendered.contains(comment),
            "comment must survive substitution byte-for-byte: {comment:?}"
        );
    }
}

// init materializes a single template.md, not one file per type.
#[test]
fn init_writes_single_template() {
    let dir = init_root();
    let root = dir.path();
    let tdir = templates_dir(root);

    let template = tdir.join("template.md");
    assert!(template.exists(), "init should materialize template.md");
    assert!(!fs::read_to_string(&template).unwrap().trim().is_empty());

    for ty in ["rfc", "story", "iteration", "spec"] {
        assert!(
            !tdir.join(format!("{ty}.md")).exists(),
            "init should not materialize a per-type {ty}.md"
        );
    }
}

// The materialized template.md carries one intent header and a guidance comment
// per section.
#[test]
fn template_carries_comments() {
    let dir = init_root();
    let root = dir.path();

    let body = fs::read_to_string(templates_dir(root).join("template.md")).unwrap();

    let intent_count = body.matches("<!-- intent:").count();
    assert_eq!(
        intent_count, 1,
        "template.md should carry exactly one intent header, got {intent_count}:\n{body}"
    );

    let section_count = body.lines().filter(|l| l.starts_with("## ")).count();
    let guidance_count = body.matches("<!-- guidance:").count();
    assert!(section_count > 0, "template.md should have sections");
    assert!(
        guidance_count >= section_count,
        "template.md should carry a guidance comment per section ({section_count}), got {guidance_count}:\n{body}"
    );
}

// An edited template.md wins over the embedded default.
#[test]
fn shared_template_override_wins() {
    let dir = init_root();
    let root = dir.path();

    let template_path = templates_dir(root).join("template.md");
    let mut edited = fs::read_to_string(&template_path).unwrap();
    edited.push_str("\n## Custom Marker\n\nSENTINEL-OVERRIDE\n");
    fs::write(&template_path, &edited).unwrap();

    let config = load_config(root);
    let store = Store::load(root, &config).unwrap();

    let path =
        lazyspec::cli::create::run(root, &config, &store, "story", "Override", "agent", |_| {})
            .unwrap();

    let body = fs::read_to_string(&path).unwrap();
    assert!(
        body.contains("SENTINEL-OVERRIDE") && body.contains("## Custom Marker"),
        "created doc must reflect the edited template.md, got:\n{body}"
    );
}

// A per-type {type}.md override takes precedence over the shared template.md.
#[test]
fn per_type_override_beats_shared() {
    let dir = init_root();
    let root = dir.path();

    fs::write(
        templates_dir(root).join("story.md"),
        "---\ntitle: \"{title}\"\ntype: {type}\nstatus: draft\nauthor: \"{author}\"\ndate: {date}\ntags: []\n---\nPER-TYPE-STORY-MARKER\n",
    )
    .unwrap();

    let config = load_config(root);
    let store = Store::load(root, &config).unwrap();

    let story =
        lazyspec::cli::create::run(root, &config, &store, "story", "Typed", "agent", |_| {})
            .unwrap();
    let iteration =
        lazyspec::cli::create::run(root, &config, &store, "iteration", "Plain", "agent", |_| {})
            .unwrap();

    assert!(fs::read_to_string(&story)
        .unwrap()
        .contains("PER-TYPE-STORY-MARKER"));
    assert!(
        !fs::read_to_string(&iteration)
            .unwrap()
            .contains("PER-TYPE-STORY-MARKER"),
        "iteration should fall back to the shared template, not the story override"
    );
}
