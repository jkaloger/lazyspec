//! User-defined hooks (RFC-075): external commands lazyspec feeds documents as
//! JSON on stdin and reads findings back from on stdout.
//!
//! This module is the `validate` event. The process is behind [`HookRunner`] so
//! a test drives the whole protocol without spawning anything, and every
//! surface (`validate`, `status`, the TUI) reaches hooks through
//! [`validate_issues`] so none of them re-implements the protocol.

mod trust;

pub use trust::TrustStore;

use crate::engine::config::{Config, HookDef, HookEvent, Severity};
use crate::engine::doc_json::doc_to_json;
use crate::engine::document::DocMeta;
use crate::engine::fs::{FileSystem, RealFileSystem};
use crate::engine::hashing::sha256_hex;
use crate::engine::store::{read_body, read_part_body};
use crate::engine::subprocess::{output_with_timeout_and_input, TimedOut};
use crate::engine::validation::ValidationIssue;
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

/// One finding a hook reported, before it is bound to the hook that raised it.
/// #[derive(Debug, Clone, PartialEq)]
pub struct HookFinding {
    pub severity: Severity,
    pub id: Option<String>,
    pub part: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

impl HookFinding {
    pub(crate) fn error(message: String) -> Self {
        Self {
            severity: Severity::Error,
            id: None,
            part: None,
            line: None,
            message,
        }
    }

