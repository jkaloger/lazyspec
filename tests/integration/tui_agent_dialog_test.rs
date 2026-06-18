#![cfg(feature = "agent")]
// The CapturingRunner uses single-threaded `RefCell` interior mutability and so
// is not `Sync`, but the spawner seam is `Arc<dyn AgentRunner>`. The fake is only
// ever touched on the test thread; the lint targets cross-thread misuse.
#![allow(clippy::arc_with_non_send_sync)]

//! Slice-4 (STORY-135): the agent dialog is template-driven. Pressing `a` on a
//! selected doc lists the prompt TEMPLATES resolved for the doc's TYPE (by
//! frontmatter name + description) plus a "Custom prompt" entry. Confirming a
//! headless template renders it and dispatches via the AgentRunner seam.
//!
//! Tests inject a fake AgentRunner (capturing the AgentContext) so no `claude`
//! process is ever launched; isolation is per-test via TempDir (TestFixture).

use crate::common::TestFixture;
use crossterm::event::{KeyCode, KeyModifiers};
use lazyspec::engine::agent::{AgentContext, AgentHandle, AgentRunner};
use lazyspec::engine::config::Config;
use lazyspec::tui::agent::AgentSpawner;
use lazyspec::tui::state::{forms::AgentAction, App};
use std::cell::RefCell;
use std::process::{Command, Stdio};
use std::sync::Arc;

/// A fake runner that records every AgentContext and never launches `claude`.
/// It wraps a trivial, immediately-exiting `true` child to satisfy the handle.
struct CapturingRunner {
    captured: RefCell<Vec<AgentContext>>,
}

impl CapturingRunner {
    fn new() -> Self {
        CapturingRunner {
            captured: RefCell::new(Vec::new()),
        }
    }
}

impl AgentRunner for CapturingRunner {
    fn spawn(&self, ctx: AgentContext) -> anyhow::Result<AgentHandle> {
        self.captured.borrow_mut().push(ctx.clone());
        let child = Command::new("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(AgentHandle {
            session_id: ctx.session_id,
            child,
        })
    }
}

fn agent_prompt(
    name: &str,
    description: &str,
    mode: &str,
    tools: Option<&str>,
    body: &str,
) -> String {
    let tools_line = match tools {
        Some(t) => format!("allowed_tools: {t}\n"),
        None => String::new(),
    };
    format!("---\nname: {name}\ndescription: {description}\nmode: {mode}\n{tools_line}---\n{body}")
}

/// A config whose `rfc` type opts into the given agent template stems.
fn config_with_rfc_agents(agents: &[&str]) -> Config {
    let mut config = Config::default();
    let rfc = config
        .documents
        .types
        .iter_mut()
        .find(|t| t.name == "rfc")
        .expect("rfc type");
    rfc.agents = agents.iter().map(|s| s.to_string()).collect();
    config
}

/// As `config_with_rfc_agents`, but also sets the global `[agents] interactive`
/// command so interactive templates are offered (RFC-046 slice 5).
fn config_with_interactive(agents: &[&str], interactive: &str) -> Config {
    let mut config = config_with_rfc_agents(agents);
    config.agents.interactive = Some(interactive.to_string());
    config
}

/// Build an App over the fixture with a CapturingRunner wired into the spawner.
/// Returns the app and the shared runner so tests can inspect captured contexts.
fn app_with_capturing_runner(fixture: &TestFixture) -> (App, Arc<CapturingRunner>) {
    let store = fixture.store();
    let mut app = App::new(
        store,
        &fixture.config(),
        ratatui_image::picker::Picker::halfblocks(),
        Box::new(lazyspec::engine::fs::RealFileSystem),
    );
    let runner = Arc::new(CapturingRunner::new());
    let spawner = AgentSpawner::with_runner(runner.clone(), fixture.root());
    app.agent_spawner = spawner;
    app.selected_type = 0;
    app.selected_doc = 0;
    (app, runner)
}

fn press_with(app: &mut App, fixture: &TestFixture, config: &Config, key: KeyCode) {
    app.handle_key(key, KeyModifiers::NONE, fixture.root(), config);
}

// AC1: a doc type that resolves >=1 template -> `a` lists one entry per resolved
// template, labelled by the template's frontmatter name + description.
#[test]
fn test_dialog_lists_resolved_templates_by_name_desc() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt(
            "refine",
            "Refine against ACs",
            "headless",
            Some("Read,Edit"),
            "Refine {{ document.id }}",
        ),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    let templates: Vec<_> = app
        .agent_dialog
        .actions
        .iter()
        .filter_map(|a| match a {
            AgentAction::Template(p) => Some(p),
            AgentAction::Custom => None,
        })
        .collect();
    assert_eq!(templates.len(), 1, "one resolved template expected");
    assert_eq!(templates[0].name, "refine");
    assert_eq!(templates[0].description, "Refine against ACs");
}

