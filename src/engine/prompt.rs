//! Context depth choice: `context.ancestors` is the FULL `implements` chain
//! (sourced from [`resolve_chain`]'s `nodes`); `context.related` is the
//! directly-adjacent `related-to` ring only, so `resolve_chain` is called with
//! depth `1`. Descendant (`forward`) docs are deliberately omitted.

use crate::engine::config::{Config, ValidationRule};
use crate::engine::context::resolve_chain;
use crate::engine::document::{split_frontmatter, DocMeta};
use crate::engine::fs::FileSystem;
use crate::engine::store::Store;
use anyhow::Result;
use minijinja::{Environment, UndefinedBehavior, Value};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// How an agent run is executed. `headless` runs in the background; `interactive`
/// hands the session to the user. Deserialized from frontmatter as a lowercase
/// string (`mode: headless` / `mode: interactive`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunMode {
    #[default]
    Headless,
    Interactive,
}

/// A user-authored agent prompt, discovered under `.lazyspec/agents/*.md`. The
/// body is retained verbatim as the minijinja render template (see [`render`]).
#[derive(Debug, Clone)]
pub struct AgentPrompt {
    pub name: String,
    pub description: String,
    pub mode: RunMode,
    pub allowed_tools: Option<String>,
    pub body_template: String,
}

/// A discovered prompt file that could not be loaded. Carries the offending path
/// and a human-readable reason so callers can surface AC8 behaviour without
/// capturing stderr.
pub struct PromptWarning {
    pub path: PathBuf,
    pub reason: String,
}

/// Raw frontmatter shape. `name` and `description` are required (a missing field
/// is a serde error, which drives the AC8 skip); `mode` and `allowed_tools` are
/// optional.
#[derive(serde::Deserialize)]
struct RawPromptFm {
    name: String,
    description: String,
    #[serde(default)]
    mode: RunMode,
    #[serde(default)]
    allowed_tools: Option<String>,
}

/// Parse a single agent prompt file's contents into an [`AgentPrompt`].
///
/// The body following the frontmatter is preserved verbatim as the render
/// template. A missing/malformed frontmatter (e.g. absent `name`/`description`)
/// is an error, which callers translate into a skipped file plus a warning.
pub fn parse_prompt(content: &str) -> Result<AgentPrompt> {
    let (yaml, body) = split_frontmatter(content)?;
    let raw: RawPromptFm = serde_yaml::from_str(&yaml)?;
    Ok(AgentPrompt {
        name: raw.name,
        description: raw.description,
        mode: raw.mode,
        allowed_tools: raw.allowed_tools,
        body_template: body,
    })
}

/// Discover user-authored agent prompts under `<repo_root>/.lazyspec/agents/`.
///
/// Zero-defaults: the engine ships no prompts, so an absent directory is not an
/// error -- it yields no prompts. Each `*.md` entry is parsed; a parse failure is
/// recorded as a [`PromptWarning`] (and emitted to stderr) and skipped without
/// aborting the rest of the discovery. Non-`.md` entries are ignored.
pub fn discover_prompts(
    repo_root: &Path,
    fs: &dyn FileSystem,
) -> (Vec<AgentPrompt>, Vec<PromptWarning>) {
    let mut prompts = Vec::new();
    let mut warnings = Vec::new();

    let agents_dir = repo_root.join(".lazyspec").join("agents");
    if !fs.is_dir(&agents_dir) {
        return (prompts, warnings);
    }

    let mut entries = match fs.read_dir(&agents_dir) {
        Ok(entries) => entries,
        Err(_) => return (prompts, warnings),
    };
    entries.sort();

    for entry in entries {
        if entry.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let content = match fs.read_to_string(&entry) {
            Ok(content) => content,
            Err(err) => {
                warnings.push(warn(&entry, format!("could not read: {err}")));
                continue;
            }
        };

        match parse_prompt(&content) {
            Ok(prompt) => prompts.push(prompt),
            Err(err) => warnings.push(warn(&entry, format!("invalid frontmatter: {err}"))),
        }
    }

    (prompts, warnings)
}

fn warn(path: &Path, reason: String) -> PromptWarning {
    eprintln!(
        "warning: skipping agent prompt {}: {}",
        path.display(),
        reason
    );
    PromptWarning {
        path: path.to_path_buf(),
        reason,
    }
}

/// The `document.*` shape exposed to templates, used both at the top level and
/// for each lineage entry under `context.ancestors` / `context.related`.
#[derive(Serialize)]
struct DocumentView {
    id: String,
    title: String,
    #[serde(rename = "type")]
    doc_type: String,
    body: String,
    status: String,
    path: String,
}

