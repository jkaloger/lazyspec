mod common;

use lazyspec::cli::critical_path::{
    build_weights, run_with_writers as critical_path_run, CriticalPathArgs,
};
use lazyspec::cli::graph::{run_with_writers as graph_run, GraphArgs, GraphFormat};
use lazyspec::cli::next::{run_with_lease_view, NextArgs};
use lazyspec::cli::Cli;
use lazyspec::engine::sequencing::{Graph, LeaseView, Scope};
use clap::CommandFactory;
use serde_json::Value;
use std::collections::HashMap;

fn args(json: bool) -> NextArgs {
    NextArgs {
        scope: None,
        after: None,
        type_filter: None,
        include_leased: false,
        json,
    }
}

fn run_capturing(
    store: &lazyspec::engine::store::Store,
    config: &lazyspec::engine::config::Config,
    a: NextArgs,
    leases: LeaseView,
) -> (String, String, i32) {
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit = run_with_lease_view(store, config, a, leases, &mut stdout, &mut stderr);
    (
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
        exit,
    )
}

fn parse_json(s: &str) -> Value {
    serde_json::from_str(s.trim()).expect("valid JSON output")
}

fn write_basic_dag(fixture: &common::TestFixture) {
    fixture.write_doc(
        "docs/rfcs/RFC-001-foo.md",
        "---\ntitle: \"Foo\"\ntype: rfc\nstatus: accepted\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-a.md",
        "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: RFC-001\n---\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-a1.md",
        "---\ntitle: \"A1\"\ntype: iteration\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: STORY-001\n---\n",
    );
}