// AC2: a type that exposes agents has a Custom entry alongside the templates.
#[test]
fn test_custom_entry_present_when_agents_available() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt("refine", "Refine", "headless", None, "Refine."),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    assert!(
        app.agent_dialog
            .actions
            .iter()
            .any(|a| matches!(a, AgentAction::Custom)),
        "Custom entry should be present"
    );
}

// AC3: NO built-in "Expand document"/"Create children" entries for any doc --
// only resolved templates + Custom.
#[test]
fn test_no_builtin_expand_or_create_children() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt("refine", "Refine", "headless", None, "Refine."),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));

    for action in &app.agent_dialog.actions {
        if let AgentAction::Template(p) = action {
            assert_ne!(p.name, "Expand document");
            assert_ne!(p.name, "Create children");
        }
    }
}

// AC4: a resolved HEADLESS template selected -> confirm -> the runner is invoked
// with the RENDERED prompt and the template's allowed_tools; an AgentRecord is
// created; the dialog closes.
#[test]
fn test_headless_selection_builds_agent_context_via_fake_runner() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt(
            "refine",
            "Refine against ACs",
            "headless",
            Some("Read,Edit"),
            "Refine doc {{ document.id }} of type {{ document.type }}.",
        ),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));
    // Select the refine template (the first action).
    let idx = app
        .agent_dialog
        .actions
        .iter()
        .position(|a| matches!(a, AgentAction::Template(p) if p.name == "refine"))
        .expect("refine template present");
    app.agent_dialog.selected_index = idx;
    press_with(&mut app, &fixture, &config, KeyCode::Enter);

    assert!(!app.agent_dialog.active, "dialog should close on confirm");

    let captured = runner.captured.borrow();
    assert_eq!(captured.len(), 1, "exactly one context captured");
    let ctx = &captured[0];
    assert_eq!(
        ctx.prompt.trim(),
        "Refine doc RFC-001 of type rfc.",
        "prompt is the rendered template body"
    );
    assert_eq!(ctx.allowed_tools, Some("Read,Edit".to_string()));

    assert_eq!(app.agent_spawner.records.len(), 1, "AgentRecord created");
    assert_eq!(app.agent_spawner.records[0].action, "refine");
}

// AC5: after confirming a headless action, control returns immediately -- a
// subsequent navigation key still moves the selection (no block).
#[test]
fn test_tui_responsive_after_headless_spawn() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-a.md", "RFC A", "draft");
    fixture.write_rfc("RFC-002-b.md", "RFC B", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt("refine", "Refine", "headless", None, "Refine."),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);
    app.build_doc_tree();
    app.selected_doc = 0;

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));
    let idx = app
        .agent_dialog
        .actions
        .iter()
        .position(|a| matches!(a, AgentAction::Template(_)))
        .unwrap();
    app.agent_dialog.selected_index = idx;
    press_with(&mut app, &fixture, &config, KeyCode::Enter);
    assert!(!app.agent_dialog.active);

    let before = app.selected_doc;
    press_with(&mut app, &fixture, &config, KeyCode::Char('j'));
    assert_ne!(
        app.selected_doc, before,
        "navigation still works after spawn"
    );
}

// AC7: a type that exposes agents but resolves NO templates offers only Custom;
// a type that exposes NO agents opens no dialog.
#[test]
fn test_empty_resolved_set_shows_only_custom() {
    // (a) exposes agents, but the named template is not authored -> only Custom.
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    let config = config_with_rfc_agents(&["nonexistent"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    assert_eq!(app.agent_dialog.actions.len(), 1);
    assert!(matches!(app.agent_dialog.actions[0], AgentAction::Custom));
    // The named-but-missing report is captured for the next unit.
    assert_eq!(app.agent_dialog.missing, vec!["nonexistent".to_string()]);

    // (b) exposes no agents -> no dialog opened.
    let fixture2 = TestFixture::new();
    fixture2.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    let config2 = Config::default(); // starter rfc has no `agents`
    let (mut app2, _runner2) = app_with_capturing_runner(&fixture2);

    press_with(&mut app2, &fixture2, &config2, KeyCode::Char('a'));
    assert!(
        !app2.agent_dialog.active,
        "no dialog when type exposes no agents"
    );
}

// AC8: open -> Esc -> closes, no agent spawned.
#[test]
fn test_esc_cancels_no_spawn() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt("refine", "Refine", "headless", None, "Refine."),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));
    assert!(app.agent_dialog.active);

    press_with(&mut app, &fixture, &config, KeyCode::Esc);
    assert!(!app.agent_dialog.active);
    assert!(
        runner.captured.borrow().is_empty(),
        "no agent spawned on cancel"
    );
}