#[derive(Serialize)]
struct ContextView {
    ancestors: Vec<DocumentView>,
    related: Vec<DocumentView>,
}

#[derive(Serialize)]
struct RenderContext {
    document: DocumentView,
    child_types: Vec<String>,
    context: ContextView,
}

/// Build the `document.*` view for a single doc, reading its body off disk via
/// `fs`. Shared by the top-level `document` and every lineage entry (>2 uses).
fn doc_to_view(store: &Store, doc: &DocMeta, fs: &dyn FileSystem) -> Result<DocumentView> {
    Ok(DocumentView {
        id: doc.id.clone(),
        title: doc.title.clone(),
        doc_type: doc.doc_type.as_str().to_string(),
        body: store.get_body_raw(&doc.path, fs)?,
        status: doc.status.to_string(),
        path: doc.path.display().to_string(),
    })
}

/// Child type names for `doc`'s type: each `child` from a `ParentChild` rule
/// whose `parent` matches `doc.doc_type`. No matching rule yields an empty list
/// (loops render empty, not undefined). Mirrors the TUI `spawn_create_children`
/// derivation.
fn child_types_for(config: &Config, doc: &DocMeta) -> Vec<String> {
    let doc_type = doc.doc_type.as_str();
    config
        .rules
        .iter()
        .filter_map(|rule| match rule {
            ValidationRule::ParentChild { parent, child, .. } if parent == doc_type => {
                Some(child.clone())
            }
            _ => None,
        })
        .collect()
}

/// Assemble the minijinja render context for `doc`:
/// - `document`: the selected doc's fields (id/title/type/body/status/path).
/// - `child_types`: child type names from `ParentChild` rules.
/// - `context.ancestors`: the `implements` chain, nearest-parent-first, target
///   excluded (full ancestry, from [`resolve_chain`]'s `nodes`).
/// - `context.related`: directly-adjacent `related-to` docs (depth `1`).
///
/// Descendant (`forward`) docs are deliberately omitted. Lineage entries
/// expose the same `document.*` shape as the top-level `document`.
pub fn build_render_context(
    store: &Store,
    config: &Config,
    doc: &DocMeta,
    fs: &dyn FileSystem,
) -> Result<Value> {
    let resolved = resolve_chain(store, &doc.id, 1)?;

    // `nodes` is root-first and includes the target last. Drop the target and
    // reverse so the immediate parent comes first.
    let mut ancestors = Vec::new();
    for node in resolved.nodes.iter().rev() {
        if node.doc.path == doc.path {
            continue;
        }
        ancestors.push(doc_to_view(store, node.doc, fs)?);
    }

    let mut related = Vec::new();
    for r in &resolved.related {
        related.push(doc_to_view(store, r.doc, fs)?);
    }

    let ctx = RenderContext {
        document: doc_to_view(store, doc, fs)?,
        child_types: child_types_for(config, doc),
        context: ContextView { ancestors, related },
    };

    Ok(Value::from_serialize(&ctx))
}

