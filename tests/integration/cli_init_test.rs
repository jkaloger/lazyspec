use lazyspec::engine::config::{
    starter_edges, starter_relationships, starter_types, Config, RelSelector, RelationshipDef,
    Traversal, TypeSelector,
};
use lazyspec::engine::store::Store;
use std::fs;
use tempfile::TempDir;

/// Parse the `.lazyspec.toml` that `init` wrote into `root`.
fn parse_written_config(root: &std::path::Path) -> Config {
    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    Config::parse(&content).unwrap()
}

#[test]
fn init_creates_config_and_directories() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    assert!(root.join(".lazyspec.toml").exists());
    assert!(root.join("docs/rfcs").is_dir());
    assert!(root.join("docs/adrs").is_dir());
    assert!(root.join("docs/stories").is_dir());
    assert!(root.join("docs/iterations").is_dir());
    assert!(root.join(".lazyspec/templates").is_dir());

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    assert!(content.contains("[[types]]"));
    assert!(!content.contains("[directories]"));
}

// AC5: init refuses when a config already exists and writes nothing.
#[test]
fn init_does_not_overwrite_existing_config() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let sentinel = "# custom config";
    fs::write(root.join(".lazyspec.toml"), sentinel).unwrap();

    let result = lazyspec::cli::init::run(root);
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("already exists"),
        "error should say the config already exists, got: {err}"
    );

    let after = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    assert_eq!(after, sentinel, "init must not touch an existing config");
}

// AC1: init emits the 4 historical relationships with their inverses.
#[test]
fn init_writes_relationships_block() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let config = parse_written_config(root);

    let mut rels = config.relationships.clone();
    rels.sort_by(|a, b| a.name.cmp(&b.name));
    let mut expected = starter_relationships();
    expected.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(
        rels, expected,
        "init relationships must equal the canonical starter set"
    );

    assert_eq!(
        config.inverse_of("implements"),
        Some("implemented-by"),
        "implements -> implemented-by"
    );
    assert_eq!(
        config.inverse_of("supersedes"),
        Some("superseded-by"),
        "supersedes -> superseded-by"
    );
    assert_eq!(
        config.inverse_of("blocks"),
        Some("blocked-by"),
        "blocks -> blocked-by"
    );

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    assert!(
        content.contains("[[relationships]]"),
        "config should contain a [[relationships]] block"
    );
}

// AC2: related-to is symmetric -- no inverse, in both the parsed registry and the raw text.
#[test]
fn init_related_to_is_symmetric_no_inverse() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let config = parse_written_config(root);
    let related_to = config
        .relationship_by_name("related-to")
        .expect("related-to must be present");
    assert_eq!(
        *related_to,
        RelationshipDef {
            name: "related-to".to_string(),
            inverse: None,
            github_native: None,
            traversal: None,
        },
        "related-to must be symmetric (no inverse)"
    );

    // The serialized related-to table must not carry an `inverse` key. Isolate
    // the block between its header and the next table to scope the check.
    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    let block_start = content
        .find("name = \"related-to\"")
        .expect("related-to block should exist in the raw text");
    let after = &content[block_start..];
    let block_end = after[1..].find("\n[").map(|i| i + 1).unwrap_or(after.len());
    let related_to_block = &after[..block_end];
    assert!(
        !related_to_block.contains("inverse"),
        "related-to block must not attach an inverse, got:\n{related_to_block}"
    );
}

// STORY-259 AC4: init states the three starter constraints and the hierarchy
// they hang on as `[[edges]]`, and says nothing at all about `[[rules]]` -- not
// a block, not a bare `rules = []` key (which would collide with any `[[rules]]`
// header appended after it).
#[test]
fn init_writes_edges_and_no_rules() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let config = parse_written_config(root);
    assert_eq!(
        config.edges,
        starter_edges(),
        "init edges must equal the canonical starter_edges()"
    );
    let names: Vec<&str> = config.edges.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "stories-need-rfcs",
            "iterations-need-stories",
            "adrs-need-relations",
            "implements-traversal",
            "related-to-traversal",
        ],
        "three starter constraints and the two blanket traversal rows, by name"
    );

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    assert!(
        content.contains("[[edges]]"),
        "config should contain an [[edges]] block, got:\n{content}"
    );
    assert!(
        !content.contains("[[rules]]") && !content.contains("rules ="),
        "config must carry no rules key at all, got:\n{content}"
    );
    for name in [
        "stories-need-rfcs",
        "iterations-need-stories",
        "adrs-need-relations",
        "implements-traversal",
        "related-to-traversal",
    ] {
        assert!(
            content.contains(name),
            "written config should name the {name} edge, got:\n{content}"
        );
    }
}

