use crate::engine::agent::resolve_agent_id;
use crate::engine::config::Config;
use crate::engine::git_ref::{GitCli, GitRefOps};
use crate::engine::lease::{fetch_ref_optional, Lease, LeaseEngine};
use anyhow::{bail, Result};
use chrono::Utc;
use serde::Serialize;
use std::path::Path;

fn extract_doc_id(input: &str, config: &Config) -> Option<String> {
    for t in &config.documents.types {
        if input.starts_with(&t.prefix) {
            return Some(input.to_string());
        }
    }
    let filename = std::path::Path::new(input)
        .file_stem()
        .and_then(|s| s.to_str())?;
    let mut parts = filename.splitn(3, '-');
    let prefix = parts.next()?;
    let number = parts.next()?;
    let candidate = format!("{}-{}", prefix, number);
    for t in &config.documents.types {
        if candidate.starts_with(&t.prefix) {
            return Some(candidate);
        }
    }
    None
}

fn resolve_doc_type<'a>(config: &'a Config, doc_id: &str) -> Result<&'a str> {
    config
        .documents
        .types
        .iter()
        .find(|t| doc_id.starts_with(&t.prefix))
        .map(|t| t.name.as_str())
        .ok_or_else(|| anyhow::anyhow!("no document type matches prefix of '{}'", doc_id))
}

fn require_coordination(config: &Config) -> Result<LeaseEngine<GitCli>> {
    let coord = config.coordination.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "coordination is not configured; add a [coordination] section to .lazyspec.toml"
        )
    })?;
    Ok(LeaseEngine::new(GitCli, coord))
}

pub fn check_lease_gate_with<R: GitRefOps>(
    root: &Path,
    config: &Config,
    doc_id: Option<&str>,
    git: &R,
    agent: &str,
) -> Result<()> {
    let coord = match &config.coordination {
        Some(c) => c,
        None => return Ok(()),
    };

    if let Some(raw_id) = doc_id {
        let id = extract_doc_id(raw_id, config).unwrap_or_else(|| raw_id.to_string());
        let type_name = resolve_doc_type(config, &id)?;
        let refname = format!("refs/lazyspec/leases/{}/{}", type_name, id);
        fetch_ref_optional(git, root, &coord.remote, &refname)?;
        let sha = git.resolve_ref(root, &refname)?;
        match sha {
            Some(sha) => {
                let blob = git.read_ref_blob(root, &sha, "lease.json")?;
                let lease: Lease = serde_json::from_str(&blob)?;
                if lease.agent != agent {
                    bail!(
                        "document is not claimed by you (held by '{}'). Run `lazyspec claim {}` first.",
                        lease.agent, id
                    );
                }
            }
            None => {
                bail!(
                    "document is not claimed. Run `lazyspec claim {}` first.",
                    id
                );
            }
        }
    } else {
        if let Err(e) = git.fetch_refs(root, &coord.remote, "refs/lazyspec/leases/*") {
            eprintln!("warning: could not fetch lease refs from remote: {}", e);
        }
        let refs = git.list_refs(root, "refs/lazyspec/leases/")?;
        let mut has_lease = false;
        for (_, sha) in &refs {
            let blob = git.read_ref_blob(root, sha, "lease.json")?;
            let lease: Lease = serde_json::from_str(&blob)?;
            if lease.agent == agent {
                has_lease = true;
                break;
            }
        }
        if !has_lease {
            bail!("no active lease. Run `lazyspec claim <id>` first.");
        }
    }
    Ok(())
}

pub fn check_lease_gate(root: &Path, config: &Config, doc_id: Option<&str>) -> Result<()> {
    if config.coordination.is_none() {
        return Ok(());
    }
    let agent = resolve_agent_id(root)?;
    check_lease_gate_with(root, config, doc_id, &GitCli, &agent)
}

pub fn run_claim(
    root: &Path,
    config: &Config,
    doc_id: &str,
    agent_id: Option<&str>,
    force: bool,
    json: bool,
) -> Result<()> {
    let engine = require_coordination(config)?;
    let type_name = resolve_doc_type(config, doc_id)?;
    let agent = match agent_id {
        Some(id) => id.to_string(),
        None => resolve_agent_id(root)?,
    };
    let lease = if force {
        engine.force_acquire(root, type_name, doc_id, &agent, Utc::now())?
    } else {
        engine.acquire(root, type_name, doc_id, &agent, Utc::now())?
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&lease)?);
    } else {
        println!(
            "Claimed {} (agent={}, expires={})",
            doc_id, lease.agent, lease.expires
        );
    }
    Ok(())
}

pub fn run_release(
    root: &Path,
    config: &Config,
    doc_id: &str,
    agent_id: Option<&str>,
    expected_holder: Option<&str>,
    json: bool,
) -> Result<()> {
    let engine = require_coordination(config)?;
    let type_name = resolve_doc_type(config, doc_id)?;
    if let Some(holder) = expected_holder {
        engine.admin_release(root, type_name, doc_id, holder)?;
    } else {
        let agent = match agent_id {
            Some(id) => id.to_string(),
            None => resolve_agent_id(root)?,
        };
        engine.release(root, type_name, doc_id, &agent)?;
    }
    if json {
        println!("{}", serde_json::json!({ "released": doc_id }));
    } else {
        println!("Released {}", doc_id);
    }
    Ok(())
}