// AC6: select "Custom prompt", type text, submit -> a headless agent is spawned
// via the runner with that text as the prompt (composed with the doc as context)
// and NO allowed_tools restriction beyond the runtime default (None).
#[test]
fn test_custom_prompt_spawns_with_runtime_default_tools() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt("refine", "Refine", "headless", None, "Refine."),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));
    // Navigate to / select the Custom entry, which opens the text input.
    let idx = app
        .agent_dialog
        .actions
        .iter()
        .position(|a| matches!(a, AgentAction::Custom))
        .expect("Custom entry present");
    app.agent_dialog.selected_index = idx;
    press_with(&mut app, &fixture, &config, KeyCode::Enter);
    assert_eq!(app.agent_dialog.text_input, Some(String::new()));

    // Type a prompt by feeding Char keys, then submit.
    for c in "tidy up".chars() {
        press_with(&mut app, &fixture, &config, KeyCode::Char(c));
    }
    press_with(&mut app, &fixture, &config, KeyCode::Enter);

    assert!(!app.agent_dialog.active, "dialog closes on submit");

    let captured = runner.captured.borrow();
    assert_eq!(captured.len(), 1, "exactly one context captured");
    let ctx = &captured[0];
    assert!(
        ctx.prompt.contains("tidy up"),
        "prompt carries the typed text, got: {}",
        ctx.prompt
    );
    assert!(
        ctx.prompt.contains("Test RFC"),
        "prompt carries the doc content as context, got: {}",
        ctx.prompt
    );
    assert_eq!(
        ctx.allowed_tools, None,
        "Custom prompt uses the runtime default (no tool restriction)"
    );

    assert_eq!(app.agent_spawner.records.len(), 1, "AgentRecord created");
    assert_eq!(app.agent_spawner.records[0].action, "Custom prompt");
}

// missing-report (STORY-135): when the doc's type names a template with no loaded
// file, `resolve_agent_actions.missing` is non-empty and is surfaced to the user
// (captured on the dialog's `missing` field; the footer renders it).
#[test]
fn test_missing_template_report_surfaced() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    // The type declares two agents but only `expand.md` is authored.
    fixture.write_agent_prompt(
        "expand.md",
        &agent_prompt("expand", "Expand", "headless", None, "Expand."),
    );
    let config = config_with_rfc_agents(&["expand", "ghost"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    assert!(
        app.agent_dialog.missing.contains(&"ghost".to_string()),
        "the named-but-missing template is reported, got: {:?}",
        app.agent_dialog.missing
    );
    // The resolved actions still contain the loaded `expand` template + Custom.
    assert!(
        app.agent_dialog
            .actions
            .iter()
            .any(|a| matches!(a, AgentAction::Template(p) if p.name == "expand")),
        "the loaded template is still offered"
    );
    assert!(
        app.agent_dialog
            .actions
            .iter()
            .any(|a| matches!(a, AgentAction::Custom)),
        "Custom entry is still offered"
    );
}

// AC5 (STORY-136): when `[agents] interactive` is UNSET, interactive templates
// are excluded from the dialog actions; headless entries are unaffected.
#[test]
fn interactive_entries_hidden_when_unset() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt("refine", "Refine", "headless", None, "Refine."),
    );
    fixture.write_agent_prompt(
        "pair.md",
        &agent_prompt("pair", "Pair on it", "interactive", None, "Pair."),
    );
    // interactive is None (config_with_rfc_agents leaves agents.interactive unset).
    let config = config_with_rfc_agents(&["refine", "pair"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));

    assert!(app.agent_dialog.active);
    let template_names: Vec<&str> = app
        .agent_dialog
        .actions
        .iter()
        .filter_map(|a| match a {
            AgentAction::Template(p) => Some(p.name.as_str()),
            AgentAction::Custom => None,
        })
        .collect();
    assert!(
        !template_names.contains(&"pair"),
        "interactive template must NOT be offered when [agents] interactive is unset, got: {template_names:?}"
    );
    assert!(
        template_names.contains(&"refine"),
        "headless template must still be offered, got: {template_names:?}"
    );
}