/// Render a prompt's body template against `ctx` in strict-undefined mode: an
/// unknown variable is an error, never a silent empty string. The underlying
/// minijinja error is preserved in the returned message so the offending
/// variable name surfaces.
pub fn render(prompt: &AgentPrompt, ctx: &Value) -> Result<String> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.render_str(&prompt.body_template, ctx)
        .map_err(|e| anyhow::anyhow!("template render failed: {e:#}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeFs {
        files: Mutex<HashMap<PathBuf, String>>,
        dirs: Mutex<Vec<PathBuf>>,
    }

    impl FakeFs {
        fn new() -> Self {
            FakeFs {
                files: Mutex::new(HashMap::new()),
                dirs: Mutex::new(Vec::new()),
            }
        }

        fn add_dir(&self, path: PathBuf) {
            self.dirs.lock().unwrap().push(path);
        }

        fn add_file(&self, path: PathBuf, contents: &str) {
            self.files
                .lock()
                .unwrap()
                .insert(path, contents.to_string());
        }
    }

    impl FileSystem for FakeFs {
        fn read_to_string(&self, path: &Path) -> Result<String> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no such file: {}", path.display()))
        }

        fn write(&self, _path: &Path, _contents: &str) -> Result<()> {
            unimplemented!("write not needed for prompt discovery tests")
        }

        fn rename(&self, _from: &Path, _to: &Path) -> Result<()> {
            unimplemented!("rename not needed for prompt discovery tests")
        }

        fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>> {
            let files = self.files.lock().unwrap();
            Ok(files
                .keys()
                .filter(|p| p.parent() == Some(path))
                .cloned()
                .collect())
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.lock().unwrap().contains_key(path)
                || self.dirs.lock().unwrap().contains(&path.to_path_buf())
        }

        fn create_dir_all(&self, _path: &Path) -> Result<()> {
            unimplemented!("create_dir_all not needed for prompt discovery tests")
        }

        fn is_dir(&self, path: &Path) -> bool {
            self.dirs.lock().unwrap().contains(&path.to_path_buf())
        }
    }

    const FULL_FM: &str = concat!(
        "---\n",
        "name: refiner\n",
        "description: Refines a document against its acceptance criteria\n",
        "mode: interactive\n",
        "allowed_tools: Read,Edit\n",
        "---\n",
        "Refine {{ doc }} now.\n",
    );

    // AC2: full frontmatter maps every field; the body is the verbatim template.
    #[test]
    fn parses_full_frontmatter_into_agent_prompt() {
        let prompt = parse_prompt(FULL_FM).unwrap();

        assert_eq!(prompt.name, "refiner");
        assert_eq!(
            prompt.description,
            "Refines a document against its acceptance criteria"
        );
        assert_eq!(prompt.mode, RunMode::Interactive);
        assert_eq!(prompt.allowed_tools, Some("Read,Edit".to_string()));
        assert_eq!(prompt.body_template, "\nRefine {{ doc }} now.\n");
    }

    // AC3: omitted mode defaults to Headless; omitted allowed_tools is None.
    #[test]
    fn mode_defaults_to_headless_when_omitted() {
        let content = concat!(
            "---\n",
            "name: minimal\n",
            "description: Just the required fields\n",
            "---\n",
            "Body.\n",
        );

        let prompt = parse_prompt(content).unwrap();

        assert_eq!(prompt.mode, RunMode::Headless);
        assert_eq!(prompt.allowed_tools, None);
    }

    // AC8 (parse level): missing required field is an error.
    #[test]
    fn missing_name_is_an_error() {
        let content = concat!(
            "---\n",
            "description: Missing the name field\n",
            "---\n",
            "Body.\n",
        );

        assert!(parse_prompt(content).is_err());
    }

    // AC1: every valid *.md under the agents dir loads; non-.md is ignored.
    #[test]
    fn discovers_md_templates_under_agents_dir() {
        let fs = FakeFs::new();
        let root = PathBuf::from("/repo");
        let agents = root.join(".lazyspec").join("agents");
        fs.add_dir(agents.clone());

        fs.add_file(agents.join("refiner.md"), FULL_FM);
        fs.add_file(
            agents.join("reviewer.md"),
            concat!(
                "---\n",
                "name: reviewer\n",
                "description: Reviews a document\n",
                "---\n",
                "Review it.\n",
            ),
        );
        fs.add_file(agents.join("notes.txt"), "not a prompt");

        let (prompts, warnings) = discover_prompts(&root, &fs);

        assert!(warnings.is_empty(), "no warnings expected");
        let mut names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["refiner", "reviewer"]);
    }

    // AC8: malformed/missing frontmatter is skipped with a warning naming the path.
    #[test]
    fn malformed_frontmatter_file_is_skipped_with_warning() {
        let fs = FakeFs::new();
        let root = PathBuf::from("/repo");
        let agents = root.join(".lazyspec").join("agents");
        fs.add_dir(agents.clone());

        fs.add_file(
            agents.join("good.md"),
            concat!(
                "---\n",
                "name: good\n",
                "description: A valid prompt\n",
                "---\n",
                "Do it.\n",
            ),
        );
        let missing_name = agents.join("missing-name.md");
        fs.add_file(
            missing_name.clone(),
            concat!("---\n", "description: No name here\n", "---\n", "Body.\n",),
        );
        let no_fm = agents.join("no-frontmatter.md");
        fs.add_file(no_fm.clone(), "Just a body, no frontmatter.\n");

        let (prompts, warnings) = discover_prompts(&root, &fs);

        let names: Vec<&str> = prompts.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["good"]);

        let warned: Vec<&PathBuf> = warnings.iter().map(|w| &w.path).collect();
        assert!(warned.contains(&&missing_name));
        assert!(warned.contains(&&no_fm));
        assert_eq!(warnings.len(), 2);
    }

    // Zero-defaults: an absent agents directory yields no prompts and no error.
    #[test]
    fn absent_agents_dir_yields_nothing() {
        let fs = FakeFs::new();
        let root = PathBuf::from("/repo");

        let (prompts, warnings) = discover_prompts(&root, &fs);

        assert!(prompts.is_empty());
        assert!(warnings.is_empty());
    }

    // --- render-context + render -----------------------------------------
    // These need a real `Store` (body lives on disk, read via `get_body_raw`),
    // so they build over a TempDir with `RealFileSystem`, mirroring context.rs.

    use crate::engine::config::Config;
    use crate::engine::fs::RealFileSystem;
    use crate::engine::store::Store;
    use tempfile::TempDir;

    fn doc_md(title: &str, doc_type: &str, related: &str) -> String {
        let related_block = if related == "[]" {
            "related: []".to_string()
        } else {
            format!("related:\n{related}")
        };
        format!(
            "---\ntitle: \"{title}\"\ntype: {doc_type}\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\n{related_block}\n---\n\n{title} body\n"
        )
    }

    fn store_from(files: &[(&str, &str)]) -> (TempDir, Store) {
        let tmp = TempDir::new().unwrap();
        for (rel_path, contents) in files {
            let full = tmp.path().join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, contents).unwrap();
        }
        let store = Store::load(tmp.path(), &Config::default()).unwrap();
        (tmp, store)
    }

    /// Build a one-line prompt with the given body template.
    fn body_prompt(body: &str) -> AgentPrompt {
        parse_prompt(&format!("---\nname: p\ndescription: d\n---\n{body}\n")).unwrap()
    }

    // AC4: a template referencing known vars renders fully, no leftover braces.
    #[test]
    fn renders_template_with_known_vars() {
        let (_tmp, store) =
            store_from(&[("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]"))]);
        let config = Config::default();
        let doc = store.resolve_shorthand("RFC-001").unwrap();
        let ctx = build_render_context(&store, &config, doc, &RealFileSystem).unwrap();

        let prompt = body_prompt("Doc {{ document.id }} type {{ document.type }}");
        let out = render(&prompt, &ctx).unwrap();

        assert_eq!(out.trim(), "Doc RFC-001 type rfc");
        assert!(!out.contains("{{") && !out.contains("}}"));
    }

    // AC5: all six document fields resolve from the selected doc.
    #[test]
    fn document_fields_resolve_from_selected_doc() {
        let (_tmp, store) =
            store_from(&[("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]"))]);
        let config = Config::default();
        let doc = store.resolve_shorthand("RFC-001").unwrap();
        let ctx = build_render_context(&store, &config, doc, &RealFileSystem).unwrap();

        let prompt = body_prompt(
            "id={{ document.id }} title={{ document.title }} type={{ document.type }} \
             body={{ document.body }} status={{ document.status }} path={{ document.path }}",
        );
        let out = render(&prompt, &ctx).unwrap();

        assert!(out.contains("id=RFC-001"), "{out}");
        assert!(out.contains("title=Base"), "{out}");
        assert!(out.contains("type=rfc"), "{out}");
        assert!(out.contains("body=Base body"), "{out}");
        assert!(out.contains("status=draft"), "{out}");
        assert!(out.contains("path=docs/rfcs/RFC-001-base.md"), "{out}");
    }

    // AC6: child_types comes only from ParentChild rules whose parent matches.
    #[test]
    fn child_types_resolve_from_parent_child_rules() {
        // Default config carries rfc->story and story->iteration; the second
        // rule (a different parent) proves filtering.
        let (_tmp, store) =
            store_from(&[("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]"))]);
        let config = Config::default();
        let doc = store.resolve_shorthand("RFC-001").unwrap();
        let ctx = build_render_context(&store, &config, doc, &RealFileSystem).unwrap();

        let prompt = body_prompt("{% for c in child_types %}{{ c }} {% endfor %}");
        let out = render(&prompt, &ctx).unwrap();

        assert!(out.contains("story"), "rfc's child is story: {out}");
        assert!(
            !out.contains("iteration"),
            "iteration is story's child, not rfc's: {out}"
        );
    }

    // AC6 companion: a type with no child rule yields an empty list; the loop
    // renders empty rather than raising an undefined error.
    #[test]
    fn child_types_empty_renders_empty_not_undefined() {
        // `adr` has no ParentChild parent rule in the default config.
        let (_tmp, store) = store_from(&[("docs/adrs/ADR-001-x.md", &doc_md("X", "adr", "[]"))]);
        let config = Config::default();
        let doc = store.resolve_shorthand("ADR-001").unwrap();
        let ctx = build_render_context(&store, &config, doc, &RealFileSystem).unwrap();

        let prompt = body_prompt("[{% for c in child_types %}{{ c }}{% endfor %}]");
        let out = render(&prompt, &ctx).unwrap();
        assert_eq!(out.trim(), "[]");
    }

    // AC7: an unknown variable errors (strict mode), never renders empty, and
    // the message names the offending var.
    #[test]
    fn unknown_variable_is_render_error_not_empty() {
        let (_tmp, store) =
            store_from(&[("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]"))]);
        let config = Config::default();
        let doc = store.resolve_shorthand("RFC-001").unwrap();
        let ctx = build_render_context(&store, &config, doc, &RealFileSystem).unwrap();

        let prompt = body_prompt("{{ document.bogus }}");
        let result = render(&prompt, &ctx);

        assert!(result.is_err(), "unknown var must error, not render empty");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("bogus"), "error should name the var: {msg}");
    }

    // AC10: ancestors/related come from resolve_chain; nearest-first ancestry,
    // target excluded, adjacent related included, forward/descendant excluded,
    // and lineage entries expose the document.* shape.
    #[test]
    fn context_ancestors_and_related_resolve_from_resolve_chain() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
            (
                "docs/iterations/ITERATION-001-leaf.md",
                &doc_md(
                    "Leaf",
                    "iteration",
                    "- implements: STORY-001\n- related-to: ADR-009",
                ),
            ),
            ("docs/adrs/ADR-009-x.md", &doc_md("Choice", "adr", "[]")),
            (
                "docs/iterations/ITERATION-002-child.md",
                &doc_md("Child", "iteration", "- implements: ITERATION-001"),
            ),
        ]);
        let config = Config::default();
        let doc = store.resolve_shorthand("ITERATION-001").unwrap();
        let ctx = build_render_context(&store, &config, doc, &RealFileSystem).unwrap();

        let prompt = body_prompt(
            "anc:{% for n in context.ancestors %}{{ n.type }} {{ n.id }}|{% endfor %}\
             rel:{% for r in context.related %}{{ r.id }}{% endfor %}",
        );
        let out = render(&prompt, &ctx).unwrap();

        // Ancestors nearest-parent-first, target excluded.
        assert!(
            out.contains("anc:story STORY-001|rfc RFC-001|"),
            "ancestors nearest-first, target excluded: {out}"
        );
        assert!(!out.contains("ITERATION-001|"), "target excluded: {out}");
        // Adjacent related doc present.
        assert!(
            out.contains("rel:ADR-009"),
            "related includes ADR-009: {out}"
        );
        // Forward (descendant) doc must NOT appear.
        assert!(
            !out.contains("ITERATION-002"),
            "forward/descendant doc must not appear: {out}"
        );

        // Lineage entries expose the same document.* shape.
        let shape = body_prompt(
            "{% for n in context.ancestors %}{{ n.body }}|{% endfor %}\
             {% for r in context.related %}{{ r.title }}{% endfor %}",
        );
        let shape_out = render(&shape, &ctx).unwrap();
        assert!(
            shape_out.contains("Mid body"),
            "ancestor body resolves: {shape_out}"
        );
        assert!(
            shape_out.contains("Choice"),
            "related title resolves: {shape_out}"
        );
    }

    // Task 8: end-to-end parse -> build_render_context -> render.
    #[test]
    fn end_to_end_parse_build_render() {
        let (_tmp, store) = store_from(&[
            ("docs/rfcs/RFC-001-base.md", &doc_md("Base", "rfc", "[]")),
            (
                "docs/stories/STORY-001-mid.md",
                &doc_md("Mid", "story", "- implements: RFC-001"),
            ),
        ]);
        let config = Config::default();
        let doc = store.resolve_shorthand("STORY-001").unwrap();
        let ctx = build_render_context(&store, &config, doc, &RealFileSystem).unwrap();

        let prompt = parse_prompt(concat!(
            "---\n",
            "name: e2e\n",
            "description: end to end\n",
            "mode: headless\n",
            "---\n",
            "Title: {{ document.title }}\n",
            "Children: {% for c in child_types %}{{ c }}{% endfor %}\n",
            "Parents: {% for n in context.ancestors %}{{ n.id }}{% endfor %}\n",
        ))
        .unwrap();
        let out = render(&prompt, &ctx).unwrap();

        assert!(out.contains("Title: Mid"), "{out}");
        assert!(
            out.contains("Children: iteration"),
            "story's child is iteration: {out}"
        );
        assert!(out.contains("Parents: RFC-001"), "{out}");
        assert_eq!(prompt.mode, RunMode::Headless);
    }
}
