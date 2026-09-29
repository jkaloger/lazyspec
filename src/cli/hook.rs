use crate::engine::config::HookEvent;
use crate::engine::config::{Config, HookDef};
use crate::engine::git_ref::GitRefOps;
use crate::engine::hooks::{HookEnv, TrustStore};
use crate::engine::pre_transition::{ReportedFinding, TransitionBlocked};
use crate::engine::store::Store;
use anyhow::{bail, Result};
use clap::Subcommand;
use serde_json::{json, Value};
use std::path::Path;

#[derive(Subcommand)]
pub enum HookCommand {
    /// List the configured hooks with their scope and trust state
    List {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Trust the current `[[hooks]]` table and the in-repo files its `run` entries name
    Trust {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Fire an event's hooks for one document without changing its status
    Run {
        /// The event to fire (`pre-transition`)
        event: String,
        /// Document to run the hooks against
        id: String,
        /// Report what the hooks would update without saving it
        #[arg(long)]
        dry_run: bool,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

fn scope(hook: &HookDef) -> Value {
    json!({
        "types": hook.types,
        "from": hook.from,
        "to": hook.to,
    })
}

fn scope_text(hook: &HookDef) -> String {
    let mut parts = vec![if hook.types.is_empty() {
        "all types".to_string()
    } else {
        hook.types.join(", ")
    }];
    if let Some(from) = &hook.from {
        parts.push(format!("from {from}"));
    }
    if let Some(to) = &hook.to {
        parts.push(format!("to {to}"));
    }
    parts.join("; ")
}

fn trust_label(trusted: bool) -> &'static str {
    if trusted {
        "trusted"
    } else {
        "untrusted"
    }
}

pub fn run_list(root: &Path, config: &Config, trust: &TrustStore, json: bool) -> String {
    let trusted = trust.is_trusted(root, &config.hooks);
    if json {
        let hooks: Vec<Value> = config
            .hooks
            .iter()
            .map(|hook| {
                json!({
                    "name": hook.name,
                    "event": hook.event.as_str(),
                    "scope": scope(hook),
                    "trust": trust_label(trusted),
                })
            })
            .collect();
        return serde_json::to_string_pretty(&hooks).expect("hooks serialise as JSON");
    }
    if config.hooks.is_empty() {
        return "No hooks configured.".to_string();
    }
    config
        .hooks
        .iter()
        .map(|hook| {
            format!(
                "{}\t{}\t{}\t{}",
                hook.name,
                hook.event,
                scope_text(hook),
                trust_label(trusted)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn run_trust(root: &Path, config: &Config, trust: &TrustStore, json: bool) -> Result<String> {
    if config.hooks.is_empty() {
        bail!("no [[hooks]] are configured, so there is nothing to trust");
    }
    trust.trust(root, &config.hooks)?;
    if json {
        return Ok(serde_json::to_string_pretty(&json!({
            "trusted": config.hooks.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
        }))?);
    }
    Ok(format!("Trusted {} hook(s).", config.hooks.len()))
}

pub fn findings_json(findings: &[ReportedFinding]) -> Value {
    Value::Array(findings.iter().map(ReportedFinding::to_json).collect())
}

/// What to print and the exit code for a move a hook refused under `--json`,
/// or `None` when `error` is something else (or the output is not JSON) and
/// should propagate as an ordinary error.
pub fn blocked_exit(error: &anyhow::Error, json: bool) -> Option<(String, i32)> {
    if !json {
        return None;
    }
    blocked_json(error).map(|body| (body, 1))
}

fn blocked_json(error: &anyhow::Error) -> Option<String> {
    let blocked = error.downcast_ref::<TransitionBlocked>()?;
    let body = json!({
        "error": "blocked by pre-transition hooks",
        "findings": findings_json(&blocked.findings),
    });
    Some(serde_json::to_string_pretty(&body).expect("findings serialise as JSON"))
}

#[allow(clippy::too_many_arguments)]
pub fn run_hook(
    env: &HookEnv,
    root: &Path,
    config: &Config,
    store: &Store,
    event: &str,
    id: &str,
    dry_run: bool,
    git: &dyn GitRefOps,
    json: bool,
) -> Result<String> {
    if event != HookEvent::PreTransition.as_str() {
        bail!(
            "`hook run` fires `{}` hooks; `{event}` runs with `validate`",
            HookEvent::PreTransition
        );
    }
    let outcome =
        crate::engine::ops::update::run_hooks_by_hand(env, root, store, id, dry_run, config, git)?;
    if json {
        return Ok(serde_json::to_string_pretty(&json!({
            "event": event,
            "id": id,
            "dry_run": dry_run,
            "findings": findings_json(&outcome.warnings),
            "updates": outcome.updates.iter().map(|u| u.to_json()).collect::<Vec<_>>(),
        }))?);
    }
    let mut lines: Vec<String> = outcome.warnings.iter().map(|f| f.to_string()).collect();
    let verb = if dry_run { "Would update" } else { "Updated" };
    lines.extend(outcome.updates.iter().map(|u| match &u.part {
        Some(part) => format!("{verb} {}/{part}", u.id),
        None => format!("{verb} {}", u.id),
    }));
    if lines.is_empty() {
        lines.push(format!("No {event} hooks had anything to report for {id}."));
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config::parse(
            r#"
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[hooks]]
name = "lint"
event = "validate"
types = ["rfc"]
run = ["x"]
"#,
        )
        .unwrap()
    }

    #[test]
    fn list_shows_name_event_scope_and_trust_and_trust_flips_it() {
        let tmp = tempfile::tempdir().unwrap();
        let trust = TrustStore::in_dir(&tmp.path().join("state"));
        let config = config();

        let before: Value =
            serde_json::from_str(&run_list(tmp.path(), &config, &trust, true)).unwrap();
        assert_eq!(before[0]["name"], "lint");
        assert_eq!(before[0]["event"], "validate");
        assert_eq!(before[0]["scope"]["types"], json!(["rfc"]));
        assert_eq!(before[0]["trust"], "untrusted");
        assert!(run_list(tmp.path(), &config, &trust, false).contains("untrusted"));

        run_trust(tmp.path(), &config, &trust, false).unwrap();

        let after: Value =
            serde_json::from_str(&run_list(tmp.path(), &config, &trust, true)).unwrap();
        assert_eq!(after[0]["trust"], "trusted");
    }

    #[test]
    fn trusting_nothing_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let trust = TrustStore::in_dir(tmp.path());
        assert!(run_trust(tmp.path(), &Config::default(), &trust, false).is_err());
    }
}