// STORY-261 AC6: the scaffolded config declares traversal in ONE table. `init`'s
// config is the worked example every new project reads (ADR-011), and an example
// that also marked `[[relationships]]` would teach the shape the edge table
// replaced -- so no starter relationship carries a marker, and every `traversal`
// key in the written file sits inside an `[[edges]]` block.
#[test]
fn init_states_traversal_only_on_edges() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let config = parse_written_config(root);
    for rel in &config.relationships {
        assert_eq!(
            rel.traversal, None,
            "relationship {} must state no traversal marker",
            rel.name
        );
    }

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    let mut table = "";
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            table = trimmed;
        }
        if trimmed.starts_with("traversal") {
            assert_eq!(
                table, "[[edges]]",
                "traversal declared under {table}:\n{content}"
            );
        }
    }
}

// The starter set is the first config in the repo where a wildcard row and a
// concrete row cover the same triple: `iterations-need-stories` matches
// `iteration --implements--> story` and so does `implements-traversal`. ADR-031
// composes overlapping rows that name the same role, and only rows that
// *disagree* are refused -- so the five-row set must strict-load. Were the
// overlap read as a contradiction, `init` would scaffold a config it cannot read
// back.
#[test]
fn the_starter_wildcard_and_concrete_chain_rows_are_not_a_contradiction() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    let config = Config::parse(&content).expect("the five-row starter set must strict-load");

    let role = |name: &str| {
        config
            .edges
            .iter()
            .find(|edge| edge.name == name)
            .unwrap_or_else(|| panic!("starter set should declare {name}"))
            .traversal
    };
    assert_eq!(
        role("iterations-need-stories"),
        Some(Traversal::Chain),
        "the concrete row keeps the role it wrote"
    );
    assert_eq!(
        role("implements-traversal"),
        Some(Traversal::Chain),
        "so does the wildcard row it overlaps"
    );
    assert_eq!(role("related-to-traversal"), Some(Traversal::Related));
}

// AC4: a freshly init-ed project loads under strict load, round-trips identically
// to the builtins, and validates clean (no strict-load / unknown-relationship errors).
#[test]
fn init_project_loads_strict_and_validates_clean() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    // Strict load of the written config must succeed.
    let content = fs::read_to_string(root.join(".lazyspec.toml")).unwrap();
    let parsed = Config::parse(&content);
    assert!(
        parsed.is_ok(),
        "fresh config must load under strict parse, got: {:?}",
        parsed.err()
    );
    let config = parsed.unwrap();

    // Round-trip equality with the pre-refactor builtins.
    assert_eq!(
        config.documents.types,
        starter_types(),
        "types must round-trip to starter_types()"
    );
    assert_eq!(
        config.edges,
        starter_edges(),
        "edges must round-trip to starter_edges()"
    );
    let mut rels = config.relationships.clone();
    rels.sort_by(|a, b| a.name.cmp(&b.name));
    let mut expected_rels = starter_relationships();
    expected_rels.sort_by(|a, b| a.name.cmp(&b.name));
    assert_eq!(
        rels, expected_rels,
        "relationships must round-trip to starter_relationships()"
    );

    // Validate over the freshly scaffolded project: only the convention/dictum
    // skeletons exist, so there must be no errors at all.
    let store = Store::load(root, &config).unwrap();
    let result = store.validate_full(&config);
    assert!(
        result.errors.is_empty(),
        "fresh project should validate with no errors, got: {:?}",
        result.errors
    );
}

/// Scaffold a project, create RFC-001 and STORY-001 in it, link `from` to `to`
/// with `relation`, and return STORY-001's `context --json`. Everything the walks
/// read comes from the config `init` wrote -- no test-authored edge row.
fn story_context_after_linking(from: &str, relation: &str, to: &str) -> serde_json::Value {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    lazyspec::cli::init::run(root).unwrap();

    let config = parse_written_config(root);
    let mut store = Store::load(root, &config).unwrap();
    for (doc_type, title) in [("rfc", "a"), ("story", "b")] {
        lazyspec::cli::create::run(root, &config, &store, doc_type, title, "tester", |_| {})
            .unwrap();
        store = Store::load(root, &config).unwrap();
    }

    lazyspec::cli::link::link_with_config(
        root,
        &store,
        from,
        relation,
        to,
        &lazyspec::engine::fs::RealFileSystem,
        Some(&config),
    )
    .unwrap();

    let store = Store::load(root, &config).unwrap();
    let json = lazyspec::cli::context::run_json(&store, "STORY-001", 1).unwrap();
    serde_json::from_str(&json).unwrap()
}

fn ids_under(context: &serde_json::Value, key: &str) -> Vec<String> {
    context[key]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|node| node["id"].as_str().map(str::to_string))
        .collect()
}

