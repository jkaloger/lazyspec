use anyhow::{anyhow, Result};
use std::path::Path;

use super::agent_metadata::AgentMetadataWriter;
use super::config::Config;
use super::git_ref::GitRefOps;
use super::lease::{parse_duration, Lease, LeaseEngine};
use super::tick::Clock;

/// One lease this host owned at boot. Captured during the pre-sleep scan so
/// the post-sleep release path doesn't re-scan (the scan and release are
/// split so the grace_period sleep can sit between them).
#[derive(Debug, Clone)]
struct OrphanLease {
    type_name: String,
    id: String,
    agent: String,
}

/// Boot-time orphan lease recovery. RFC-041 §Boot recovery.
///
/// Scans local lease refs for ones owned by this host (agent prefix
/// `{host_id}:`), waits `grace_period` if any are found, then admin-releases
/// each and marks the corresponding agent session `crashed`. Worktrees are
/// left in place — the operator decides whether to resume or discard.
///
/// Scan-then-sleep-then-release (rather than `release_by_host_prefix`'s
/// scan+release in one shot) is intentional: RFC-041 requires the grace
/// window to elapse *before* the release, so a concurrent live process from
/// this host has a chance to heartbeat its lease and "rescue" it from the
/// scan list. On the same host running a single daemon this race is
/// effectively empty, but the conservative ordering matches the RFC.
pub fn boot_orphan_recovery<G, M, C>(
    root: &Path,
    host_id: &str,
    lease_engine: &LeaseEngine<G>,
    config: &Config,
    clock: &C,
    metadata: &M,
) -> Result<()>
where
    G: GitRefOps,
    M: AgentMetadataWriter,
    C: Clock,
{
    let coord = config
        .coordination
        .as_ref()
        .ok_or_else(|| anyhow!("coordination config missing; boot recovery cannot run"))?;

    let needle = format!("{}:", host_id);
    let mut orphans: Vec<OrphanLease> = Vec::new();

    for type_def in &config.documents.types {
        let type_name = &type_def.name;
        let pattern = format!("refs/lazyspec/leases/{}/*", type_name);
        let ref_prefix = format!("refs/lazyspec/leases/{}/", type_name);
        let refs = match lease_engine.git.list_refs(root, &pattern) {
            Ok(r) => r,
            Err(e) => {
                eprintln!(
                    "boot recovery: failed to list lease refs for type '{}': {}",
                    type_name, e
                );
                continue;
            }
        };
        for (refname, sha) in refs {
            let blob = match lease_engine.git.read_ref_blob(root, &sha, "lease.json") {
                Ok(b) => b,
                Err(e) => {
                    eprintln!(
                        "boot recovery: failed to read lease blob {}: {}",
                        refname, e
                    );
                    continue;
                }
            };
            let lease: Lease = match serde_json::from_str(&blob) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!(
                        "boot recovery: failed to parse lease blob {}: {}",
                        refname, e
                    );
                    continue;
                }
            };
            if !lease.agent.starts_with(&needle) {
                continue;
            }
            let id = match refname.strip_prefix(&ref_prefix) {
                Some(id) => id.to_string(),
                None => {
                    eprintln!(
                        "boot recovery: lease ref {} did not match prefix {}",
                        refname, ref_prefix
                    );
                    continue;
                }
            };
            orphans.push(OrphanLease {
                type_name: type_name.clone(),
                id,
                agent: lease.agent,
            });
        }
    }

    if orphans.is_empty() {
        eprintln!("boot recovery: 0 orphans");
        return Ok(());
    }

    let grace = parse_duration(&coord.grace_period)?;
    let std_grace = grace
        .to_std()
        .map_err(|e| anyhow!("invalid grace_period (negative duration?): {}", e))?;

    eprintln!(
        "boot recovery: found {} orphan(s) for host={}; waiting grace_period={}",
        orphans.len(),
        host_id,
        coord.grace_period
    );
    clock.sleep(std_grace);

    for orphan in &orphans {
        eprintln!(
            "boot recovery: admin-releasing {}/{} (agent={})",
            orphan.type_name, orphan.id, orphan.agent
        );
        if let Err(e) = lease_engine.release(root, &orphan.type_name, &orphan.id, &orphan.agent) {
            eprintln!(
                "boot recovery: failed to release {}/{}: {}",
                orphan.type_name, orphan.id, e
            );
            continue;
        }
        let session_id = match orphan.agent.split_once(':') {
            Some((_, s)) if !s.is_empty() => s,
            _ => {
                eprintln!(
                    "boot recovery: agent ident '{}' has no session_id after ':'; skipping mark_crashed",
                    orphan.agent
                );
                continue;
            }
        };
        eprintln!("boot recovery: marking {} as crashed", session_id);
        if let Err(e) = metadata.mark_crashed(session_id) {
            eprintln!(
                "boot recovery: failed to mark session {} as crashed: {}",
                session_id, e
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::agent_metadata::AgentMetadataWriter;
    use crate::engine::config::{
        Config, CoordinationConfig, DocumentConfig, FilesystemConfig, Naming, NumberingStrategy,
        StoreBackend, TypeDef, UiConfig,
    };
    use crate::engine::config::{Directories, Templates};
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use chrono::{DateTime, Duration as ChronoDuration, Utc};
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    // ---- Fakes ----------------------------------------------------------

    struct FakeClock {
        now_instant: Mutex<Instant>,
        now_utc: Mutex<DateTime<Utc>>,
        sleeps: Mutex<Vec<Duration>>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now_instant: Mutex::new(Instant::now()),
                now_utc: Mutex::new(Utc::now()),
                sleeps: Mutex::new(Vec::new()),
            }
        }
        fn sleep_durations(&self) -> Vec<Duration> {
            self.sleeps.lock().unwrap().clone()
        }
    }

    impl Clock for FakeClock {
        fn now_instant(&self) -> Instant {
            *self.now_instant.lock().unwrap()
        }
        fn now_utc(&self) -> DateTime<Utc> {
            *self.now_utc.lock().unwrap()
        }
        fn sleep(&self, dur: Duration) {
            self.sleeps.lock().unwrap().push(dur);
        }
    }

    #[derive(Default)]
    struct RecordingAgentMetadata {
        crashed: Mutex<Vec<String>>,
    }

    impl RecordingAgentMetadata {
        fn crashed_sessions(&self) -> Vec<String> {
            self.crashed.lock().unwrap().clone()
        }
    }

    impl AgentMetadataWriter for RecordingAgentMetadata {
        fn mark_crashed(&self, session_id: &str) -> Result<()> {
            self.crashed.lock().unwrap().push(session_id.to_string());
            Ok(())
        }
    }

    // ---- Helpers --------------------------------------------------------

    fn dummy_root() -> PathBuf {
        PathBuf::from("/tmp/fake")
    }

    fn test_coord() -> CoordinationConfig {
        CoordinationConfig {
            remote: "origin".into(),
            lease_duration: "60m".into(),
            grace_period: "2m".into(),
            max_push_retries: 5,
            max_clock_skew: "5m".into(),
        }
    }

    fn type_def(name: &str) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            plural: format!("{}s", name),
            dir: format!("docs/{}s", name),
            prefix: name.to_uppercase(),
            icon: None,
            numbering: NumberingStrategy::default(),
            subdirectory: false,
            store: StoreBackend::default(),
            singleton: false,
            parent_type: None,
        }
    }

    fn config_with_types(types: Vec<TypeDef>) -> Config {
        Config {
            documents: DocumentConfig {
                types,
                naming: Naming {
                    pattern: "{type}-{n:03}-{title}.md".to_string(),
                },
                sqids: None,
                reserved: None,
                github: None,
            },
            filesystem: FilesystemConfig {
                directories: Directories {
                    rfcs: "docs/rfcs".into(),
                    adrs: "docs/adrs".into(),
                    stories: "docs/stories".into(),
                    iterations: "docs/iterations".into(),
                },
                templates: Templates {
                    dir: ".lazyspec/templates".into(),
                },
            },
            ui: UiConfig::default(),
            rules: vec![],
            ref_count_ceiling: 15,
            certification: Default::default(),
            coordination: Some(test_coord()),
            orchestration: None,
        }
    }

    fn lease_json(agent: &str) -> String {
        let now = Utc::now();
        serde_json::to_string_pretty(&Lease {
            agent: agent.to_string(),
            acquired: now,
            expires: now + ChronoDuration::minutes(60),
        })
        .unwrap()
    }

    fn lease_ref_pair(type_name: &str, id: &str, sha: &str) -> (String, String) {
        (
            format!("refs/lazyspec/leases/{}/{}", type_name, id),
            sha.to_string(),
        )
    }

    // ---- Tests ----------------------------------------------------------

    #[test]
    fn boot_recovery_noop_when_no_orphans() {
        let mock = MockGitRefClient::new().with_list_result(Ok(vec![]));
        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        assert!(
            clock.sleep_durations().is_empty(),
            "no sleep when no orphans"
        );
        assert!(
            metadata.crashed_sessions().is_empty(),
            "no mark_crashed when no orphans"
        );
        let calls = engine.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("delete_remote_ref:")),
            "no release when no orphans"
        );
    }

    #[test]
    fn boot_recovery_finds_orphans_by_host_prefix() {
        // Two refs: one ours, one another host's. Only ours should be released.
        let our_lease = lease_json("host-A:sess-1");
        let other_lease = lease_json("host-B:sess-2");

        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![
                lease_ref_pair("story", "STORY-001", "sha1"),
                lease_ref_pair("story", "STORY-002", "sha2"),
            ]))
            .with_read_blob_result(Ok(our_lease.clone()))
            .with_read_blob_result(Ok(other_lease))
            // release() for STORY-001 needs: fetch, resolve, read_blob, delete_remote, delete_ref
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(our_lease))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        let calls = engine.git.calls.borrow();
        let delete_remote: Vec<&String> = calls
            .iter()
            .filter(|c| c.starts_with("delete_remote_ref:"))
            .collect();
        assert_eq!(
            delete_remote.len(),
            1,
            "exactly one release for our orphan, got calls: {:?}",
            calls
        );
        assert!(delete_remote[0].contains("STORY-001"));
        assert!(!delete_remote[0].contains("STORY-002"));
    }

    #[test]
    fn boot_recovery_waits_grace_period_when_orphans_exist() {
        let our_lease = lease_json("host-A:sess-1");
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![lease_ref_pair("story", "STORY-001", "sha1")]))
            .with_read_blob_result(Ok(our_lease.clone()))
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(our_lease))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        let sleeps = clock.sleep_durations();
        assert_eq!(sleeps.len(), 1, "exactly one sleep call");
        // grace_period = 2m in test_coord
        assert_eq!(sleeps[0], Duration::from_secs(120));
    }

    #[test]
    fn boot_recovery_no_sleep_when_no_orphans() {
        // Refs exist but none match our host prefix.
        let other_lease = lease_json("host-B:sess-1");
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![lease_ref_pair("story", "STORY-001", "sha1")]))
            .with_read_blob_result(Ok(other_lease));

        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        assert!(
            clock.sleep_durations().is_empty(),
            "must not sleep when no orphan matches host prefix"
        );
    }

    #[test]
    fn boot_recovery_admin_releases_each_orphan() {
        // Two orphans of ours across two types.
        let our_lease_a = lease_json("host-A:sess-1");
        let our_lease_b = lease_json("host-A:sess-2");

        let mock = MockGitRefClient::new()
            // list_refs: first for "story", then for "iteration"
            .with_list_result(Ok(vec![lease_ref_pair("story", "STORY-001", "sha1")]))
            .with_list_result(Ok(vec![lease_ref_pair(
                "iteration",
                "ITERATION-001",
                "sha2",
            )]))
            // prefilter blob reads (one per ref)
            .with_read_blob_result(Ok(our_lease_a.clone()))
            .with_read_blob_result(Ok(our_lease_b.clone()))
            // release for STORY-001
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(our_lease_a))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()))
            // release for ITERATION-001
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha2".to_string())))
            .with_read_blob_result(Ok(our_lease_b))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story"), type_def("iteration")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        let calls = engine.git.calls.borrow();
        let releases: Vec<&String> = calls
            .iter()
            .filter(|c| c.starts_with("delete_remote_ref:"))
            .collect();
        assert_eq!(releases.len(), 2);
        assert!(releases.iter().any(|c| c.contains("STORY-001")));
        assert!(releases.iter().any(|c| c.contains("ITERATION-001")));
    }

    #[test]
    fn boot_recovery_marks_session_crashed() {
        let our_lease = lease_json("host-A:session-xyz");
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![lease_ref_pair("story", "STORY-001", "sha1")]))
            .with_read_blob_result(Ok(our_lease.clone()))
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(our_lease))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        let crashed = metadata.crashed_sessions();
        assert_eq!(crashed, vec!["session-xyz".to_string()]);
    }

    #[test]
    fn boot_recovery_ignores_other_host_leases() {
        // All refs belong to other hosts. No release, no sleep, no mark.
        let other_a = lease_json("otherhost:sess-1");
        let other_b = lease_json("yetanother:sess-2");
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![
                lease_ref_pair("story", "STORY-001", "sha1"),
                lease_ref_pair("story", "STORY-002", "sha2"),
            ]))
            .with_read_blob_result(Ok(other_a))
            .with_read_blob_result(Ok(other_b));

        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        assert!(clock.sleep_durations().is_empty());
        assert!(metadata.crashed_sessions().is_empty());
        let calls = engine.git.calls.borrow();
        assert!(!calls.iter().any(|c| c.starts_with("delete_remote_ref:")));
    }

    #[test]
    fn boot_recovery_prefix_boundary_excludes_lookalike_hosts() {
        // host-A prefix must not match "host-A-foo" — the trailing ':' is the boundary.
        let lookalike = lease_json("host-A-foo:sess-1");
        let mock = MockGitRefClient::new()
            .with_list_result(Ok(vec![lease_ref_pair("story", "STORY-001", "sha1")]))
            .with_read_blob_result(Ok(lookalike));

        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let config = config_with_types(vec![type_def("story")]);

        boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata).unwrap();

        assert!(clock.sleep_durations().is_empty());
        assert!(metadata.crashed_sessions().is_empty());
    }

    #[test]
    fn boot_recovery_errors_when_coordination_missing() {
        let mock = MockGitRefClient::new();
        let engine = LeaseEngine::new(mock, test_coord());
        let clock = FakeClock::new();
        let metadata = RecordingAgentMetadata::default();
        let mut config = config_with_types(vec![type_def("story")]);
        config.coordination = None;

        let err =
            boot_orphan_recovery(&dummy_root(), "host-A", &engine, &config, &clock, &metadata)
                .unwrap_err();
        assert!(err.to_string().contains("coordination config missing"));
    }
}
