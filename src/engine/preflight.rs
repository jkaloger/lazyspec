use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use notify::{recommended_watcher, EventHandler, RecommendedWatcher, RecursiveMode, Watcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::config::Config;
use super::prompt::{render_prompt, DocSummary};

/// Default workflow/prompt path for the v1 single-role builder.
/// Multi-role (`.lazyspec/workflows/<role>.md`) is a future evolution.
pub const DEFAULT_PROMPT_PATH: &str = ".lazyspec/prompts/builder.md";

/// Default config file path, relative to the project root.
pub const DEFAULT_CONFIG_PATH: &str = ".lazyspec.toml";

pub struct PreflightChecks<'a> {
    pub root: &'a Path,
    pub config: &'a Config,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightReport {
    pub workflow_readable: bool,
    pub prompt_renders: bool,
    pub agent_users_non_empty: bool,
}

impl PreflightReport {
    pub fn is_ok(&self) -> bool {
        self.workflow_readable && self.prompt_renders && self.agent_users_non_empty
    }

    /// All-pass report. Convenience for tests + the production default until
    /// the real `run_preflight` runs at daemon start.
    pub fn all_ok() -> Self {
        Self {
            workflow_readable: true,
            prompt_renders: true,
            agent_users_non_empty: true,
        }
    }
}

/// Receive-side trait for the notify-driven preflight invalidation channel.
///
/// `poll` returns `true` if any filesystem event has been observed against the
/// watched config or prompt files since the last poll. Production implementation
/// wraps `notify::RecommendedWatcher`; tests use a recording fake.
pub trait PreflightWatcher: Send {
    fn poll(&self) -> bool;
}

/// Filesystem-event-driven preflight watcher.
///
/// Watches the *parent directories* of `config_path` and `prompt_path`
/// (non-recursively) — atomic-write patterns (rename + replace) by most editors
/// fire events on the parent, not the file. Events are filtered inside the
/// notify handler to only enqueue those whose path list contains the watched
/// config or prompt path.
pub struct NotifyPreflightWatcher {
    rx: Receiver<()>,
    // Held to keep the OS-level watch alive for the lifetime of this struct;
    // dropping the watcher unregisters the kqueue/inotify hook.
    _watcher: RecommendedWatcher,
}

impl NotifyPreflightWatcher {
    pub fn start(config_path: PathBuf, prompt_path: PathBuf) -> Result<Self> {
        let (tx, rx) = unbounded::<()>();
        let handler = PreflightEventFilter {
            tx: Mutex::new(tx),
            config_path: config_path.clone(),
            prompt_path: prompt_path.clone(),
        };
        let mut watcher: RecommendedWatcher = recommended_watcher(handler)?;

        let config_parent = config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let prompt_parent = prompt_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        // Parent dirs may not exist on a fresh workspace; create them so the
        // watcher attaches. Failure to create is non-fatal — the watcher just
        // never fires for that path until it appears.
        let _ = fs::create_dir_all(&config_parent);
        let _ = fs::create_dir_all(&prompt_parent);

        watcher.watch(&config_parent, RecursiveMode::NonRecursive)?;
        if prompt_parent != config_parent {
            watcher.watch(&prompt_parent, RecursiveMode::NonRecursive)?;
        }

        Ok(Self {
            rx,
            _watcher: watcher,
        })
    }
}

impl PreflightWatcher for NotifyPreflightWatcher {
    fn poll(&self) -> bool {
        let mut any = false;
        while self.rx.try_recv().is_ok() {
            any = true;
        }
        any
    }
}

struct PreflightEventFilter {
    tx: Mutex<Sender<()>>,
    config_path: PathBuf,
    prompt_path: PathBuf,
}

impl EventHandler for PreflightEventFilter {
    fn handle_event(&mut self, event: notify::Result<notify::Event>) {
        let Ok(event) = event else { return };
        let matches = event
            .paths
            .iter()
            .any(|p| p == &self.config_path || p == &self.prompt_path);
        if matches {
            let _ = self.tx.lock().unwrap().send(());
        }
    }
}

/// Run preflight checks for the orchestration daemon.
///
/// If `config.orchestration` is `None` (legacy config without an orchestration
/// block), both `workflow_readable` and `agent_users_non_empty` are reported as
/// `false` — there is no workflow path to read and no agent_users to validate.
pub fn run_preflight(checks: &PreflightChecks) -> Result<PreflightReport> {
    let Some(orch) = checks.config.orchestration.as_ref() else {
        return Ok(PreflightReport {
            workflow_readable: false,
            prompt_renders: false,
            agent_users_non_empty: false,
        });
    };

    let prompt_path: PathBuf = checks.root.join(DEFAULT_PROMPT_PATH);

    let (workflow_readable, prompt_renders) = match fs::read_to_string(&prompt_path) {
        Ok(template) => (true, prompt_renders_ok(&template)),
        Err(_) => (false, false),
    };

    Ok(PreflightReport {
        workflow_readable,
        prompt_renders,
        agent_users_non_empty: !orch.agent_users.is_empty(),
    })
}

