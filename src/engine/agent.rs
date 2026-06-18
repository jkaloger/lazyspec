use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

#[cfg(feature = "agent")]
use std::path::PathBuf;
#[cfg(feature = "agent")]
use std::process::{Child, Stdio};

pub fn resolve_agent_id(root: &Path) -> Result<String> {
    resolve_agent_id_with_env(
        root,
        std::env::var("LAZYSPEC_AGENT_ID").ok(),
        std::env::var("CLAUDE_SESSION_ID").ok(),
    )
}

pub fn resolve_agent_id_with_env(
    root: &Path,
    lazyspec_agent_id: Option<String>,
    claude_session_id: Option<String>,
) -> Result<String> {
    if let Some(id) = lazyspec_agent_id.filter(|s| !s.is_empty()) {
        return Ok(id);
    }

    if let Some(id) = claude_session_id.filter(|s| !s.is_empty()) {
        return Ok(id);
    }

    let output = Command::new("git")
        .args(["config", "user.name"])
        .current_dir(root)
        .output()?;

    if !output.status.success() {
        bail!("git config user.name failed; set $LAZYSPEC_AGENT_ID or configure git user.name");
    }

    let user_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if user_name.is_empty() {
        bail!("git config user.name is empty; set $LAZYSPEC_AGENT_ID or configure git user.name");
    }

    Ok(user_name)
}

/// The result of resolving a document type's opt-in `agents` list against the
/// templates that actually loaded.
#[cfg(feature = "agent")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAgents {
    /// Template stems from the type's `agents` list that DID load, in the type's declared order.
    pub actions: Vec<String>,
    /// Stems named in the type's `agents` list with no matching loaded template
    /// (user named an action they did not author) -- the named-but-missing report
    /// the dialog surfaces.
    pub missing: Vec<String>,
}

#[cfg(feature = "agent")]
/// Resolve a type's opt-in `agents` list against the templates that actually loaded.
/// Intersection in declared order; declared-but-not-loaded names go to `missing`.
/// Empty `type_agents` → empty actions + empty missing (off; not an error).
pub fn resolve_agent_actions(type_agents: &[String], loaded: &[String]) -> ResolvedAgents {
    let mut actions = Vec::new();
    let mut missing = Vec::new();
    for name in type_agents {
        if loaded.iter().any(|l| l == name) {
            actions.push(name.clone());
        } else {
            missing.push(name.clone());
        }
    }
    ResolvedAgents { actions, missing }
}

/// Inputs a headless agent run needs: the rendered prompt, optional tool
/// allow-list, the document the run is scoped to, and the session id.
#[cfg(feature = "agent")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContext {
    pub prompt: String,
    pub allowed_tools: Option<String>,
    pub doc_path: PathBuf,
    pub session_id: String,
}

/// A spawned headless agent: the session id plus the live child process.
#[cfg(feature = "agent")]
#[derive(Debug)]
pub struct AgentHandle {
    pub session_id: String,
    pub child: Child,
}

/// Seam for launching a headless background agent. Production uses [`ClaudeP`];
/// tests inject a fake to assert on the [`AgentContext`] without spawning.
#[cfg(feature = "agent")]
pub trait AgentRunner {
    fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle>;
}

/// The v1 runner: invokes `claude -p` as a detached background process.
#[cfg(feature = "agent")]
pub struct ClaudeP;

#[cfg(feature = "agent")]
impl ClaudeP {
    fn build_command(ctx: &AgentContext) -> Command {
        let mut cmd = Command::new("claude");
        cmd.args(["-p", &ctx.prompt]);
        cmd.args(["--session-id", &ctx.session_id]);
        if let Some(tools) = &ctx.allowed_tools {
            cmd.args(["--allowedTools", tools]);
        }
        cmd
    }
}

