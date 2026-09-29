//! User-defined hooks (RFC-075): external commands lazyspec feeds documents as
//! JSON on stdin and reads findings back from on stdout.
//!
//! This module is the `validate` event. The process is behind [`HookRunner`] so
//! a test drives the whole protocol without spawning anything, and every
//! surface (`validate`, `status`, the TUI) reaches hooks through
//! [`validate_issues`] so none of them re-implements the protocol.

use crate::engine::config::{Config, HookDef, HookEvent, Severity};
use crate::engine::doc_json::doc_to_json;
use crate::engine::document::DocMeta;
use crate::engine::fs::{FileSystem, RealFileSystem};
use crate::engine::subprocess::{output_with_timeout_and_input, TimedOut};
use crate::engine::validation::ValidationIssue;
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

static HOOKS_DISABLED: AtomicBool = AtomicBool::new(false);

/// `--no-hooks`: every hook is skipped for the rest of the process, on every
/// surface.
pub fn disable_hooks() {
    HOOKS_DISABLED.store(true, Ordering::Relaxed);
}

pub fn hooks_disabled() -> bool {
    HOOKS_DISABLED.load(Ordering::Relaxed)
}

/// One finding a hook reported, before it is bound to the hook that raised it.
/// Owned and cloneable because the cache keeps it.
#[derive(Debug, Clone, PartialEq)]
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

    fn into_issue(self, hook: &str) -> (Severity, ValidationIssue) {
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

pub trait HookRunner: Send + Sync {
    fn run(&self, hook: &HookDef, root: &Path, input: &[u8]) -> Result<HookProcess>;
}

/// Spawns `hook.run` with the project root as its working directory. No shell.
pub struct ProcessRunner;

impl HookRunner for ProcessRunner {
    fn run(&self, hook: &HookDef, root: &Path, input: &[u8]) -> Result<HookProcess> {
        let program = &hook.run[0];
        let program = if program.contains('/') {
            root.join(program)
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

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A document as a hook reads it: the `show --json` shape with `body`, each
/// part with its `body`, and a `content_hash` over all of them -- the value a
/// `pre-transition` update echoes back to prove the document did not move.
pub(crate) fn hook_document(doc: &DocMeta, root: &Path, fs: &dyn FileSystem) -> Value {
    let mut json = doc_to_json(doc);
    let body = fs
        .read_to_string(&root.join(&doc.path))
        .ok()
        .and_then(|content| DocMeta::extract_body(&content).ok())
        .unwrap_or_default();
    let mut hashed = body.clone();
    if let Some(entries) = json.get_mut("parts").and_then(|p| p.as_array_mut()) {
        for (part, entry) in doc.parts.iter().zip(entries.iter_mut()) {
            let part_body = fs
                .read_to_string(&root.join(&part.path))
                .unwrap_or_default();
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

/// What a hook is called with and the key its result is cached under: the hook
/// plus the hash of exactly these bytes.
struct HookInput {
    bytes: Vec<u8>,
    hash: String,
}

fn validate_input(hook: &HookDef, root: &Path, docs: &[&DocMeta]) -> Option<HookInput> {
    let mut matching: Vec<&DocMeta> = docs
        .iter()
        .copied()
        .filter(|doc| !doc.validate_ignore && hook.applies_to_type(&doc.doc_type.to_string()))
        .collect();
    if matching.is_empty() {
        return None;
    }
    matching.sort_by(|a, b| a.path.cmp(&b.path));
    let fs = RealFileSystem;
    let documents: Vec<Value> = matching
        .into_iter()
        .map(|doc| hook_document(doc, root, &fs))
        .collect();
    let payload = serde_json::json!({
        "event": HookEvent::Validate.as_str(),
        "hook": hook.name,
        "documents": documents,
    });
    let bytes = serde_json::to_vec(&payload).expect("a hook payload serialises as JSON");
    let hash = sha256_hex(&bytes);
    Some(HookInput { bytes, hash })
}

/// Validate-hook results keyed by hook name plus the hash of the input they ran
/// on. Only a full pass writes it; the TUI's quick refresh only reads it.
#[derive(Default)]
pub struct HookCache(Mutex<HashMap<String, (String, Vec<HookFinding>)>>);

impl HookCache {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, Vec<HookFinding>)>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn get(&self, hook: &str, input_hash: &str) -> Option<Vec<HookFinding>> {
        let entries = self.lock();
        let (hash, findings) = entries.get(hook)?;
        (hash == input_hash).then(|| findings.clone())
    }

    /// One entry per hook: a new input hash replaces the old, so the cache is
    /// bounded by the number of hooks.
    fn put(&self, hook: &str, input_hash: String, findings: Vec<HookFinding>) {
        self.lock().insert(hook.to_string(), (input_hash, findings));
    }
}

/// Which hooks are trusted, in user-local state outside the repo (RFC-075
/// Trust): a fingerprint of the `[[hooks]]` table and of every in-repo file a
/// `run` names, filed under the project root.
pub struct TrustStore {
    file: PathBuf,
}

impl TrustStore {
    pub fn user_local() -> Self {
        let dir = match std::env::var_os("LAZYSPEC_STATE_DIR") {
            Some(dir) => PathBuf::from(dir),
            None => {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".lazyspec")
            }
        };
        Self::in_dir(&dir)
    }

    pub fn in_dir(dir: &Path) -> Self {
        Self {
            file: dir.join("hook-trust.json"),
        }
    }

    fn key(root: &Path) -> String {
        root.canonicalize()
            .unwrap_or_else(|_| root.to_path_buf())
            .display()
            .to_string()
    }

    fn load(&self) -> HashMap<String, String> {
        std::fs::read_to_string(&self.file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn is_trusted(&self, root: &Path, hooks: &[HookDef]) -> bool {
        self.load().get(&Self::key(root)) == Some(&fingerprint(root, hooks))
    }

    pub fn trust(&self, root: &Path, hooks: &[HookDef]) -> Result<()> {
        let mut entries = self.load();
        entries.insert(Self::key(root), fingerprint(root, hooks));
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.file, serde_json::to_string_pretty(&entries)?)?;
        Ok(())
    }
}

/// A hash of the `[[hooks]]` table plus the bytes of each file inside the repo
/// that a `run` argv names. Editing a hook or the script it runs changes it.
fn fingerprint(root: &Path, hooks: &[HookDef]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(hooks).expect("hooks serialise as JSON"));
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    for hook in hooks {
        for arg in &hook.run {
            let Ok(target) = root.join(arg).canonicalize() else {
                continue;
            };
            if !target.starts_with(&root) || !target.is_file() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&target) else {
                continue;
            };
            hasher.update(arg.as_bytes());
            hasher.update(Sha256::digest(&bytes));
        }
    }
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// What hooks run against: how a process is spawned, whose trust decides, and
/// where results are cached. A surface that outlives one call (the TUI) owns one
/// and hands clones to its worker, so both see the same cache; a test injects
/// fakes.
#[derive(Clone)]
pub struct HookEnv {
    pub runner: Arc<dyn HookRunner>,
    pub trust: Arc<TrustStore>,
    pub cache: Arc<HookCache>,
}

impl HookEnv {
    /// The real process runner and the user's trust store, with an empty cache.
    pub fn process() -> Self {
        Self {
            runner: Arc::new(ProcessRunner),
            trust: Arc::new(TrustStore::user_local()),
            cache: Arc::new(HookCache::default()),
        }
    }
}

/// [`Pass::Full`] runs every hook and refills the cache. [`Pass::Cached`] only
/// reads it, so it spawns nothing (STORY-295 AC6).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Pass {
    Full,
    Cached,
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
    if hooks_disabled() || names.is_empty() {
        return Gate::Inactive;
    }
    if env.trust.is_trusted(root, &config.hooks) {
        return Gate::Trusted;
    }
    Gate::Untrusted(names)
}

fn run_validate_hooks(
    env: &HookEnv,
    pass: Pass,
    root: &Path,
    docs: &[&DocMeta],
    config: &Config,
) -> Vec<(Severity, ValidationIssue)> {
    let mut issues = Vec::new();
    for hook in validate_hooks(config) {
        let Some(input) = validate_input(hook, root, docs) else {
            continue;
        };
        let findings = match env.cache.get(&hook.name, &input.hash) {
            Some(cached) if pass == Pass::Cached => cached,
            _ if pass == Pass::Cached => continue,
            _ => {
                let run = env.runner.run(hook, root, &input.bytes);
                let findings = interpret(hook, run);
                env.cache.put(&hook.name, input.hash, findings.clone());
                findings
            }
        };
        issues.extend(findings.into_iter().map(|f| f.into_issue(&hook.name)));
    }
    issues
}

/// Every `validate` hook's findings as the validation report carries them, plus
/// one warning naming `hook trust` when the hooks are not yet trusted.
pub fn validate_issues(
    env: &HookEnv,
    pass: Pass,
    root: &Path,
    docs: &[&DocMeta],
    config: &Config,
) -> Vec<(Severity, ValidationIssue)> {
    match gate(env, root, config) {
        Gate::Inactive => Vec::new(),
        Gate::Untrusted(hooks) => {
            vec![(Severity::Warning, ValidationIssue::HooksUntrusted { hooks })]
        }
        Gate::Trusted => run_validate_hooks(env, pass, root, docs, config),
    }
}

/// Run the hooks for their side effect on the cache. The TUI's background
/// worker calls this so the quick refresh has something to read.
pub fn refill_cache(env: &HookEnv, root: &Path, docs: &[&DocMeta], config: &Config) {
    if gate(env, root, config) == Gate::Trusted {
        run_validate_hooks(env, Pass::Full, root, docs, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::document::{DocType, Status};
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    type Reply = Box<dyn Fn(&Value) -> Result<HookProcess> + Send + Sync>;

    struct FakeRunner {
        reply: Reply,
        calls: AtomicUsize,
        last_input: Mutex<Option<Value>>,
    }

    impl FakeRunner {
        fn replying(stdout: &str) -> Arc<Self> {
            let stdout = stdout.to_string();
            Self::with(move |_| {
                Ok(HookProcess {
                    code: Some(0),
                    stdout: stdout.clone(),
                    stderr: String::new(),
                })
            })
        }

        fn with(f: impl Fn(&Value) -> Result<HookProcess> + Send + Sync + 'static) -> Arc<Self> {
            Arc::new(Self {
                reply: Box::new(f),
                calls: AtomicUsize::new(0),
                last_input: Mutex::new(None),
            })
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    impl HookRunner for FakeRunner {
        fn run(&self, _: &HookDef, _: &Path, input: &[u8]) -> Result<HookProcess> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let input: Value = serde_json::from_slice(input).unwrap();
            let reply = (self.reply)(&input);
            *self.last_input.lock().unwrap() = Some(input);
            reply
        }
    }

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
        cache: Arc<HookCache>,
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
            Self {
                tmp,
                trust,
                cache: Arc::new(HookCache::default()),
                config,
            }
        }

        fn root(&self) -> PathBuf {
            self.tmp.path().join("repo")
        }

        fn trusted(self) -> Self {
            std::fs::create_dir_all(self.root()).unwrap();
            self.trust.trust(&self.root(), &self.config.hooks).unwrap();
            self
        }

        fn env(&self, runner: &Arc<FakeRunner>) -> HookEnv {
            HookEnv {
                runner: runner.clone(),
                trust: self.trust.clone(),
                cache: self.cache.clone(),
            }
        }

        fn issues(
            &self,
            runner: &Arc<FakeRunner>,
            pass: Pass,
            docs: &[&DocMeta],
        ) -> Vec<(Severity, ValidationIssue)> {
            let env = self.env(runner);
            validate_issues(&env, pass, &self.root(), docs, &self.config)
        }
    }

    fn messages(issues: &[(Severity, ValidationIssue)]) -> Vec<String> {
        issues.iter().map(|(_, i)| i.to_string()).collect()
    }

    #[test]
    fn a_hook_runs_once_with_every_matching_document() {
        let fx = Fixture::new(vec![hook("lint", &["story"])]).trusted();
        let runner = FakeRunner::replying(r#"{"findings":[]}"#);
        let (a, b, rfc) = (
            doc("STORY-1", "story"),
            doc("STORY-2", "story"),
            doc("RFC-1", "rfc"),
        );

        fx.issues(&runner, Pass::Full, &[&a, &b, &rfc]);

        assert_eq!(runner.calls(), 1);
        let input = runner.last_input.lock().unwrap().clone().unwrap();
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
        let runner = FakeRunner::replying(
            r#"{"findings":[{"id":"STORY-1","part":"design.md","line":14,"severity":"warning","message":"missing ## Goals"}]}"#,
        );
        let a = doc("STORY-1", "story");

        let issues = fx.issues(&runner, Pass::Full, &[&a]);

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
        let cases: Vec<(Arc<FakeRunner>, &str)> = vec![
            (
                FakeRunner::with(|_| {
                    Ok(HookProcess {
                        code: Some(3),
                        stdout: String::new(),
                        stderr: "boom".into(),
                    })
                }),
                "status 3; stderr: boom",
            ),
            (
                FakeRunner::with(|_| {
                    Ok(HookProcess {
                        code: Some(0),
                        stdout: "not json".into(),
                        stderr: "oops".into(),
                    })
                }),
                "invalid JSON",
            ),
            (
                FakeRunner::with(|_| {
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
                FakeRunner::replying(r#"{"findings":[],"updates":[{"id":"STORY-1"}]}"#),
                "updates",
            ),
        ];
        for (runner, needle) in cases {
            let fx = Fixture::new(vec![hook("lint", &[])]).trusted();
            let a = doc("STORY-1", "story");
            let issues = fx.issues(&runner, Pass::Full, &[&a]);
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
        let runner = FakeRunner::with(|_| {
            Ok(HookProcess {
                code: Some(0),
                stdout: "nope".into(),
                stderr: "traceback".into(),
            })
        });
        let a = doc("STORY-1", "story");
        let message = messages(&fx.issues(&runner, Pass::Full, &[&a])).remove(0);
        assert!(message.contains("traceback"), "{message}");
    }

    #[test]
    fn untrusted_hooks_are_skipped_with_one_warning_naming_hook_trust() {
        let fx = Fixture::new(vec![hook("a", &[]), hook("b", &[])]);
        let runner = FakeRunner::replying(r#"{"findings":[]}"#);
        let a = doc("STORY-1", "story");

        let issues = fx.issues(&runner, Pass::Full, &[&a]);

        assert_eq!(runner.calls(), 0);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].0, Severity::Warning);
        assert!(issues[0].1.to_string().contains("hook trust"));
    }

    #[test]
    fn editing_the_hooks_makes_them_untrusted_again() {
        let mut fx = Fixture::new(vec![hook("lint", &[])]).trusted();
        assert!(fx.trust.is_trusted(&fx.root(), &fx.config.hooks));
        fx.config.hooks[0].timeout = Some(5);
        assert!(!fx.trust.is_trusted(&fx.root(), &fx.config.hooks));
    }

    #[test]
    fn editing_an_in_repo_run_target_makes_the_hooks_untrusted_again() {
        let fx = Fixture::new(vec![hook("lint", &[])]);
        let script = fx.root().join(".lazyspec/hooks/lint");
        std::fs::create_dir_all(script.parent().unwrap()).unwrap();
        std::fs::write(&script, "#!/bin/sh\n").unwrap();
        fx.trust.trust(&fx.root(), &fx.config.hooks).unwrap();
        assert!(fx.trust.is_trusted(&fx.root(), &fx.config.hooks));

        std::fs::write(&script, "#!/bin/sh\nrm -rf /\n").unwrap();

        assert!(!fx.trust.is_trusted(&fx.root(), &fx.config.hooks));
    }

    #[test]
    fn a_cached_pass_spawns_nothing_and_a_full_pass_fills_it() {
        let fx = Fixture::new(vec![hook("lint", &[])]).trusted();
        let runner = FakeRunner::replying(
            r#"{"findings":[{"id":"STORY-1","severity":"error","message":"bad"}]}"#,
        );
        let a = doc("STORY-1", "story");

        assert!(fx.issues(&runner, Pass::Cached, &[&a]).is_empty());
        assert_eq!(runner.calls(), 0);

        assert_eq!(fx.issues(&runner, Pass::Full, &[&a]).len(), 1);
        assert_eq!(runner.calls(), 1);

        assert_eq!(fx.issues(&runner, Pass::Cached, &[&a]).len(), 1);
        assert_eq!(runner.calls(), 1);
    }

    #[test]
    fn a_changed_input_misses_the_cache() {
        let fx = Fixture::new(vec![hook("lint", &[])]).trusted();
        let runner = FakeRunner::replying(
            r#"{"findings":[{"id":"STORY-1","severity":"error","message":"bad"}]}"#,
        );
        let a = doc("STORY-1", "story");
        let b = doc("STORY-2", "story");
        fx.issues(&runner, Pass::Full, &[&a]);

        assert!(fx.issues(&runner, Pass::Cached, &[&a, &b]).is_empty());
    }

    #[test]
    fn refill_cache_runs_hooks_without_returning_findings() {
        let fx = Fixture::new(vec![hook("lint", &[])]).trusted();
        let runner = FakeRunner::replying(r#"{"findings":[]}"#);
        let a = doc("STORY-1", "story");
        let env = fx.env(&runner);

        refill_cache(&env, &fx.root(), &[&a], &fx.config);

        assert_eq!(runner.calls(), 1);
        assert!(fx
            .cache
            .get(
                "lint",
                &validate_input(&fx.config.hooks[0], &fx.root(), &[&a])
                    .unwrap()
                    .hash
            )
            .is_some());
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
        let process = ProcessRunner.run(&hook, tmp.path(), b"{}").unwrap();
        let findings = interpret(&hook, Ok(process));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].message, "seen");
    }

    #[test]
    fn the_process_runner_kills_a_hook_past_its_timeout() {
        let tmp = tempfile::tempdir().unwrap();
        let hook = HookDef {
            run: vec!["sh".into(), "-c".into(), "sleep 5".into()],
            timeout: Some(1),
            ..hook("slow", &[])
        };
        let findings = interpret(&hook, ProcessRunner.run(&hook, tmp.path(), b"{}"));
        assert_eq!(findings[0].message, "timed out after 1s");
    }
}

/// Fakes for the surfaces that run hooks: a runner that answers from a closure
/// and records what it was called with, and envs with a trust store in the
/// project's temp dir.
#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    type Answer = Box<dyn Fn(&str, &Value) -> String + Send + Sync>;

    pub(crate) struct ScriptedRunner {
        answer: Answer,
        called: Mutex<Vec<(String, Value)>>,
        pub(crate) count: AtomicUsize,
    }

    impl ScriptedRunner {
        pub(crate) fn new(
            answer: impl Fn(&str, &Value) -> String + Send + Sync + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                answer: Box::new(answer),
                called: Mutex::new(Vec::new()),
                count: AtomicUsize::new(0),
            })
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
    }

    impl HookRunner for ScriptedRunner {
        fn run(&self, hook: &HookDef, _: &Path, input: &[u8]) -> Result<HookProcess> {
            self.count.fetch_add(1, Ordering::SeqCst);
            let input: Value = serde_json::from_slice(input).unwrap();
            let stdout = (self.answer)(&hook.name, &input);
            self.called.lock().unwrap().push((hook.name.clone(), input));
            Ok(HookProcess {
                code: Some(0),
                stdout,
                stderr: String::new(),
            })
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

    pub(crate) fn untrusted_env(runner: Arc<ScriptedRunner>, tmp: &tempfile::TempDir) -> HookEnv {
        HookEnv {
            runner,
            trust: Arc::new(TrustStore::in_dir(&tmp.path().join(".hook-state"))),
            cache: Arc::new(HookCache::default()),
        }
    }

    pub(crate) fn trusted_env(
        runner: Arc<ScriptedRunner>,
        root: &Path,
        config: &Config,
    ) -> HookEnv {
        let trust = TrustStore::in_dir(&root.join(".hook-state"));
        trust.trust(root, &config.hooks).unwrap();
        HookEnv {
            runner,
            trust: Arc::new(trust),
            cache: Arc::new(HookCache::default()),
        }
    }
}
