use anyhow::{anyhow, Context, Result};
use minijinja::{context, Environment, UndefinedBehavior};
use std::process::Command;

pub struct BranchVars {
    pub iteration_id: String,
    pub iteration_slug: String,
    pub agent_id: String,
    pub story_id: String,
    pub date: String,
}

pub fn render_branch_name(template: &str, vars: &BranchVars) -> Result<String> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.add_template("branch", template)
        .context("failed to parse branch template")?;
    let tmpl = env
        .get_template("branch")
        .context("failed to load branch template")?;
    tmpl.render(context! {
        iteration_id => vars.iteration_id,
        iteration_slug => vars.iteration_slug,
        agent_id => vars.agent_id,
        story_id => vars.story_id,
        date => vars.date,
    })
    .context("failed to render branch template")
}

pub fn sanitize_branch_name(name: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["check-ref-format", "--branch", name])
        .output()
        .context("failed to spawn git check-ref-format")?;
    if output.status.success() {
        Ok(name.to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(anyhow!(
            "git rejected branch name {name:?}: {}",
            stderr.trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> BranchVars {
        BranchVars {
            iteration_id: "ITERATION-175".to_string(),
            iteration_slug: "workspace".to_string(),
            agent_id: "claude-bot".to_string(),
            story_id: "STORY-127".to_string(),
            date: "2026-05-13".to_string(),
        }
    }

    #[test]
    fn renders_happy_path() {
        let out =
            render_branch_name("agents/{{ story_id }}/{{ iteration_slug }}", &fixture()).unwrap();
        assert_eq!(out, "agents/STORY-127/workspace");
    }

    #[test]
    fn strict_undefined_errors_on_missing_var() {
        let err = render_branch_name("{{ missing }}", &fixture());
        assert!(err.is_err(), "expected error for undefined var");
    }

    #[test]
    fn sandbox_blocks_introspection() {
        let err = render_branch_name("{{ story_id.__class__ }}", &fixture());
        assert!(err.is_err(), "expected error for dunder introspection");
    }

    #[test]
    fn sanitize_passes_valid_branch_name() {
        let out = sanitize_branch_name("agents/STORY-127").unwrap();
        assert_eq!(out, "agents/STORY-127");
    }

    #[test]
    fn sanitize_rejects_space_in_name() {
        let err = sanitize_branch_name("agents/with space");
        assert!(err.is_err(), "expected error for space in branch name");
    }

    #[test]
    fn sanitize_rejects_dotdot() {
        let err = sanitize_branch_name("agents/..");
        assert!(err.is_err(), "expected error for .. in branch name");
    }

    #[test]
    fn sanitize_rejects_trailing_slash() {
        let err = sanitize_branch_name("agents/foo/");
        assert!(err.is_err(), "expected error for trailing slash");
    }
}