#[test]
fn ac1_next_json_emits_top_level_keys_with_expected_shape() {
    let fixture = common::TestFixture::new();
    write_basic_dag(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let (stdout, _stderr, exit) =
        run_capturing(&store, &config, args(true), LeaseView::default());

    assert_eq!(exit, 0);
    let v = parse_json(&stdout);
    assert!(v.get("ready").is_some(), "ready key");
    assert!(v.get("bottlenecks").is_some(), "bottlenecks key");
    assert!(v.get("warnings").is_some(), "warnings key");

    let ready = v["ready"].as_array().expect("ready array");
    assert!(!ready.is_empty(), "expected at least one ready item");
    for item in ready {
        assert!(item.get("id").is_some());
        let kind = item["kind"].as_str().expect("kind str");
        assert!(
            matches!(kind, "claimable" | "needs-children" | "needs-status-update"),
            "kind was {}",
            kind
        );
        assert!(item.get("leased_by").is_some(), "leased_by present");
        assert!(item["leased_by"].is_null(), "unleased fixture: null");
    }
}

#[test]
fn ac2_default_excludes_leased_candidates_from_ready() {
    let fixture = common::TestFixture::new();
    write_basic_dag(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let mut held: HashMap<String, String> = HashMap::new();
    held.insert("ITERATION-001".to_string(), "agent-x".to_string());
    let leases = LeaseView { held };

    let (stdout, _stderr, exit) = run_capturing(&store, &config, args(true), leases);

    assert_eq!(exit, 0);
    let v = parse_json(&stdout);
    let ids: Vec<&str> = v["ready"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(!ids.contains(&"ITERATION-001"), "leased id should be hidden, got {:?}", ids);
}

#[test]
fn ac2_include_leased_surfaces_leased_candidate_with_lessee() {
    let fixture = common::TestFixture::new();
    write_basic_dag(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let mut held: HashMap<String, String> = HashMap::new();
    held.insert("ITERATION-001".to_string(), "agent-x".to_string());
    let leases = LeaseView { held };

    let mut a = args(true);
    a.include_leased = true;

    let (stdout, _stderr, exit) = run_capturing(&store, &config, a, leases);

    assert_eq!(exit, 0);
    let v = parse_json(&stdout);
    let leased = v["ready"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "ITERATION-001")
        .expect("leased iteration in ready");
    assert_eq!(leased["leased_by"], "agent-x");
}

#[test]
fn ac3_scope_and_after_are_mutually_exclusive() {
    let fixture = common::TestFixture::new();
    write_basic_dag(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let mut a = args(true);
    a.scope = Some("RFC-001".to_string());
    a.after = Some("STORY-001".to_string());

    let (_stdout, stderr, exit) = run_capturing(&store, &config, a, LeaseView::default());

    assert_ne!(exit, 0);
    assert!(
        stderr.contains("mutually exclusive"),
        "stderr should mention mutually exclusive, got: {}",
        stderr
    );
}

#[test]
fn ac4_scope_with_iteration_id_is_rejected_with_hint() {
    let fixture = common::TestFixture::new();
    write_basic_dag(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let mut a = args(true);
    a.scope = Some("ITERATION-001".to_string());

    let (_stdout, stderr, exit) = run_capturing(&store, &config, a, LeaseView::default());

    assert_ne!(exit, 0);
    assert!(stderr.contains("ITERATION-001"), "stderr names the id, got: {}", stderr);
    assert!(
        stderr.to_lowercase().contains("rfc") && stderr.to_lowercase().contains("story"),
        "stderr hints scope only takes RFC/Story, got: {}",
        stderr
    );
}

#[test]
fn ac5_scope_constrains_ready_to_implements_subtree_of_anchor() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/rfcs/RFC-001-foo.md",
        "---\ntitle: \"Foo\"\ntype: rfc\nstatus: accepted\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-001-a.md",
        "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: RFC-001\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-b.md",
        "---\ntitle: \"B unrelated\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let config = fixture.config();

    let mut a = args(true);
    a.scope = Some("RFC-001".to_string());

    let (stdout, _stderr, exit) = run_capturing(&store, &config, a, LeaseView::default());

    assert_eq!(exit, 0);
    let v = parse_json(&stdout);
    let ids: Vec<&str> = v["ready"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"STORY-001"), "STORY-001 should surface, got {:?}", ids);
    assert!(!ids.contains(&"STORY-002"), "STORY-002 unrelated, got {:?}", ids);
}

#[test]
fn ac6_type_filter_constrains_ready_to_matching_doc_type() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/stories/STORY-001-a.md",
        "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-i.md",
        "---\ntitle: \"I\"\ntype: iteration\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let config = fixture.config();

    let mut a = args(true);
    a.type_filter = Some("story".to_string());

    let (stdout, _stderr, exit) = run_capturing(&store, &config, a, LeaseView::default());

    assert_eq!(exit, 0);
    let v = parse_json(&stdout);
    let ids: Vec<&str> = v["ready"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"STORY-001"), "story present, got {:?}", ids);
    assert!(!ids.contains(&"ITERATION-001"), "iteration filtered out, got {:?}", ids);
}

#[test]
fn ac7_cycle_excludes_members_from_ready_and_warns() {
    let fixture = common::TestFixture::new();
    fixture.write_doc(
        "docs/stories/STORY-001-a.md",
        "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- blocks: STORY-002\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-b.md",
        "---\ntitle: \"B\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- blocks: STORY-001\n---\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-c.md",
        "---\ntitle: \"C\"\ntype: iteration\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    );

    let store = fixture.store();
    let config = fixture.config();

    let (stdout, _stderr, exit) =
        run_capturing(&store, &config, args(true), LeaseView::default());

    assert_eq!(exit, 0);
    let v = parse_json(&stdout);
    let ids: Vec<&str> = v["ready"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert!(!ids.contains(&"STORY-001"), "cycle node A excluded, got {:?}", ids);
    assert!(!ids.contains(&"STORY-002"), "cycle node B excluded, got {:?}", ids);

    let warnings = v["warnings"].as_array().expect("warnings array");
    let cycle = warnings
        .iter()
        .find(|w| w["type"] == "cycle")
        .expect("cycle warning");
    let warning_ids: Vec<&str> = cycle["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i.as_str().unwrap())
        .collect();
    assert!(warning_ids.contains(&"STORY-001"), "warning names A, got {:?}", warning_ids);
    assert!(warning_ids.contains(&"STORY-002"), "warning names B, got {:?}", warning_ids);
}

fn graph_args(format: GraphFormat) -> GraphArgs {
    GraphArgs {
        scope: None,
        after: None,
        format,
    }
}

fn run_graph_capturing(
    store: &lazyspec::engine::store::Store,
    config: &lazyspec::engine::config::Config,
    a: GraphArgs,
) -> (String, String, i32) {
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit = graph_run(store, config, a, &mut stdout, &mut stderr);
    (
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
        exit,
    )
}

fn write_graph_fixture(fixture: &common::TestFixture) {
    // 3 nodes, 2 blocks edges (A->B, B->C), 1 implements edge (I implements A)
    fixture.write_doc(
        "docs/stories/STORY-001-a.md",
        "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- blocks: STORY-002\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-b.md",
        "---\ntitle: \"B\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- blocks: STORY-003\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-003-c.md",
        "---\ntitle: \"C\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\n---\n",
    );
    fixture.write_doc(
        "docs/iterations/ITERATION-001-i.md",
        "---\ntitle: \"I\"\ntype: iteration\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\nrelated:\n- implements: STORY-001\n---\n",
    );
}

#[test]
fn ac3_graph_scope_and_after_are_mutually_exclusive() {
    let fixture = common::TestFixture::new();
    write_graph_fixture(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let mut a = graph_args(GraphFormat::D2);
    a.scope = Some("STORY-001".to_string());
    a.after = Some("STORY-002".to_string());

    let (_stdout, stderr, exit) = run_graph_capturing(&store, &config, a);

    assert_ne!(exit, 0);
    assert!(
        stderr.contains("mutually exclusive"),
        "stderr should mention mutually exclusive, got: {}",
        stderr
    );
}

#[test]
fn ac4_graph_scope_with_iteration_id_is_rejected_with_hint() {
    let fixture = common::TestFixture::new();
    write_graph_fixture(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let mut a = graph_args(GraphFormat::D2);
    a.scope = Some("ITERATION-001".to_string());

    let (_stdout, stderr, exit) = run_graph_capturing(&store, &config, a);

    assert_ne!(exit, 0);
    assert!(stderr.contains("ITERATION-001"), "stderr names the id, got: {}", stderr);
    assert!(
        stderr.to_lowercase().contains("rfc") && stderr.to_lowercase().contains("story"),
        "stderr hints scope only takes RFC/Story, got: {}",
        stderr
    );
}

#[test]
fn ac8_graph_d2_format_emits_d2_tokens() {
    let fixture = common::TestFixture::new();
    write_graph_fixture(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let (stdout, _stderr, exit) =
        run_graph_capturing(&store, &config, graph_args(GraphFormat::D2));

    assert_eq!(exit, 0);
    assert!(!stdout.is_empty(), "expected non-empty stdout");
    assert!(stdout.contains("direction: down"), "expected d2 header, got: {}", stdout);
    assert!(stdout.contains("->"), "expected an edge arrow, got: {}", stdout);
}

#[test]
fn ac8_graph_dot_format_emits_dot_tokens() {
    let fixture = common::TestFixture::new();
    write_graph_fixture(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let (stdout, _stderr, exit) =
        run_graph_capturing(&store, &config, graph_args(GraphFormat::Dot));

    assert_eq!(exit, 0);
    assert!(!stdout.is_empty(), "expected non-empty stdout");
    assert!(stdout.contains("digraph G {"), "expected dot header, got: {}", stdout);
    assert!(stdout.contains("->"), "expected an edge arrow, got: {}", stdout);
}

#[test]
fn ac8_graph_json_format_emits_nodes_and_edges() {
    let fixture = common::TestFixture::new();
    write_graph_fixture(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let (stdout, _stderr, exit) =
        run_graph_capturing(&store, &config, graph_args(GraphFormat::Json));

    assert_eq!(exit, 0);
    let v: Value = serde_json::from_str(stdout.trim()).expect("valid JSON output");
    let nodes = v["nodes"].as_array().expect("nodes array");
    let edges = v["edges"].as_array().expect("edges array");
    assert!(!nodes.is_empty(), "nodes non-empty");
    assert!(!edges.is_empty(), "edges non-empty");
}

#[test]
fn ac13_graph_help_documents_scope_after_format() {
    let mut cmd = Cli::command();
    let graph_cmd = cmd
        .find_subcommand_mut("graph")
        .expect("graph subcommand present");
    let help = graph_cmd.render_help().to_string();
    assert!(help.contains("--scope"), "help mentions --scope, got: {}", help);
    assert!(help.contains("--after"), "help mentions --after, got: {}", help);
    assert!(help.contains("--format"), "help mentions --format, got: {}", help);
}

fn critical_path_args(json: bool) -> CriticalPathArgs {
    CriticalPathArgs {
        scope: None,
        after: None,
        json,
    }
}

fn run_critical_path_capturing(
    store: &lazyspec::engine::store::Store,
    config: &lazyspec::engine::config::Config,
    a: CriticalPathArgs,
) -> (String, String, i32) {
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let exit = critical_path_run(store, config, a, &mut stdout, &mut stderr);
    (
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
        exit,
    )
}

fn write_critical_path_fixture(fixture: &common::TestFixture) {
    // A→B→C blocks chain. Priorities: A=must, B=must, C=should. Should
    // produce a non-trivial weighted path through the chain.
    fixture.write_doc(
        "docs/stories/STORY-001-a.md",
        "---\ntitle: \"A\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\npriority: must\nrelated:\n- blocks: STORY-002\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-002-b.md",
        "---\ntitle: \"B\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\npriority: must\nrelated:\n- blocks: STORY-003\n---\n",
    );
    fixture.write_doc(
        "docs/stories/STORY-003-c.md",
        "---\ntitle: \"C\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\npriority: should\n---\n",
    );
}

#[test]
fn ac3_critical_path_scope_and_after_are_mutually_exclusive() {
    let fixture = common::TestFixture::new();
    write_critical_path_fixture(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let mut a = critical_path_args(true);
    a.scope = Some("STORY-001".to_string());
    a.after = Some("STORY-002".to_string());

    let (_stdout, stderr, exit) = run_critical_path_capturing(&store, &config, a);

    assert_ne!(exit, 0);
    assert!(
        stderr.contains("mutually exclusive"),
        "stderr should mention mutually exclusive, got: {}",
        stderr
    );
}

#[test]
fn ac4_critical_path_scope_with_iteration_id_is_rejected_with_hint() {
    let fixture = common::TestFixture::new();
    write_critical_path_fixture(&fixture);
    fixture.write_doc(
        "docs/iterations/ITERATION-001-i.md",
        "---\ntitle: \"I\"\ntype: iteration\nstatus: draft\nauthor: t\ndate: 2026-01-01\ntags: []\npriority: must\n---\n",
    );
    let store = fixture.store();
    let config = fixture.config();

    let mut a = critical_path_args(true);
    a.scope = Some("ITERATION-001".to_string());

    let (_stdout, stderr, exit) = run_critical_path_capturing(&store, &config, a);

    assert_ne!(exit, 0);
    assert!(
        stderr.contains("ITERATION-001"),
        "stderr names the id, got: {}",
        stderr
    );
    assert!(
        stderr.to_lowercase().contains("rfc") && stderr.to_lowercase().contains("story"),
        "stderr hints scope only takes RFC/Story, got: {}",
        stderr
    );
}

#[test]
fn ac9_critical_path_json_returns_ordered_doc_ids_matching_engine() {
    let fixture = common::TestFixture::new();
    write_critical_path_fixture(&fixture);
    let store = fixture.store();
    let config = fixture.config();

    let (stdout, _stderr, exit) =
        run_critical_path_capturing(&store, &config, critical_path_args(true));

    assert_eq!(exit, 0);
    let v: Value = serde_json::from_str(stdout.trim()).expect("valid JSON output");
    let cli_path: Vec<String> = v
        .as_array()
        .expect("JSON array of ids")
        .iter()
        .map(|i| i.as_str().expect("string id").to_string())
        .collect();

    let graph = Graph::from_store(&store);
    let weights = build_weights(&store, &config);
    let engine_path: Vec<String> = graph
        .critical_path(Scope::All, &weights)
        .into_iter()
        .map(|n| n.0)
        .collect();

    assert_eq!(
        cli_path, engine_path,
        "CLI path must match engine output exactly"
    );
    assert!(!cli_path.is_empty(), "expected non-empty path");
}

#[test]
fn ac13_critical_path_help_documents_scope_after_json() {
    let mut cmd = Cli::command();
    let cp_cmd = cmd
        .find_subcommand_mut("critical-path")
        .expect("critical-path subcommand present");
    let help = cp_cmd.render_help().to_string();
    assert!(
        help.contains("--scope"),
        "help mentions --scope, got: {}",
        help
    );
    assert!(
        help.contains("--after"),
        "help mentions --after, got: {}",
        help
    );
    assert!(
        help.contains("--json"),
        "help mentions --json, got: {}",
        help
    );
}