// AC2 (STORY-136): with `[agents] interactive` set, the interactive entry's label
// carries a marker distinct from headless.
#[test]
fn dialog_labels_interactive_entries() {
    use lazyspec::engine::prompt::{AgentPrompt, RunMode};
    use lazyspec::tui::state::forms::AgentAction;
    use lazyspec::tui::views::overlays::action_label;

    let interactive = AgentAction::Template(AgentPrompt {
        name: "pair".to_string(),
        description: "Pair on it".to_string(),
        mode: RunMode::Interactive,
        allowed_tools: None,
        body_template: "Pair.".to_string(),
    });
    let headless = AgentAction::Template(AgentPrompt {
        name: "refine".to_string(),
        description: "Refine".to_string(),
        mode: RunMode::Headless,
        allowed_tools: None,
        body_template: "Refine.".to_string(),
    });

    let interactive_label = action_label(&interactive);
    let headless_label = action_label(&headless);

    assert!(
        interactive_label.contains("interactive"),
        "interactive label must carry a visible marker, got: {interactive_label:?}"
    );
    assert!(
        !headless_label.contains("interactive"),
        "headless label must NOT carry the interactive marker, got: {headless_label:?}"
    );
}

// AC2/AC4 (STORY-136): selecting an interactive entry sets the suspend/run/restore
// request (cmd/prompt/doc_path), closes the dialog, and never spawns an agent.
#[test]
fn interactive_dispatch_triggers_suspend_run_restore() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "pair.md",
        &agent_prompt(
            "pair",
            "Pair on it",
            "interactive",
            Some("Read,Edit"),
            "Pair on {{ document.id }} of type {{ document.type }}.",
        ),
    );
    let interactive_cmd = r#"claude "$LAZYSPEC_PROMPT""#;
    let config = config_with_interactive(&["pair"], interactive_cmd);
    let (mut app, runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));
    let idx = app
        .agent_dialog
        .actions
        .iter()
        .position(|a| matches!(a, AgentAction::Template(p) if p.name == "pair"))
        .expect("interactive template present");
    app.agent_dialog.selected_index = idx;
    press_with(&mut app, &fixture, &config, KeyCode::Enter);

    assert!(
        !app.agent_dialog.active,
        "dialog closes on interactive dispatch"
    );

    let req = app
        .interactive_request
        .as_ref()
        .expect("interactive_request set on dispatch");
    assert_eq!(
        req.cmd, interactive_cmd,
        "cmd is the configured interactive command"
    );
    assert_eq!(
        req.prompt.trim(),
        "Pair on RFC-001 of type rfc.",
        "prompt is the rendered template body"
    );
    assert_eq!(
        req.doc_path,
        fixture.root().join("docs/rfcs/RFC-001-test.md"),
        "doc_path is the doc's full path"
    );

    assert!(
        runner.captured.borrow().is_empty(),
        "interactive dispatch must NOT spawn via the AgentRunner"
    );
}

// AC7 (STORY-136): an interactive run leaves NO AgentRecord.
#[test]
fn interactive_run_leaves_no_record() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "pair.md",
        &agent_prompt("pair", "Pair on it", "interactive", None, "Pair."),
    );
    let config = config_with_interactive(&["pair"], r#"claude "$LAZYSPEC_PROMPT""#);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));
    let idx = app
        .agent_dialog
        .actions
        .iter()
        .position(|a| matches!(a, AgentAction::Template(p) if p.name == "pair"))
        .expect("interactive template present");
    app.agent_dialog.selected_index = idx;
    press_with(&mut app, &fixture, &config, KeyCode::Enter);

    assert_eq!(
        app.agent_spawner.records.len(),
        0,
        "interactive run must leave no AgentRecord (never touches AgentSpawner)"
    );
}

// Custom entry opens the text input (full spawn is the next unit).
#[test]
fn test_custom_entry_opens_text_input() {
    let fixture = TestFixture::new();
    fixture.write_rfc("RFC-001-test.md", "Test RFC", "draft");
    fixture.write_agent_prompt(
        "refine.md",
        &agent_prompt("refine", "Refine", "headless", None, "Refine."),
    );
    let config = config_with_rfc_agents(&["refine"]);
    let (mut app, _runner) = app_with_capturing_runner(&fixture);

    press_with(&mut app, &fixture, &config, KeyCode::Char('a'));
    let idx = app
        .agent_dialog
        .actions
        .iter()
        .position(|a| matches!(a, AgentAction::Custom))
        .unwrap();
    app.agent_dialog.selected_index = idx;
    press_with(&mut app, &fixture, &config, KeyCode::Enter);

    assert!(app.agent_dialog.active);
    assert_eq!(app.agent_dialog.text_input, Some(String::new()));
}
