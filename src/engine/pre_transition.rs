//! `pre-transition` hooks (RFC-075, STORY-296): run the hooks that match a
//! status change, and check what they ask to update before anything is saved.
//!
//! This module only decides. Saving the updates together with the status is
//! `ops::update`'s job, so the CLI and the TUI share one path and neither
//! re-implements it.

use crate::engine::config::{Config, HookDef, HookEvent, Severity};
use crate::engine::document::DocMeta;
use crate::engine::fs::RealFileSystem;
use crate::engine::hooks::{hook_document, hooks_disabled, parse_reply, HookEnv, HookFinding};
use crate::engine::store::Store;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

/// A finding bound to the hook that raised it.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportedFinding {
    pub hook: String,
    pub finding: HookFinding,
}

impl ReportedFinding {
    fn new(hook: &str, finding: HookFinding) -> Self {
        Self {
            hook: hook.to_string(),
            finding,
        }
    }

    fn error(hook: &str, message: String) -> Self {
        Self::new(hook, HookFinding::error(message))
    }

    pub fn to_json(&self) -> Value {
        json!({
            "hook": self.hook,
            "severity": self.finding.severity,
            "id": self.finding.id,
            "part": self.finding.part,
            "line": self.finding.line,
            "message": self.finding.message,
        })
    }
}

impl fmt::Display for ReportedFinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let finding = &self.finding;
        let mut place = String::new();
        if let Some(id) = &finding.id {
            place.push_str(&format!(" {id}"));
            if let Some(part) = &finding.part {
                place.push_str(&format!("/{part}"));
            }
            if let Some(line) = finding.line {
                place.push_str(&format!(":{line}"));
            }
        }
        write!(
            f,
            "{} [{}]{}: {}",
            match finding.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
            },
            self.hook,
            place,
            finding.message
        )
    }
}

/// The move was refused: at least one hook reported an error.
#[derive(Debug)]
pub struct TransitionBlocked {
    pub findings: Vec<ReportedFinding>,
}

impl fmt::Display for TransitionBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "blocked by pre-transition hooks")?;
        for finding in &self.findings {
            write!(f, "\n  {finding}")?;
        }
        Ok(())
    }
}

impl std::error::Error for TransitionBlocked {}

/// A body write a hook asked for that has passed every check.
#[derive(Debug, Clone)]
pub struct PlannedUpdate {
    pub id: String,
    pub path: PathBuf,
    pub part: Option<String>,
    pub body: String,
    pub(crate) hash: String,
    /// What the target held when the hook was called, so a failed save can put it back.
    pub(crate) original: String,
}

impl PlannedUpdate {
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "part": self.part, "body": self.body })
    }
}

/// What the matching hooks allow: the updates to save with the move, and the
/// findings that did not block it.
#[derive(Debug, Default)]
pub struct Cleared {
    pub updates: Vec<PlannedUpdate>,
    pub warnings: Vec<ReportedFinding>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawUpdate {
    id: String,
    part: Option<String>,
    hash: String,
    body: String,
}

fn matching_hooks<'a>(config: &'a Config, doc: &DocMeta, from: &str, to: &str) -> Vec<&'a HookDef> {
    config
        .hooks
        .iter()
        .filter(|hook| {
            hook.event == HookEvent::PreTransition
                && hook.applies_to_type(doc.doc_type.as_str())
                && hook.applies_to_transition(from, to)
        })
        .collect()
}

fn context_documents(hook: &HookDef, store: &Store, root: &Path, doc: &DocMeta) -> Vec<Value> {
    if hook.context_types.is_empty() {
        return Vec::new();
    }
    let fs = RealFileSystem;
    let mut docs: Vec<&DocMeta> = store
        .all_docs()
        .into_iter()
        .filter(|d| d.path != doc.path)
        .filter(|d| hook.context_types.iter().any(|t| t == d.doc_type.as_str()))
        .collect();
    docs.sort_by(|a, b| a.path.cmp(&b.path));
    docs.into_iter()
        .map(|d| hook_document(d, root, &fs))
        .collect()
}