    pub(crate) fn into_issue(self, hook: &str) -> (Severity, ValidationIssue) {
        (
            self.severity,
            ValidationIssue::Hook {
                hook: hook.to_string(),
                id: self.id,
                part: self.part,
                line: self.line,
                message: self.message,
            },
        )
    }
}

pub struct HookProcess {
    /// `None` when the process was killed by a signal.
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// `root` is the project root, the hook's working directory; `scripts_root` is
/// `config.docs_root(root)`, where a `run` path is resolved.
pub trait HookRunner: Send + Sync {
    fn run(
        &self,
        hook: &HookDef,
        root: &Path,
        scripts_root: &Path,
        input: &[u8],
    ) -> Result<HookProcess>;
}

/// Spawns `hook.run` with the project root as its working directory and a path
/// program resolved against the scripts root. No shell.
pub struct ProcessRunner;

impl HookRunner for ProcessRunner {
    fn run(
        &self,
        hook: &HookDef,
        root: &Path,
        scripts_root: &Path,
        input: &[u8],
    ) -> Result<HookProcess> {
        let program = &hook.run[0];
        let program = if program.contains('/') {
            scripts_root.join(program)
        } else {
            PathBuf::from(program)
        };
        let mut command = Command::new(program);
        command.args(&hook.run[1..]).current_dir(root);
        let output = output_with_timeout_and_input(command, input.to_vec(), hook.timeout())?;
        Ok(HookProcess {
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Deserialize)]
struct Reply {
    #[serde(default)]
    findings: Vec<ReplyFinding>,
    #[serde(default)]
    updates: Vec<Value>,
}

#[derive(Deserialize)]
struct ReplyFinding {
    id: Option<String>,
    part: Option<String>,
    line: Option<u32>,
    severity: Severity,
    message: String,
}

fn with_stderr(message: String, stderr: &str) -> String {
    let stderr = stderr.trim();
    if stderr.is_empty() {
        return message;
    }
    format!("{message}; stderr: {stderr}")
}

/// What a hook's run means as findings. Every way a hook can misbehave becomes
/// one error finding that names it (RFC-075): the caller never sees a `Result`.
fn interpret(hook: &HookDef, run: Result<HookProcess>) -> Vec<HookFinding> {
    let reply = match parse_reply(hook, run) {
        Ok(reply) => reply,
        Err(finding) => return vec![finding],
    };
    if !reply.updates.is_empty() {
        return vec![HookFinding::error(with_stderr(
            "returned `updates`, which a validate hook may not".to_string(),
            &reply.stderr,
        ))];
    }
    reply.findings
}

/// What a hook that ran cleanly said: its findings, and the updates it asked for
/// still unchecked.
pub(crate) struct ParsedReply {
    pub findings: Vec<HookFinding>,
    pub updates: Vec<Value>,
    stderr: String,
}

/// A hook that timed out, crashed, exited non-zero or printed something that is
/// not the protocol is one error finding.
pub(crate) fn parse_reply(
    hook: &HookDef,
    run: Result<HookProcess>,
) -> std::result::Result<ParsedReply, HookFinding> {
    let process = match run {
        Ok(process) => process,
        Err(e) => match e.downcast_ref::<TimedOut>() {
            Some(timed_out) => {
                return Err(HookFinding::error(with_stderr(
                    format!("timed out after {}s", hook.timeout().as_secs()),
                    &String::from_utf8_lossy(&timed_out.stderr),
                )))
            }
            None => return Err(HookFinding::error(format!("could not run: {e}"))),
        },
    };
    if process.code != Some(0) {
        let status = match process.code {
            Some(code) => format!("exited with status {code}"),
            None => "was killed by a signal".to_string(),
        };
        return Err(HookFinding::error(with_stderr(status, &process.stderr)));
    }
    let reply: Reply = match serde_json::from_str(&process.stdout) {
        Ok(reply) => reply,
        Err(e) => {
            return Err(HookFinding::error(with_stderr(
                format!("printed invalid JSON: {e}"),
                &process.stderr,
            )))
        }
    };
    Ok(ParsedReply {
        findings: reply
            .findings
            .into_iter()
            .map(|f| HookFinding {
                severity: f.severity,
                id: f.id,
                part: f.part,
                line: f.line,
                message: f.message,
            })
            .collect(),
        updates: reply.updates,
        stderr: process.stderr,
    })
}

/// A document as a hook reads it: the `show --json` shape with `body`, each
/// part with its `body`, and a `content_hash` over all of them -- the value a
/// `pre-transition` update echoes back to prove the document did not move.
pub(crate) fn hook_document(doc: &DocMeta, root: &Path, fs: &dyn FileSystem) -> Value {
    let mut json = doc_to_json(doc);
    let body = read_body(root, &doc.path, fs).unwrap_or_default();
    let mut hashed = body.clone();
    if let Some(entries) = json.get_mut("parts").and_then(|p| p.as_array_mut()) {
        for (part, entry) in doc.parts.iter().zip(entries.iter_mut()) {
            let part_body = read_part_body(root, &part.path, fs).unwrap_or_default();
            hashed.push_str(&part.name);
            hashed.push_str(&part_body);
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("body".to_string(), Value::String(part_body));
            }
        }
    }
    json["body"] = Value::String(body);
    json["content_hash"] = Value::String(sha256_hex(hashed.as_bytes()));
    json
}

fn validate_input(
    hook: &HookDef,
    root: &Path,
    docs: &[&DocMeta],
    fs: &dyn FileSystem,
) -> Option<Vec<u8>> {
    let mut matching: Vec<&DocMeta> = docs
        .iter()
        .copied()
        .filter(|doc| !doc.validate_ignore && hook.applies_to_type(&doc.doc_type.to_string()))
        .collect();
    if matching.is_empty() {
        return None;
    }
    matching.sort_by(|a, b| a.path.cmp(&b.path));
    let documents: Vec<Value> = matching
        .into_iter()
        .map(|doc| hook_document(doc, root, fs))
        .collect();
    let payload = serde_json::json!({
        "event": HookEvent::Validate.as_str(),
        "hook": hook.name,
        "documents": documents,
    });
    Some(serde_json::to_vec(&payload).expect("a hook payload serialises as JSON"))
}

/// What hooks run against: how a process is spawned, whose trust decides, how documents are read, and whether `--no-hooks`
/// turned them all off. A surface that outlives one call (the TUI) owns one
/// and hands clones to its worker; a test injects fakes.
#[derive(Clone)]
pub struct HookEnv {
    pub runner: Arc<dyn HookRunner>,
    pub trust: Arc<TrustStore>,
    pub fs: Arc<dyn FileSystem + Send + Sync>,
    /// `--no-hooks`: every hook is skipped on every surface. Lives here, not in
    /// a static, so a config reload cannot lose it.
    pub disabled: bool,
}

impl HookEnv {
    /// The real process runner, file system and the user's trust store.
    pub fn process(no_hooks: bool) -> Self {
        Self {
            runner: Arc::new(ProcessRunner),
            trust: Arc::new(TrustStore::user_local()),
            fs: Arc::new(RealFileSystem),
            disabled: no_hooks,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Gate {
    /// Nothing to run: no `validate` hook is configured, or `--no-hooks`.
    Inactive,
    Untrusted(Vec<String>),
    Trusted,
}

fn validate_hooks(config: &Config) -> impl Iterator<Item = &HookDef> {
    config
        .hooks
        .iter()
        .filter(|hook| hook.event == HookEvent::Validate)
}

fn gate(env: &HookEnv, root: &Path, config: &Config) -> Gate {
    let names: Vec<String> = validate_hooks(config).map(|h| h.name.clone()).collect();
    if env.disabled || names.is_empty() {
        return Gate::Inactive;
    }
    if env.trust.is_trusted(root, config) {
        return Gate::Trusted;
    }
    Gate::Untrusted(names)
}

fn run_validate_hooks(
    env: &HookEnv,
    root: &Path,
    docs: &[&DocMeta],
    config: &Config,
) -> Vec<(Severity, ValidationIssue)> {
    let mut issues = Vec::new();
    for hook in validate_hooks(config) {
        let Some(input) = validate_input(hook, root, docs, &*env.fs) else {
            continue;
        };
        let run = env.runner.run(hook, root, &config.docs_root(root), &input);
        let findings = interpret(hook, run);
        issues.extend(findings.into_iter().map(|f| f.into_issue(&hook.name)));
    }
    issues
}

/// Every `validate` hook's findings as the validation report carries them, plus
/// one warning naming `hook trust` when the hooks are not yet trusted.
pub(crate) fn validate_issues(
    env: &HookEnv,
    root: &Path,
    docs: &[&DocMeta],
    config: &Config,
) -> Vec<(Severity, ValidationIssue)> {
    match gate(env, root, config) {
        Gate::Inactive => Vec::new(),
        Gate::Untrusted(hooks) => {
            vec![(Severity::Warning, ValidationIssue::HooksUntrusted { hooks })]
        }
        Gate::Trusted => run_validate_hooks(env, root, docs, config),
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{exited, ScriptedRunner};
    use super::*;
    use crate::engine::document::{DocType, Status};
    use std::time::Duration;

    fn hook(name: &str, types: &[&str]) -> HookDef {
        HookDef {
            name: name.to_string(),
            event: HookEvent::Validate,
            run: vec![format!(".lazyspec/hooks/{name}")],
            types: types.iter().map(|t| t.to_string()).collect(),
            from: None,
            to: None,
            context_types: Vec::new(),
            timeout: None,
        }
    }

    fn doc(id: &str, doc_type: &str) -> DocMeta {
        DocMeta {
            path: PathBuf::from(format!("docs/{id}.md")),
            title: id.to_string(),
            doc_type: DocType::new(doc_type),
            status: Status::new("draft"),
            author: "t".to_string(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            tags: vec![],
            provenance: vec![],
            governs: vec![],
            reviewed: None,
            related: vec![],
            validate_ignore: false,
            virtual_doc: false,
            assignee: None,
            id: id.to_string(),
            attributes: Default::default(),
            parts: Vec::new(),
            sidecars: Vec::new(),
        }
    }

    struct Fixture {
        tmp: tempfile::TempDir,
        trust: Arc<TrustStore>,
        config: Config,
    }

    impl Fixture {
        fn new(hooks: Vec<HookDef>) -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let config = Config {
                hooks,
                ..Config::default()
            };
            let trust = Arc::new(TrustStore::in_dir(&tmp.path().join("state")));
            Self { tmp, trust, config }
        }

        fn root(&self) -> PathBuf {
            self.tmp.path().join("repo")
        }

        fn trusted(self) -> Self {
            std::fs::create_dir_all(self.root()).unwrap();
            self.trust.trust(&self.root(), &self.config).unwrap();
            self
        }

        fn env(&self, runner: &Arc<ScriptedRunner>) -> HookEnv {
            HookEnv {
                runner: runner.clone(),
                trust: self.trust.clone(),
                fs: Arc::new(RealFileSystem),
                disabled: false,
            }
        }

        fn issues(
            &self,
            runner: &Arc<ScriptedRunner>,
            docs: &[&DocMeta],
        ) -> Vec<(Severity, ValidationIssue)> {
            let env = self.env(runner);
            validate_issues(&env, &self.root(), docs, &self.config)
        }
    }

    fn messages(issues: &[(Severity, ValidationIssue)]) -> Vec<String> {
        issues.iter().map(|(_, i)| i.to_string()).collect()
    }

    #[test]
    fn a_hook_runs_once_with_every_matching_document() {
        let fx = Fixture::new(vec![hook("lint", &["story"])]).trusted();
        let runner = ScriptedRunner::replying(r#"{"findings":[]}"#);
        let (a, b, rfc) = (
            doc("STORY-1", "story"),
            doc("STORY-2", "story"),
            doc("RFC-1", "rfc"),
        );

        fx.issues(&runner, &[&a, &b, &rfc]);

        assert_eq!(runner.calls(), 1);
        let input = runner.last_input();
        assert_eq!(input["event"], "validate");
        assert_eq!(input["hook"], "lint");
        let ids: Vec<&str> = input["documents"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["STORY-1", "STORY-2"]);
        assert!(input["documents"][0]["content_hash"].is_string());
        assert!(input["documents"][0]["parts"].is_array());
        assert!(input["documents"][0]["body"].is_string());
    }

    #[test]
    fn findings_carry_their_fields_and_name_the_hook() {
        let fx = Fixture::new(vec![hook("lint", &[])]).trusted();
        let runner = ScriptedRunner::replying(
            r#"{"findings":[{"id":"STORY-1","part":"design.md","line":14,"severity":"warning","message":"missing ## Goals"}]}"#,
        );
        let a = doc("STORY-1", "story");

        let issues = fx.issues(&runner, &[&a]);

        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].0, Severity::Warning);
        let json = issues[0].1.to_json();
        assert_eq!(json["rule"], "hook");
        assert_eq!(json["hook"], "lint");
        assert_eq!(json["id"], "STORY-1");
        assert_eq!(json["part"], "design.md");
        assert_eq!(json["line"], 14);
        let message = json["message"].as_str().unwrap();
        assert!(message.contains("lint") && message.contains("missing ## Goals"));
    }

    #[test]
    fn a_misbehaving_hook_becomes_one_error_finding_naming_it() {
        let cases: Vec<(Arc<ScriptedRunner>, &str)> = vec![
            (
                ScriptedRunner::with(|_, _| exited(3, "", "boom")),
                "status 3; stderr: boom",
            ),
            (
                ScriptedRunner::with(|_, _| exited(0, "not json", "oops")),
                "invalid JSON",
            ),
            (
                ScriptedRunner::with(|_, _| {
                    Err(TimedOut {
                        timeout: Duration::from_secs(30),
                        stdout: Vec::new(),
                        stderr: b"stuck on lock".to_vec(),
                    }
                    .into())
                }),
                "timed out after 30s; stderr: stuck on lock",
            ),
            (
                ScriptedRunner::replying(r#"{"findings":[],"updates":[{"id":"STORY-1"}]}"#),
                "updates",
            ),
        ];
        for (runner, needle) in cases {
            let fx = Fixture::new(vec![hook("lint", &[])]).trusted();
            let a = doc("STORY-1", "story");
            let issues = fx.issues(&runner, &[&a]);
            assert_eq!(issues.len(), 1, "{needle}");
            assert_eq!(issues[0].0, Severity::Error);
            let message = issues[0].1.to_string();
            assert!(
                message.contains("lint") && message.contains(needle),
                "{message}"
            );
        }
    }

    #[test]
    fn stderr_travels_with_an_invalid_json_finding() {
        let fx = Fixture::new(vec![hook("lint", &[])]).trusted();
        let runner = ScriptedRunner::with(|_, _| exited(0, "nope", "traceback"));
        let a = doc("STORY-1", "story");
        let message = messages(&fx.issues(&runner, &[&a])).remove(0);
        assert!(message.contains("traceback"), "{message}");
    }

    #[test]
    fn untrusted_hooks_are_skipped_with_one_warning_naming_hook_trust() {
        let fx = Fixture::new(vec![hook("a", &[]), hook("b", &[])]);
        let runner = ScriptedRunner::replying(r#"{"findings":[]}"#);
        let a = doc("STORY-1", "story");

        let issues = fx.issues(&runner, &[&a]);

        assert_eq!(runner.calls(), 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].0, Severity::Warning);
        assert!(issues[0].1.to_string().contains("hook trust"));
    }

    #[test]
    fn editing_the_hooks_makes_them_untrusted_again() {
        let mut fx = Fixture::new(vec![hook("lint", &[])]).trusted();
        assert!(fx.trust.is_trusted(&fx.root(), &fx.config));
        fx.config.hooks[0].timeout = Some(5);
        assert!(!fx.trust.is_trusted(&fx.root(), &fx.config));
    }

    #[test]
    fn editing_an_in_repo_run_target_makes_the_hooks_untrusted_again() {
        let fx = Fixture::new(vec![hook("lint", &[])]);
        let script = fx.root().join(".lazyspec/hooks/lint");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        fx.trust.trust(&fx.root(), &fx.config).unwrap();
        assert!(fx.trust.is_trusted(&fx.root(), &fx.config));

        std::fs::write(&script, "#!/bin/sh\nrm -rf /\n").unwrap();

        assert!(!fx.trust.is_trusted(&fx.root(), &fx.config));
    }

    #[test]
    fn the_process_runner_speaks_the_protocol_over_stdin_and_stdout() {
        let tmp = tempfile::tempdir().unwrap();
        let hook = HookDef {
            run: vec![
                "sh".into(),
                "-c".into(),
                r#"cat >/dev/null; echo '{"findings":[{"id":"X-1","severity":"error","message":"seen"}]}'"#.into(),
            ],
            ..hook("sh", &[])
        };
        let process = ProcessRunner
            .run(&hook, tmp.path(), tmp.path(), b"{}")
            .unwrap();
        let findings = interpret(&hook, Ok(process));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].message, "seen");
    }

    #[test]
    fn a_disabled_env_runs_nothing_and_warns_of_nothing() {
        let fx = Fixture::new(vec![hook("lint", &[])]);
        let runner = ScriptedRunner::replying(r#"{"findings":[]}"#);
        let mut env = fx.env(&runner);
        env.disabled = true;
        let a = doc("STORY-1", "story");

        let issues = validate_issues(&env, &fx.root(), &[&a], &fx.config);

        assert!(issues.is_empty());
        assert_eq!(runner.calls(), 0);
    }

    fn extended_fixture() -> (Fixture, PathBuf) {
        let mut fx = Fixture::new(vec![hook("lint", &[])]);
        let pack = fx.tmp.path().join("pack");
        let script = pack.join(".lazyspec/hooks/lint");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        fx.config.extends = Some(crate::engine::config::Extends {
            root: pack,
            ..Default::default()
        });
        (fx, script)
    }

    #[test]
    fn an_extended_packs_hook_script_is_fingerprinted_and_run_from_the_pack() {
        let (fx, script) = extended_fixture();
        std::fs::create_dir_all(fx.root()).unwrap();
        fx.trust.trust(&fx.root(), &fx.config).unwrap();
        assert!(fx.trust.is_trusted(&fx.root(), &fx.config));

        std::fs::write(&script, "#!/bin/sh\nrm -rf /\n").unwrap();
        assert!(
            !fx.trust.is_trusted(&fx.root(), &fx.config),
            "the pack's script is part of the fingerprint"
        );

        fx.trust.trust(&fx.root(), &fx.config).unwrap();
        let runner = ScriptedRunner::replying(r#"{"findings":[]}"#);
        let a = doc("STORY-1", "story");
        fx.issues(&runner, &[&a]);
        assert_eq!(
            runner.last_scripts_root(),
            fx.config.docs_root(&fx.root()),
            "run resolves against the pack, not the project root"
        );
    }

    #[test]
    fn the_process_runner_finds_a_script_in_the_scripts_root_and_not_the_project_root() {
        use std::os::unix::fs::PermissionsExt;
        let (fx, script) = extended_fixture();
        std::fs::write(&script, "#!/bin/sh\ncat >/dev/null\necho '{\"findings\":[{\"severity\":\"warning\",\"message\":\"from the pack\"}]}'\n").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(fx.root()).unwrap();

        let process = ProcessRunner
            .run(
                &fx.config.hooks[0],
                &fx.root(),
                &fx.config.docs_root(&fx.root()),
                b"{}",
            )
            .unwrap();

        let findings = interpret(&fx.config.hooks[0], Ok(process));
        assert_eq!(findings[0].message, "from the pack");
    }
    #[test]
    fn the_process_runner_kills_a_hook_past_its_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let hook = HookDef {
            run: vec!["sh".into(), "-c".into(), "sleep 5".into()],
            timeout: Some(1),
            ..hook("slow", &[])
        };
        let findings = interpret(
            &hook,
            ProcessRunner.run(&hook, tmp.path(), tmp.path(), b"{}"),
        );
        assert_eq!(findings[0].message, "timed out after 1s");
    }
}

/// Fakes for the surfaces that run hooks: a runner that answers from a closure
/// and records what it was called with, and envs with a trust store in the
/// project's temp dir.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    type Answer = Box<dyn Fn(&str, &Value) -> Result<HookProcess> + Send + Sync>;

    pub(crate) struct ScriptedRunner {
        answer: Answer,
        called: Mutex<Vec<(String, Value)>>,
        count: AtomicUsize,
        scripts_roots: Mutex<Vec<PathBuf>>,
    }

    pub(crate) fn exited(code: i32, stdout: &str, stderr: &str) -> Result<HookProcess> {
        Ok(HookProcess {
            code: Some(code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        })
    }

    impl ScriptedRunner {
        pub(crate) fn new(
            answer: impl Fn(&str, &Value) -> String + Send + Sync + 'static,
        ) -> Arc<Self> {
            Self::with(move |hook, input| exited(0, &answer(hook, input), ""))
        }

        pub(crate) fn replying(stdout: &str) -> Arc<Self> {
            let stdout = stdout.to_string();
            Self::new(move |_, _| stdout.clone())
        }

        pub(crate) fn with(
            answer: impl Fn(&str, &Value) -> Result<HookProcess> + Send + Sync + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                answer: Box::new(answer),
                called: Mutex::new(Vec::new()),
                count: AtomicUsize::new(0),
                scripts_roots: Mutex::new(Vec::new()),
            })
        }

        pub(crate) fn calls(&self) -> usize {
            self.count.load(Ordering::SeqCst)
        }

        pub(crate) fn hooks_called(&self) -> Vec<String> {
            self.called
                .lock()
                .unwrap()
                .iter()
                .map(|(hook, _)| hook.clone())
                .collect()
        }

        pub(crate) fn last_input(&self) -> Value {
            self.called.lock().unwrap().last().unwrap().1.clone()
        }

        pub(crate) fn last_scripts_root(&self) -> PathBuf {
            self.scripts_roots.lock().unwrap().last().unwrap().clone()
        }
    }

    impl HookRunner for ScriptedRunner {
        fn run(
            &self,
            hook: &HookDef,
            _: &Path,
            scripts_root: &Path,
            input: &[u8],
        ) -> Result<HookProcess> {
            self.count.fetch_add(1, Ordering::SeqCst);
            let input: Value = serde_json::from_slice(input).unwrap();
            let reply = (self.answer)(&hook.name, &input);
            self.called.lock().unwrap().push((hook.name.clone(), input));
            self.scripts_roots
                .lock()
                .unwrap()
                .push(scripts_root.to_path_buf());
            reply
        }
    }

    pub(crate) fn fixture_hook(name: &str, event: HookEvent) -> HookDef {
        HookDef {
            name: name.to_string(),
            event,
            run: vec![format!(".lazyspec/hooks/{name}")],
            types: Vec::new(),
            from: None,
            to: None,
            context_types: Vec::new(),
            timeout: None,
        }
    }

    pub(crate) fn env_with(runner: Arc<ScriptedRunner>, trust: TrustStore) -> HookEnv {
        HookEnv {
            runner,
            trust: Arc::new(trust),
            fs: Arc::new(RealFileSystem),
            disabled: false,
        }
    }

    pub(crate) fn untrusted_env(runner: Arc<ScriptedRunner>, tmp: &tempfile::TempDir) -> HookEnv {
        env_with(runner, TrustStore::in_dir(&tmp.path().join(".hook-state")))
    }

    pub(crate) fn trusted_env(
        runner: Arc<ScriptedRunner>,
        root: &Path,
        config: &Config,
    ) -> HookEnv {
        let trust = TrustStore::in_dir(&root.join(".hook-state"));
        trust.trust(root, config).unwrap();
        env_with(runner, trust)
    }
}