// STORY-261 AC6, the chain half: a project scaffolded by `init` and nothing else
// walks its hierarchy. The chain a `context` call reports is the one the starter
// `[[edges]]` set declares, so a starter set that constrained without walking
// would leave every new project's chain a chain of one.
#[test]
fn a_scaffolded_project_walks_the_chain_from_its_starter_edges() {
    let context = story_context_after_linking("STORY-001", "implements", "RFC-001");

    let chain = ids_under(&context, "chain");
    assert!(
        chain.contains(&"RFC-001".to_string()),
        "the implemented rfc belongs to the story's chain, got {chain:?}"
    );
    assert!(
        chain.contains(&"STORY-001".to_string()),
        "the target is its own chain root, got {chain:?}"
    );
}

// STORY-261 AC6, the related half: a scaffolded project's neighbourhood comes
// from the starter set's blanket `related-to` row and nowhere else, now that no
// starter relationship carries a marker. The link is declared on the RFC, so
// the story reaches it only by the `related` walk reading the edge backwards --
// nothing in the story's own frontmatter mentions it. A `related-to` link is a
// neighbour and no chain edge, so it leaves the chain alone.
#[test]
fn a_scaffolded_project_walks_the_neighbourhood_from_its_starter_edges() {
    let context = story_context_after_linking("RFC-001", "related-to", "STORY-001");

    assert_eq!(
        ids_under(&context, "related"),
        vec!["RFC-001".to_string()],
        "the linked rfc is the story's neighbourhood"
    );
    let chain = ids_under(&context, "chain");
    assert!(
        !chain.contains(&"RFC-001".to_string()),
        "a related link is no chain edge, got {chain:?}"
    );
}

// A fresh config must leave room for further `[[edges]]` rows: the starter set
// is written as array-of-tables headers, so a hand-appended row parses beside
// them rather than colliding with a bare `edges = ...` key.
#[test]
fn init_config_accepts_an_appended_edges_block() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let config_path = root.join(".lazyspec.toml");
    let content = fs::read_to_string(&config_path).unwrap();
    assert!(
        !content.contains("edges = "),
        "edges must serialize as [[edges]] headers, got:\n{content}"
    );

    fs::write(
        &config_path,
        format!(
            "{content}\n[[edges]]\nname = \"iterations-implement-stories\"\n\
             from = \"iteration\"\nto = [\"story\", \"rfc\"]\nvia = \"implements\"\n"
        ),
    )
    .unwrap();

    let config = parse_written_config(root);
    let edge = config
        .edges
        .iter()
        .find(|e| e.name == "iterations-implement-stories")
        .expect("appended edge should parse");
    assert_eq!(
        edge.from,
        TypeSelector::Types(vec!["iteration".to_string()])
    );
    assert_eq!(
        edge.to,
        TypeSelector::Types(vec!["story".to_string(), "rfc".to_string()])
    );
    assert_eq!(edge.via, RelSelector::Named(vec!["implements".to_string()]));

    // Non-empty edges still round-trip out as an `[[edges]]` block, so a config
    // rewrite (TUI settings editor, web view) cannot silently drop them.
    let emitted = config.to_toml().unwrap();
    assert!(
        emitted.contains("[[edges]]"),
        "declared edges must survive to_toml, got:\n{emitted}"
    );
    assert_eq!(Config::parse(&emitted).unwrap().edges, config.edges);
}

#[test]
fn init_creates_convention_skeleton_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    lazyspec::cli::init::run(root).unwrap();

    let index = root.join("docs/convention/convention/index.md");
    let example = root.join("docs/convention/convention/example.md");

    assert!(index.exists(), "convention index.md should be created");
    assert!(example.exists(), "convention example.md should be created");

    let index_content = fs::read_to_string(&index).unwrap();
    assert!(index_content.contains("type: convention"));
    assert!(index_content.contains("status: draft"));
    assert!(index_content.contains("author: \"unknown\""));
    assert!(index_content.contains("tags: []"));
    // Date should be YYYY-MM-DD format
    let date_re = regex::Regex::new(r"date: \d{4}-\d{2}-\d{2}").unwrap();
    assert!(
        date_re.is_match(&index_content),
        "index.md should contain a date in YYYY-MM-DD format"
    );

    let example_content = fs::read_to_string(&example).unwrap();
    assert!(example_content.contains("type: dictum"));
    assert!(example_content.contains("status: draft"));
    assert!(example_content.contains("author: \"unknown\""));
    assert!(example_content.contains("tags: [example]"));
    assert!(
        date_re.is_match(&example_content),
        "example.md should contain a date in YYYY-MM-DD format"
    );
}

#[test]
fn init_does_not_overwrite_existing_convention_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let convention_dir = root.join("docs/convention/convention");
    fs::create_dir_all(&convention_dir).unwrap();
    fs::write(convention_dir.join("index.md"), "# my custom convention").unwrap();

    lazyspec::cli::init::run(root).unwrap();

    let content = fs::read_to_string(convention_dir.join("index.md")).unwrap();
    assert_eq!(content, "# my custom convention");
}
