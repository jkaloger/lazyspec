use crate::engine::agent::resolve_agent_id;
use crate::engine::config::Config;
use crate::engine::git_ref::{GitCli, GitRefOps};
use crate::engine::lease::{fetch_ref_optional, Lease, LeaseEngine};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    min_interval: Option<&str>,
    json: bool,
) -> Result<()> {
    let engine = require_coordination(config)?;
    run_heartbeat_with(
        root,
        config,
        &engine,
        doc_id,
        agent_id,
        min_interval,
        Utc::now(),
        json,
    )
}

fn heartbeat_state_path(root: &Path, type_name: &str, doc_id: &str) -> PathBuf {
    root.join(".lazyspec/state")
        .join(format!("heartbeat-{}-{}", type_name, doc_id))
}

fn write_heartbeat_state(state_path: &Path, now: DateTime<Utc>) -> Result<()> {
    let dir = state_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("state path has no parent"))?;
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(now.to_rfc3339().as_bytes())?;
    tmp.persist(state_path)
        .map_err(|e| anyhow::anyhow!("persist failed: {}", e))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn run_heartbeat_with<R: GitRefOps>(
    root: &Path,
    config: &Config,
    engine: &LeaseEngine<R>,
    doc_id: &str,
    agent_id: Option<&str>,
    min_interval: Option<&str>,
    now: DateTime<Utc>,
    json: bool,
) -> Result<()> {
    let type_name = resolve_doc_type(config, doc_id)?;
    let agent = match agent_id {
        Some(id) => id.to_string(),
        None => resolve_agent_id(root)?,
    };

    if let Some(s) = min_interval {
        let interval = crate::engine::lease::parse_duration(s)?;
        let state_path = heartbeat_state_path(root, type_name, doc_id);
        match std::fs::read_to_string(&state_path) {
            Ok(raw) => {
                let last = DateTime::parse_from_rfc3339(raw.trim())?.with_timezone(&Utc);
                if now - last < interval {
                    if json {
                        println!(
                            "{}",
                            serde_json::json!({
                                "skipped": true,
                                "reason": "throttled",
                                "last": last.to_rfc3339(),
                            })
                        );
                    } else {
                        println!(
                            "Heartbeat {} skipped (throttled, last {})",
                            doc_id,
                            last.to_rfc3339()
                        );
                    }
                    return Ok(());
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    let lease = engine.heartbeat(root, type_name, doc_id, &agent, now)?;

    if min_interval.is_some() {
        let state_path = heartbeat_state_path(root, type_name, doc_id);
        if let Err(e) = write_heartbeat_state(&state_path, now) {
            eprintln!("warning: failed to write heartbeat state file: {}", e);
        }
    }

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
        CertificationConfig, Config, CoordinationConfig, DocumentConfig, FilesystemConfig, Naming,
        NumberingStrategy, StoreBackend, Templates, TypeDef, UiConfig,
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
                        agents: Vec::new(),
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
                        agents: Vec::new(),
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
                templates: Templates {
                    dir: ".lazyspec/templates".to_string(),
                },
            },
            relationships: crate::engine::config::starter_relationships(),
            ui: UiConfig::default(),
            rules: vec![],
            ref_count_ceiling: 15,
            certification: CertificationConfig::default(),
            coordination: Some(CoordinationConfig {
                remote: "origin".to_string(),
                lease_duration: "60m".to_string(),
                grace_period: "2m".to_string(),
                max_push_retries: 5,
                max_clock_skew: "5m".to_string(),
            }),
            agents: Default::default(),
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

    // --- heartbeat throttle tests ---
    //
    // Note: AC4 specifies a JSON shape `{"skipped":true,"reason":"throttled","last":"<rfc3339>"}`
    // for the throttled-skip case. Stdout capture isn't available without a new dev-dep
    // (`gag`/`assert_cmd` are not in Cargo.toml). The throttle path is covered behaviourally
    // by `heartbeat_skips_when_last_run_within_interval` (engine never called, state file
    // untouched). JSON-shape coverage is deferred to an integration test.

    use chrono::TimeZone;

    fn engine_for(config: &Config, mock: MockGitRefClient) -> LeaseEngine<MockGitRefClient> {
        LeaseEngine::new(mock, config.coordination.clone().unwrap())
    }

    fn fixed_now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 11, 12, 0, 0).unwrap()
    }

    fn succeeding_mock() -> MockGitRefClient {
        MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha-old".to_string())))
            .with_read_blob_result(Ok(make_lease_json("agent-a")))
            .with_create_commit_result(Ok("sha-new".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()))
    }

    #[test]
    fn heartbeat_without_min_interval_runs_unconditionally() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with_coordination();
        let engine = engine_for(&config, succeeding_mock());
        let now = fixed_now();

        let result = run_heartbeat_with(
            dir.path(),
            &config,
            &engine,
            "RFC-001",
            Some("agent-a"),
            None,
            now,
            true,
        );

        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
        assert!(
            !engine.git.calls.borrow().is_empty(),
            "engine should be called when min_interval is None"
        );
        assert!(
            !dir.path().join(".lazyspec/state").exists(),
            "no state I/O when min_interval is None"
        );
    }

    #[test]
    fn heartbeat_skips_when_last_run_within_interval() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with_coordination();
        let now = fixed_now();
        let last = now - Duration::minutes(5);

        let state_dir = dir.path().join(".lazyspec/state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let state_path = state_dir.join("heartbeat-rfc-RFC-001");
        std::fs::write(&state_path, last.to_rfc3339()).unwrap();

        // Mock has no results queued; if engine is called, pop_or_default will return defaults
        // (e.g. resolve -> Ok(None)) which would make heartbeat fail with "no lease found".
        // So either path -- success or err -- shows engine ran. We assert mock.calls is empty.
        let engine = engine_for(&config, MockGitRefClient::new());

        let result = run_heartbeat_with(
            dir.path(),
            &config,
            &engine,
            "RFC-001",
            Some("agent-a"),
            Some("15m"),
            now,
            true,
        );

        assert!(result.is_ok(), "throttled skip should be Ok");
        assert!(
            engine.git.calls.borrow().is_empty(),
            "engine must not be called on throttle: {:?}",
            engine.git.calls.borrow()
        );
        let written = std::fs::read_to_string(&state_path).unwrap();
        assert_eq!(
            written,
            last.to_rfc3339(),
            "state file should be unchanged on skip"
        );
    }

    #[test]
    fn heartbeat_runs_when_state_file_older_than_interval() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with_coordination();
        let now = fixed_now();
        let last = now - Duration::minutes(30);

        let state_dir = dir.path().join(".lazyspec/state");
        std::fs::create_dir_all(&state_dir).unwrap();
        let state_path = state_dir.join("heartbeat-rfc-RFC-001");
        std::fs::write(&state_path, last.to_rfc3339()).unwrap();

        let engine = engine_for(&config, succeeding_mock());

        let result = run_heartbeat_with(
            dir.path(),
            &config,
            &engine,
            "RFC-001",
            Some("agent-a"),
            Some("15m"),
            now,
            true,
        );

        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
        assert!(
            !engine.git.calls.borrow().is_empty(),
            "engine should have been called when state is stale"
        );
        let written = std::fs::read_to_string(&state_path).unwrap();
        assert_eq!(
            written,
            now.to_rfc3339(),
            "state file should be updated to `now` after a successful heartbeat"
        );
    }

    #[test]
    fn heartbeat_runs_when_state_file_absent() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with_coordination();
        let now = fixed_now();

        let engine = engine_for(&config, succeeding_mock());

        let result = run_heartbeat_with(
            dir.path(),
            &config,
            &engine,
            "RFC-001",
            Some("agent-a"),
            Some("15m"),
            now,
            true,
        );

        assert!(result.is_ok(), "expected Ok, got {:?}", result.err());
        assert!(
            !engine.git.calls.borrow().is_empty(),
            "engine should be called when state file is absent"
        );
        let state_path = dir.path().join(".lazyspec/state/heartbeat-rfc-RFC-001");
        assert!(state_path.exists(), "state file should be written");
        let written = std::fs::read_to_string(&state_path).unwrap();
        assert_eq!(written, now.to_rfc3339());
    }

    #[test]
    fn heartbeat_state_file_written_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let config = config_with_coordination();
        let now = fixed_now();
        let engine = engine_for(&config, succeeding_mock());

        run_heartbeat_with(
            dir.path(),
            &config,
            &engine,
            "RFC-001",
            Some("agent-a"),
            Some("15m"),
            now,
            true,
        )
        .unwrap();

        let state_dir = dir.path().join(".lazyspec/state");
        let entries: Vec<_> = std::fs::read_dir(&state_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(
            entries.len(),
            1,
            "expected exactly one file in state dir, found {:?}",
            entries
        );
        assert_eq!(entries[0], "heartbeat-rfc-RFC-001");
    }

    #[test]
    #[cfg(unix)]
    fn heartbeat_state_write_failure_does_not_fail_command() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let config = config_with_coordination();
        let now = fixed_now();

        let state_dir = dir.path().join(".lazyspec/state");
        std::fs::create_dir_all(&state_dir).unwrap();
        std::fs::set_permissions(&state_dir, std::fs::Permissions::from_mode(0o555)).unwrap();

        let engine = engine_for(&config, succeeding_mock());

        let result = run_heartbeat_with(
            dir.path(),
            &config,
            &engine,
            "RFC-001",
            Some("agent-a"),
            Some("15m"),
            now,
            true,
        );

        // Restore perms so TempDir cleanup succeeds.
        std::fs::set_permissions(&state_dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(
            result.is_ok(),
            "state write failure should not fail command, got {:?}",
            result.err()
        );
    }
}