#[derive(Serialize)]
struct LeaseEntry {
    #[serde(rename = "ref")]
    ref_name: String,
    agent: String,
    acquired: String,
    expires: String,
}

pub fn run_leases(root: &Path, config: &Config, json: bool) -> Result<()> {
    let engine = require_coordination(config)?;
    let leases = engine.query(root)?;
    let entries: Vec<LeaseEntry> = leases
        .into_iter()
        .map(|(ref_name, lease)| LeaseEntry {
            ref_name,
            agent: lease.agent,
            acquired: lease.acquired.to_rfc3339(),
            expires: lease.expires.to_rfc3339(),
        })
        .collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&entries)?);
    } else {
        for e in &entries {
            println!("{}\t{}\t{}\t{}", e.ref_name, e.agent, e.acquired, e.expires);
        }
    }
    Ok(())
}

pub fn run_heartbeat(
    root: &Path,
    config: &Config,
    doc_id: &str,
    agent_id: Option<&str>,
    json: bool,
) -> Result<()> {
    let engine = require_coordination(config)?;
    let type_name = resolve_doc_type(config, doc_id)?;
    let agent = match agent_id {
        Some(id) => id.to_string(),
        None => resolve_agent_id(root)?,
    };
    let lease = engine.heartbeat(root, type_name, doc_id, &agent, Utc::now())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&lease)?);
    } else {
        println!(
            "Heartbeat {} (agent={}, expires={})",
            doc_id, lease.agent, lease.expires
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{
        CertificationConfig, Config, CoordinationConfig, Directories, DocumentConfig,
        FilesystemConfig, Naming, NumberingStrategy, StoreBackend, Templates, TypeDef, UiConfig,
    };
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use crate::engine::lease::Lease;
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    fn dummy_root() -> PathBuf {
        PathBuf::from("/tmp/fake")
    }

    fn config_with_coordination() -> Config {
        Config {
            documents: DocumentConfig {
                types: vec![
                    TypeDef {
                        name: "rfc".to_string(),
                        plural: "rfcs".to_string(),
                        dir: "docs/rfcs".to_string(),
                        prefix: "RFC-".to_string(),
                        icon: None,
                        numbering: NumberingStrategy::default(),
                        subdirectory: false,
                        store: StoreBackend::default(),
                        singleton: false,
                        parent_type: None,
                    },
                    TypeDef {
                        name: "story".to_string(),
                        plural: "stories".to_string(),
                        dir: "docs/stories".to_string(),
                        prefix: "STORY-".to_string(),
                        icon: None,
                        numbering: NumberingStrategy::default(),
                        subdirectory: false,
                        store: StoreBackend::default(),
                        singleton: false,
                        parent_type: None,
                    },
                ],
                naming: Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                },
                sqids: None,
                reserved: None,
                github: None,
            },
            filesystem: FilesystemConfig {
                directories: Directories {
                    rfcs: "docs/rfcs".to_string(),
                    adrs: "docs/adrs".to_string(),
                    stories: "docs/stories".to_string(),
                    iterations: "docs/iterations".to_string(),
                },
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            ui: UiConfig::default(),
            rules: vec![],
            ref_count_ceiling: 15,
            certification: CertificationConfig::default(),
            coordination: Some(CoordinationConfig {
                remote: "origin".to_string(),
                lease_duration: "60m".to_string(),
                grace_period: "2m".to_string(),
                max_push_retries: 5,
            }),
        }
    }

    fn config_without_coordination() -> Config {
        let mut config = config_with_coordination();
        config.coordination = None;
        config
    }

    fn make_lease_json(agent: &str) -> String {
        let now = Utc::now();
        serde_json::to_string_pretty(&Lease {
            agent: agent.to_string(),
            acquired: now,
            expires: now + Duration::minutes(60),
        })
        .unwrap()
    }

    #[test]
    fn no_coordination_skips_check() {
        let config = config_without_coordination();
        let mock = MockGitRefClient::new();
        let result =
            check_lease_gate_with(&dummy_root(), &config, Some("RFC-001"), &mock, "agent-a");
        assert!(result.is_ok());
        assert!(mock.calls.borrow().is_empty());
    }

    #[test]
    fn specific_doc_with_held_lease_proceeds() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(make_lease_json("agent-a")));
        let result =
            check_lease_gate_with(&dummy_root(), &config, Some("RFC-001"), &mock, "agent-a");
        assert!(result.is_ok());
        let calls = mock.calls.borrow();
        assert!(calls[0].contains("refs/lazyspec/leases/rfc/RFC-001"));
    }

    #[test]
    fn specific_doc_with_no_lease_errors() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new().with_resolve_result(Ok(None));
        let result =
            check_lease_gate_with(&dummy_root(), &config, Some("RFC-001"), &mock, "agent-a");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("document is not claimed"));
        assert!(err.to_string().contains("lazyspec claim"));
    }

    #[test]
    fn specific_doc_held_by_other_agent_errors() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(make_lease_json("agent-b")));
        let result =
            check_lease_gate_with(&dummy_root(), &config, Some("RFC-001"), &mock, "agent-a");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not claimed by you"));
        assert!(err.to_string().contains("agent-b"));
    }

    #[test]
    fn create_with_any_active_lease_proceeds() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![(
                "refs/lazyspec/leases/rfc/RFC-001".to_string(),
                "sha1".to_string(),
            )]))
            .with_read_blob_result(Ok(make_lease_json("agent-a")));
        let result = check_lease_gate_with(&dummy_root(), &config, None, &mock, "agent-a");
        assert!(result.is_ok());
    }

    #[test]
    fn create_with_no_leases_errors() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new().with_list_result(Ok(vec![]));
        let result = check_lease_gate_with(&dummy_root(), &config, None, &mock, "agent-a");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("no active lease"));
    }

    #[test]
    fn create_with_only_other_agent_leases_errors() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![(
                "refs/lazyspec/leases/rfc/RFC-001".to_string(),
                "sha1".to_string(),
            )]))
            .with_read_blob_result(Ok(make_lease_json("agent-b")));
        let result = check_lease_gate_with(&dummy_root(), &config, None, &mock, "agent-a");
        let err = result.unwrap_err();
        assert!(err.to_string().contains("no active lease"));
    }

    #[test]
    fn extract_doc_id_from_shorthand() {
        let config = config_with_coordination();
        assert_eq!(
            extract_doc_id("RFC-001", &config),
            Some("RFC-001".to_string())
        );
        assert_eq!(
            extract_doc_id("STORY-042", &config),
            Some("STORY-042".to_string())
        );
    }

    #[test]
    fn extract_doc_id_from_path() {
        let config = config_with_coordination();
        assert_eq!(
            extract_doc_id("docs/rfcs/RFC-001-some-title.md", &config),
            Some("RFC-001".to_string()),
        );
        assert_eq!(
            extract_doc_id("docs/stories/STORY-042-my-story.md", &config),
            Some("STORY-042".to_string()),
        );
    }

    #[test]
    fn extract_doc_id_unknown_prefix_returns_none() {
        let config = config_with_coordination();
        assert_eq!(extract_doc_id("UNKNOWN-001", &config), None);
    }

    #[test]
    fn path_input_resolves_and_checks_lease() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(make_lease_json("agent-a")));
        let result = check_lease_gate_with(
            &dummy_root(),
            &config,
            Some("docs/rfcs/RFC-001-some-title.md"),
            &mock,
            "agent-a",
        );
        assert!(result.is_ok());
        let calls = mock.calls.borrow();
        assert!(calls[0].contains("refs/lazyspec/leases/rfc/RFC-001"));
    }

    #[test]
    fn gate_fetches_before_resolve() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(make_lease_json("agent-a")));
        let result =
            check_lease_gate_with(&dummy_root(), &config, Some("RFC-001"), &mock, "agent-a");
        assert!(result.is_ok());
        let calls = mock.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:origin:refs/lazyspec/leases/rfc/RFC-001"));
        assert!(calls[1].starts_with("resolve_ref:refs/lazyspec/leases/rfc/RFC-001"));
    }

    #[test]
    fn gate_falls_back_to_local_on_fetch_failure() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Err(anyhow::anyhow!(
                "couldn't find remote ref refs/lazyspec/leases/rfc/RFC-001"
            )))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(make_lease_json("agent-a")));
        let result =
            check_lease_gate_with(&dummy_root(), &config, Some("RFC-001"), &mock, "agent-a");
        assert!(result.is_ok());
        let calls = mock.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:"));
        assert!(calls[1].starts_with("resolve_ref:"));
    }

    #[test]
    fn gate_fetches_glob_before_list_for_create() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/leases/rfc/RFC-001".to_string(),
                "sha1".to_string(),
            )]))
            .with_read_blob_result(Ok(make_lease_json("agent-a")));
        let result = check_lease_gate_with(&dummy_root(), &config, None, &mock, "agent-a");
        assert!(result.is_ok());
        let calls = mock.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:origin:refs/lazyspec/leases/"));
        assert!(calls[1].starts_with("list_refs:refs/lazyspec/leases/"));
    }

    #[test]
    fn gate_glob_fetch_failure_falls_back_to_local() {
        let config = config_with_coordination();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Err(anyhow::anyhow!("network timeout")))
            .with_list_result(Ok(vec![(
                "refs/lazyspec/leases/rfc/RFC-001".to_string(),
                "sha1".to_string(),
            )]))
            .with_read_blob_result(Ok(make_lease_json("agent-a")));
        let result = check_lease_gate_with(&dummy_root(), &config, None, &mock, "agent-a");
        assert!(result.is_ok());
        let calls = mock.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:"));
        assert!(calls[1].starts_with("list_refs:"));
    }
}