#[cfg(feature = "agent")]
impl AgentRunner for ClaudeP {
    fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle> {
        let mut cmd = ClaudeP::build_command(&ctx);
        let child = cmd
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

/// A fake [`AgentRunner`] for tests: records every [`AgentContext`] it receives
/// and never launches `claude`. On `fail`, [`AgentRunner::spawn`] errors;
/// otherwise it returns a handle wrapping a trivial, immediately-exiting child.
#[cfg(all(feature = "agent", test))]
pub(crate) struct FakeRunner {
    pub captured: std::cell::RefCell<Vec<AgentContext>>,
    pub fail: bool,
}

#[cfg(all(feature = "agent", test))]
impl FakeRunner {
    pub(crate) fn new() -> Self {
        FakeRunner {
            captured: std::cell::RefCell::new(Vec::new()),
            fail: false,
        }
    }
}

#[cfg(all(feature = "agent", test))]
impl AgentRunner for FakeRunner {
    fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle> {
        self.captured.borrow_mut().push(ctx.clone());
        if self.fail {
            bail!("FakeRunner: forced failure");
        }
        // A deterministic, immediately-exiting child -- never `claude`.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn uses_lazyspec_agent_id_when_set() {
        let result =
            resolve_agent_id_with_env(Path::new("/tmp/fake"), Some("my-agent".into()), None)
                .unwrap();
        assert_eq!(result, "my-agent");
    }

    #[test]
    fn uses_claude_session_id_when_lazyspec_unset() {
        let result =
            resolve_agent_id_with_env(Path::new("/tmp/fake"), None, Some("sess-123".into()))
                .unwrap();
        assert_eq!(result, "sess-123");
    }

    #[test]
    fn lazyspec_agent_id_takes_priority_over_claude_session_id() {
        let result = resolve_agent_id_with_env(
            Path::new("/tmp/fake"),
            Some("agent-1".into()),
            Some("sess-123".into()),
        )
        .unwrap();
        assert_eq!(result, "agent-1");
    }

    #[test]
    fn empty_strings_treated_as_unset() {
        let result =
            resolve_agent_id_with_env(Path::new("/tmp/fake"), Some("".into()), Some("sess".into()))
                .unwrap();
        assert_eq!(result, "sess");
    }

    #[test]
    fn falls_back_to_git_config_username() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "TestUser"])
            .current_dir(root)
            .output()
            .unwrap();