/// A document as it is on disk now, and the body a part or the body would
/// restore to.
struct Snapshot {
    hash: String,
    body: String,
    parts: Vec<(String, String)>,
}

fn snapshot(doc: &DocMeta, root: &Path) -> Snapshot {
    let json = hook_document(doc, root, &RealFileSystem);
    let text = |v: &Value| v.as_str().unwrap_or_default().to_string();
    Snapshot {
        hash: text(&json["content_hash"]),
        body: text(&json["body"]),
        parts: json["parts"]
            .as_array()
            .map(|parts| {
                parts
                    .iter()
                    .map(|p| (text(&p["name"]), text(&p["body"])))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn check_update(
    hook: &HookDef,
    raw: Value,
    store: &Store,
    root: &Path,
) -> Result<PlannedUpdate, ReportedFinding> {
    let refuse = |why: String| ReportedFinding::error(&hook.name, format!("invalid update: {why}"));
    let raw: RawUpdate = serde_json::from_value(raw).map_err(|e| {
        refuse(format!(
            "{e}; an update is `id`, `part`, `hash` and `body` only"
        ))
    })?;
    let target = store
        .resolve_shorthand(&raw.id)
        .map_err(|_| refuse(format!("no document {}", raw.id)))?;
    let current = snapshot(target, root);
    if current.hash != raw.hash {
        return Err(refuse(format!(
            "{} changed since the hook was called (hash mismatch)",
            raw.id
        )));
    }
    let original = match &raw.part {
        None => current.body,
        Some(name) => current
            .parts
            .into_iter()
            .find(|(part, _)| part == name)
            .map(|(_, body)| body)
            .ok_or_else(|| refuse(format!("{} has no part {name}", raw.id)))?,
    };
    Ok(PlannedUpdate {
        id: target.id.clone(),
        path: target.path.clone(),
        part: raw.part,
        body: raw.body,
        hash: raw.hash,
        original,
    })
}

/// Whether every update still describes the document as it is now. Called
/// again right before saving, because a hook can run for seconds.
pub fn updates_are_current(updates: &[PlannedUpdate], store: &Store, root: &Path) -> bool {
    updates.iter().all(|u| {
        store
            .resolve_shorthand(&u.id)
            .is_ok_and(|doc| snapshot(doc, root).hash == u.hash)
    })
}

/// Run the pre-transition hooks that match `from` -> `to` on `doc`, in
/// declaration order. The first hook to report an error, misbehave, or ask for an
/// update that fails a check stops the rest and blocks the move.
pub fn check(
    env: &HookEnv,
    root: &Path,
    store: &Store,
    config: &Config,
    doc: &DocMeta,
    from: &str,
    to: &str,
) -> Result<Cleared, TransitionBlocked> {
    let hooks = matching_hooks(config, doc, from, to);
    if hooks.is_empty() || hooks_disabled() {
        return Ok(Cleared::default());
    }
    if !env.trust.is_trusted(root, &config.hooks) {
        let warnings = hooks
            .iter()
            .map(|hook| {
                ReportedFinding::new(
                    &hook.name,
                    HookFinding {
                        severity: Severity::Warning,
                        id: None,
                        part: None,
                        line: None,
                        message:
                            "skipped until trusted; run `lazyspec hook trust` to trust the hooks"
                                .to_string(),
                    },
                )
            })
            .collect();
        return Ok(Cleared {
            updates: Vec::new(),
            warnings,
        });
    }

    let document = hook_document(doc, root, &RealFileSystem);
    let mut cleared = Cleared::default();
    let mut targeted: HashSet<(String, Option<String>)> = HashSet::new();
    for hook in hooks {
        let payload = json!({
            "event": HookEvent::PreTransition.as_str(),
            "hook": hook.name,
            "transition": { "from": from, "to": to },
            "documents": [document],
            "context": context_documents(hook, store, root, doc),
        });
        let bytes = serde_json::to_vec(&payload).expect("a hook payload serialises as JSON");
        let reply = match parse_reply(hook, env.runner.run(hook, root, &bytes)) {
            Ok(reply) => reply,
            Err(finding) => {
                cleared
                    .warnings
                    .push(ReportedFinding::new(&hook.name, finding));
                return Err(blocked(cleared.warnings));
            }
        };
        cleared.warnings.extend(
            reply
                .findings
                .into_iter()
                .map(|f| ReportedFinding::new(&hook.name, f)),
        );
        if has_error(&cleared.warnings) {
            return Err(blocked(cleared.warnings));
        }
        for raw in reply.updates {
            let planned = match check_update(hook, raw, store, root) {
                Ok(planned) => planned,
                Err(finding) => {
                    cleared.warnings.push(finding);
                    return Err(blocked(cleared.warnings));
                }
            };
            if !targeted.insert((planned.id.clone(), planned.part.clone())) {
                cleared.warnings.push(ReportedFinding::error(
                    &hook.name,
                    format!(
                        "invalid update: {} is already updated by another hook in this move",
                        planned.id
                    ),
                ));
                return Err(blocked(cleared.warnings));
            }
            cleared.updates.push(planned);
        }
    }
    Ok(cleared)
}

fn has_error(findings: &[ReportedFinding]) -> bool {
    findings
        .iter()
        .any(|f| f.finding.severity == Severity::Error)
}

fn blocked(findings: Vec<ReportedFinding>) -> TransitionBlocked {
    TransitionBlocked { findings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::hooks::test_support::{fixture_hook, ScriptedRunner};
    use crate::engine::store::test_support::store_from_with_config;
    use std::sync::Arc;

    const STORY: &str = "docs/stories/STORY-001-a.md";
    const OTHER: &str = "docs/stories/STORY-002-b.md";

    fn doc_text(title: &str) -> String {
        format!(
            "---\ntitle: \"{title}\"\ntype: story\nstatus: draft\nauthor: t\ndate: 2026-04-01\ntags: []\nrelated: []\n---\n\nbody of {title}\n"
        )
    }

    struct World {
        tmp: tempfile::TempDir,
        store: Store,
        config: Config,
    }

    fn world(hooks: Vec<HookDef>) -> World {
        let config = Config {
            hooks,
            ..Config::default()
        };
        let (tmp, store) =
            store_from_with_config(&[(STORY, &doc_text("A")), (OTHER, &doc_text("B"))], &config);
        World { tmp, store, config }
    }

    fn env(runner: Arc<ScriptedRunner>, w: &World) -> HookEnv {
        crate::engine::hooks::test_support::trusted_env(runner, w.tmp.path(), &w.config)
    }

    fn run(w: &World, env: &HookEnv) -> Result<Cleared, TransitionBlocked> {
        let doc = w.store.resolve_shorthand("STORY-001").unwrap();
        check(
            env,
            w.tmp.path(),
            &w.store,
            &w.config,
            doc,
            "draft",
            "review",
        )
    }

    fn hash_of(w: &World, id: &str) -> String {
        snapshot(w.store.resolve_shorthand(id).unwrap(), w.tmp.path()).hash
    }

    #[test]
    fn hooks_run_in_order_and_stop_at_the_first_error() {
        let w = world(vec![
            fixture_hook("first", HookEvent::PreTransition),
            fixture_hook("second", HookEvent::PreTransition),
            fixture_hook("third", HookEvent::PreTransition),
        ]);
        let runner = ScriptedRunner::new(|hook, _| match hook {
            "second" => r#"{"findings":[{"severity":"error","message":"no"}]}"#.to_string(),
            _ => r#"{"findings":[{"severity":"warning","message":"hm"}]}"#.to_string(),
        });
        let err = run(&w, &env(runner.clone(), &w)).unwrap_err();
        assert_eq!(runner.hooks_called(), ["first", "second"]);
        assert_eq!(err.findings.len(), 2, "the earlier warning is reported too");
        assert!(err.to_string().contains("error [second]: no"));
    }

    #[test]
    fn warnings_do_not_block_and_the_payload_carries_the_transition() {
        let mut hook = fixture_hook("gate", HookEvent::PreTransition);
        hook.context_types = vec!["story".to_string()];
        let w = world(vec![hook]);
        let runner = ScriptedRunner::new(|_, _| {
            r#"{"findings":[{"severity":"warning","message":"hm"}]}"#.to_string()
        });
        let cleared = run(&w, &env(runner.clone(), &w)).unwrap();
        assert_eq!(cleared.warnings.len(), 1);
        let input = runner.last_input();
        assert_eq!(input["transition"], json!({"from":"draft","to":"review"}));
        assert_eq!(input["documents"][0]["id"], "STORY-001");
        assert_eq!(input["context"][0]["id"], "STORY-002");
        assert!(input["context"][0]["content_hash"].is_string());
    }

    #[test]
    fn hooks_outside_the_transition_do_not_run() {
        let mut hook = fixture_hook("gate", HookEvent::PreTransition);
        hook.to = Some("complete".to_string());
        let w = world(vec![hook]);
        let runner = ScriptedRunner::new(|_, _| "{}".to_string());
        run(&w, &env(runner.clone(), &w)).unwrap();
        assert!(runner.hooks_called().is_empty());
    }

    #[test]
    fn an_untrusted_hook_is_skipped_with_a_warning() {
        let w = world(vec![fixture_hook("gate", HookEvent::PreTransition)]);
        let runner = ScriptedRunner::new(|_, _| unreachable!());
        let untrusted = crate::engine::hooks::test_support::untrusted_env(runner.clone(), &w.tmp);
        let cleared = run(&w, &untrusted).unwrap();
        assert!(runner.hooks_called().is_empty());
        assert!(cleared.warnings[0].finding.message.contains("hook trust"));
    }

    #[test]
    fn a_valid_update_is_planned_with_what_it_would_replace() {
        let w = world(vec![fixture_hook("roll", HookEvent::PreTransition)]);
        let hash = hash_of(&w, "STORY-002");
        let runner = ScriptedRunner::new(move |_, _| {
            json!({"updates":[{"id":"STORY-002","hash":hash,"body":"new"}]}).to_string()
        });
        let cleared = run(&w, &env(runner, &w)).unwrap();
        assert_eq!(cleared.updates.len(), 1);
        assert_eq!(cleared.updates[0].body, "new");
        assert!(cleared.updates[0].original.contains("body of B"));
    }

    #[test]
    fn a_bad_update_blocks_the_move() {
        let w = world(vec![fixture_hook("roll", HookEvent::PreTransition)]);
        let good = hash_of(&w, "STORY-002");
        let cases = [
            (
                json!({"id":"STORY-999","hash":"x","body":"n"}),
                "no document",
            ),
            (
                json!({"id":"STORY-002","hash":"stale","body":"n"}),
                "hash mismatch",
            ),
            (
                json!({"id":"STORY-002","part":"nope.md","hash":good,"body":"n"}),
                "no part",
            ),
            (
                json!({"id":"STORY-002","hash":good,"body":"n","title":"t"}),
                "unknown field",
            ),
        ];
        for (update, expected) in cases {
            let runner = ScriptedRunner::new(move |_, _| json!({"updates":[update]}).to_string());
            let err = run(&w, &env(runner, &w)).unwrap_err();
            assert!(err.to_string().contains(expected), "{expected}: {err}");
        }
    }

    #[test]
    fn two_hooks_updating_the_same_target_block_the_move() {
        let w = world(vec![
            fixture_hook("a", HookEvent::PreTransition),
            fixture_hook("b", HookEvent::PreTransition),
        ]);
        let hash = hash_of(&w, "STORY-002");
        let runner = ScriptedRunner::new(move |_, _| {
            json!({"updates":[{"id":"STORY-002","hash":hash,"body":"n"}]}).to_string()
        });
        let err = run(&w, &env(runner, &w)).unwrap_err();
        assert!(err.to_string().contains("already updated"), "{err}");
    }

    #[test]
    fn a_misbehaving_hook_blocks_the_move() {
        let w = world(vec![fixture_hook("gate", HookEvent::PreTransition)]);
        let runner = ScriptedRunner::new(|_, _| "not json".to_string());
        let err = run(&w, &env(runner, &w)).unwrap_err();
        assert!(err.to_string().contains("invalid JSON"), "{err}");
    }
}