fn prompt_renders_ok(template: &str) -> bool {
    let stub_doc = DocSummary {
        id: "DUMMY".to_string(),
        title: "x".to_string(),
        body: String::new(),
        status: "draft".to_string(),
        assignees: vec![],
    };
    render_prompt(template, &stub_doc, None, &[]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, OrchestrationConfig};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn orch(agent_users: Vec<&str>) -> OrchestrationConfig {
        OrchestrationConfig {
            agent_users: agent_users.into_iter().map(String::from).collect(),
            claim_type: "story".to_string(),
            branch_template: "agents/{{ story_id }}".to_string(),
            workspace_root: PathBuf::from(".lazyspec/workspaces"),
            base_branch: "main".to_string(),
            runtime: Default::default(),
            hooks: Default::default(),
            poll_interval_ms: 1000,
            max_concurrent_agents: 1,
            active_statuses: vec!["ready".to_string()],
            heartbeat_interval_ms: 1000,
            metadata_push_interval_ms: 1000,
            stall_timeout_ms: 60000,
            max_turns: 50,
            max_failure_attempts: 3,
            max_retry_backoff_ms: 60000,
            handoff_states: vec![],
            continuation_delay_ms: 0,
        }
    }

    fn cfg(orchestration: Option<OrchestrationConfig>) -> Config {
        Config {
            orchestration,
            ..Config::default()
        }
    }

    fn write_prompt(root: &Path, contents: &str) {
        let dir = root.join(".lazyspec/prompts");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("builder.md"), contents).unwrap();
    }

    #[test]
    fn is_ok_only_when_all_pass() {
        let pass = PreflightReport {
            workflow_readable: true,
            prompt_renders: true,
            agent_users_non_empty: true,
        };
        assert!(pass.is_ok());

        for combo in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
            (false, false, false),
        ] {
            let r = PreflightReport {
                workflow_readable: combo.0,
                prompt_renders: combo.1,
                agent_users_non_empty: combo.2,
            };
            assert!(!r.is_ok(), "expected !is_ok for {combo:?}");
        }
    }

    #[test]
    fn agent_users_non_empty_returns_false_on_empty_vec() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "hello");
        let config = cfg(Some(orch(vec![])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(!report.agent_users_non_empty);
    }

    #[test]
    fn agent_users_non_empty_returns_true_when_populated() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "hello");
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(report.agent_users_non_empty);
    }

    #[test]
    fn workflow_readable_returns_false_when_missing() {
        let tmp = TempDir::new().unwrap();
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(!report.workflow_readable);
        assert!(!report.prompt_renders);
    }

    #[test]
    fn workflow_readable_returns_true_when_present() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "hello");
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(report.workflow_readable);
    }

    #[test]
    fn prompt_renders_returns_false_on_template_error() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "{{ undefined_thing }}");
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(report.workflow_readable);
        assert!(!report.prompt_renders);
    }

    #[test]
    fn prompt_renders_returns_false_on_syntax_error() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "{{ unterminated");
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(!report.prompt_renders);
    }

    #[test]
    fn prompt_renders_returns_true_on_clean_template() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "Doc id: {{ doc.id }}");
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(report.prompt_renders);
    }

    #[test]
    fn prompt_renders_true_on_literal_only_template() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "literal text, no variables");
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(report.prompt_renders);
    }

    #[test]
    fn no_orchestration_block_fails_workflow_and_agent_users() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "hello");
        let config = cfg(None);
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(!report.workflow_readable);
        assert!(!report.agent_users_non_empty);
        assert!(!report.prompt_renders);
        assert!(!report.is_ok());
    }

    #[test]
    fn happy_path_all_pass_is_ok() {
        let tmp = TempDir::new().unwrap();
        write_prompt(tmp.path(), "Doc {{ doc.id }} status={{ doc.status }}");
        let config = cfg(Some(orch(vec!["claude-bot"])));
        let report = run_preflight(&PreflightChecks {
            root: tmp.path(),
            config: &config,
        })
        .unwrap();
        assert!(report.is_ok(), "expected is_ok, got {report:?}");
    }
}