        let result = resolve_agent_id_with_env(root, None, None).unwrap();
        assert_eq!(result, "TestUser");
    }

    #[test]
    fn both_empty_strings_falls_back_to_git() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "GitUser"])
            .current_dir(root)
            .output()
            .unwrap();

        let result = resolve_agent_id_with_env(root, Some("".into()), Some("".into())).unwrap();
        assert_eq!(result, "GitUser");
    }

    // AC3: ClaudeP builds `claude -p <prompt> --session-id <id>`; no --allowedTools when None.
    #[cfg(feature = "agent")]
    #[test]
    fn claudep_builds_claude_p_command() {
        let ctx = AgentContext {
            prompt: "do the thing".into(),
            allowed_tools: None,
            doc_path: PathBuf::from("docs/iterations/ITERATION-1.md"),
            session_id: "sess-abc".into(),
        };

        let cmd = ClaudeP::build_command(&ctx);

        assert_eq!(cmd.get_program(), "claude");
        let argv: Vec<&str> = cmd.get_args().map(|a| a.to_str().unwrap()).collect();
        assert_eq!(argv, ["-p", "do the thing", "--session-id", "sess-abc"]);
        assert!(!argv.contains(&"--allowedTools"));
    }

    // AC4: --allowedTools appears (with its value) only when allowed_tools is Some.
    #[cfg(feature = "agent")]
    #[test]
    fn claudep_includes_allowed_tools_only_when_some() {
        let with_tools = AgentContext {
            prompt: "p".into(),
            allowed_tools: Some("Read,Edit".into()),
            doc_path: PathBuf::from("doc.md"),
            session_id: "s".into(),
        };
        let argv: Vec<String> = ClaudeP::build_command(&with_tools)
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();
        let flag_pos = argv
            .iter()
            .position(|a| a == "--allowedTools")
            .expect("--allowedTools should be present when Some");
        assert_eq!(argv[flag_pos + 1], "Read,Edit");

        let without_tools = AgentContext {
            allowed_tools: None,
            ..with_tools
        };
        let argv_none: Vec<String> = ClaudeP::build_command(&without_tools)
            .get_args()
            .map(|a| a.to_str().unwrap().to_string())
            .collect();
        assert!(!argv_none.iter().any(|a| a == "--allowedTools"));
    }

    // AC3: the action set is the intersection of the type's declared `agents`
    // with the loaded templates, in the type's declared order -- an extra
    // unrelated loaded template proves it is an intersection, not a union.
    #[cfg(feature = "agent")]
    #[test]
    fn resolve_intersects_type_agents_with_loaded() {
        let type_agents = vec!["expand".to_string(), "create-children".to_string()];
        let loaded = vec![
            "expand".to_string(),
            "create-children".to_string(),
            "summarize".to_string(),
        ];
        let resolved = resolve_agent_actions(&type_agents, &loaded);
        assert_eq!(
            resolved.actions,
            vec!["expand".to_string(), "create-children".to_string()]
        );
        assert!(resolved.missing.is_empty());
    }

    // AC4: a declared name with no matching loaded template is reported in
    // `missing` and excluded from `actions`.
    #[cfg(feature = "agent")]
    #[test]
    fn resolve_reports_named_but_missing() {
        let type_agents = vec!["expand".to_string(), "nonexistent".to_string()];
        let loaded = vec!["expand".to_string()];
        let resolved = resolve_agent_actions(&type_agents, &loaded);
        assert_eq!(resolved.actions, vec!["expand".to_string()]);
        assert_eq!(resolved.missing, vec!["nonexistent".to_string()]);
    }

    // AC5: a loaded template referenced by no type appears in neither `actions`
    // nor `missing`.
    #[cfg(feature = "agent")]
    #[test]
    fn resolve_ignores_unreferenced_loaded_template() {
        let type_agents = vec!["expand".to_string()];
        let loaded = vec!["expand".to_string(), "orphan".to_string()];
        let resolved = resolve_agent_actions(&type_agents, &loaded);
        assert_eq!(resolved.actions, vec!["expand".to_string()]);
        assert!(resolved.missing.is_empty());
        assert!(!resolved.actions.contains(&"orphan".to_string()));
        assert!(!resolved.missing.contains(&"orphan".to_string()));
    }

    // AC6: resolution is per-type and independent -- one type with a list, another
    // with none, resolve without cross-coupling.
    #[cfg(feature = "agent")]
    #[test]
    fn resolve_is_per_type_independent() {
        let loaded = vec!["expand".to_string()];

        let type_a = vec!["expand".to_string()];
        let resolved_a = resolve_agent_actions(&type_a, &loaded);
        assert_eq!(resolved_a.actions, vec!["expand".to_string()]);
        assert!(resolved_a.missing.is_empty());

        let type_b: Vec<String> = Vec::new();
        let resolved_b = resolve_agent_actions(&type_b, &loaded);
        assert!(resolved_b.actions.is_empty());
        assert!(resolved_b.missing.is_empty());
    }

    // AC2: a fake runner records the exact AgentContext and spawns no `claude`.
    #[cfg(feature = "agent")]
    #[test]
    fn fake_runner_captures_context_without_subprocess() {
        let ctx = AgentContext {
            prompt: "refine this".into(),
            allowed_tools: Some("Read".into()),
            doc_path: PathBuf::from("docs/stories/STORY-132.md"),
            session_id: "session-xyz".into(),
        };

        let runner = FakeRunner::new();
        let handle = runner
            .spawn(ctx.clone())
            .expect("fake spawn should succeed");

        assert_eq!(handle.session_id, "session-xyz");
        let captured = runner.captured.borrow();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0], ctx);
    }
}
