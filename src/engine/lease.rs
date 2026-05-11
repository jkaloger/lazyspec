use anyhow::{bail, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::config::CoordinationConfig;
use super::git_ref::GitRefOps;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Lease {
    pub agent: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub acquired: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub expires: DateTime<Utc>,
}

pub fn parse_duration(s: &str) -> Result<Duration> {
    if s.is_empty() {
        bail!("empty duration string");
    }
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: i64 = num_str
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid duration number: {}", num_str))?;
    match unit {
        "s" => Ok(Duration::seconds(num)),
        "m" => Ok(Duration::minutes(num)),
        "h" => Ok(Duration::hours(num)),
        _ => bail!("unknown duration unit '{}', expected s/m/h", unit),
    }
}

fn lease_ref(type_name: &str, id: &str) -> String {
    format!("refs/lazyspec/leases/{}/{}", type_name, id)
}

fn lease_glob(type_name: &str) -> String {
    format!("refs/lazyspec/leases/{}/*", type_name)
}

const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

pub fn fetch_ref_optional(
    git: &impl GitRefOps,
    root: &Path,
    remote: &str,
    refname: &str,
) -> Result<()> {
    if let Err(e) = git.fetch_refs(root, remote, refname) {
        if !e.to_string().contains("couldn't find remote ref") {
            return Err(e);
        }
    }
    Ok(())
}

pub struct LeaseEngine<R: GitRefOps> {
    pub git: R,
    pub config: CoordinationConfig,
}

impl<R: GitRefOps> LeaseEngine<R> {
    pub fn new(git: R, config: CoordinationConfig) -> Self {
        Self { git, config }
    }

    pub fn acquire(
        &self,
        root: &Path,
        type_name: &str,
        id: &str,
        agent: &str,
        now: DateTime<Utc>,
    ) -> Result<Lease> {
        let refname = lease_ref(type_name, id);

        // Glob fetch so --prune removes stale local lease refs whose remote counterparts are gone.
        fetch_ref_optional(&self.git, root, &self.config.remote, &lease_glob(type_name))?;
        let existing = self.git.resolve_ref(root, &refname)?;
        if existing.is_some() {
            bail!("lease held");
        }

        let duration = parse_duration(&self.config.lease_duration)?;
        let lease = Lease {
            agent: agent.to_string(),
            acquired: now,
            expires: now + duration,
        };
        let json = serde_json::to_string_pretty(&lease)?;
        // Create commit object only (no local ref update yet). Remote CAS via
        // --force-with-lease=ref:0000...0000 is the linearization point; local ref
        // advances only on push success to avoid acquire/crash phantom-create windows.
        let new_sha = self
            .git
            .create_commit(root, &refname, &[("lease.json", &json)], None)?;
        self.git.push_ref_with_lease(
            root,
            &self.config.remote,
            &refname,
            &new_sha,
            Some(ZERO_SHA),
        )?;
        self.git.update_ref(root, &refname, &new_sha, ZERO_SHA)?;
        Ok(lease)
    }

    pub fn release(&self, root: &Path, type_name: &str, id: &str, agent: &str) -> Result<()> {
        self.delete_lease(root, type_name, id, agent, |holder, expected| {
            format!("lease held by '{}', not '{}'", holder, expected)
        })
    }

    pub fn admin_release(
        &self,
        root: &Path,
        type_name: &str,
        id: &str,
        expected_holder: &str,
    ) -> Result<()> {
        self.delete_lease(root, type_name, id, expected_holder, |holder, expected| {
            format!(
                "expected holder '{}', but lease held by '{}'",
                expected, holder
            )
        })
    }

    fn delete_lease(
        &self,
        root: &Path,
        type_name: &str,
        id: &str,
        expected_agent: &str,
        mismatch_msg: impl FnOnce(&str, &str) -> String,
    ) -> Result<()> {
        let refname = lease_ref(type_name, id);
        // Glob fetch with prune so a remote-absent ref clears the stale local ref;
        // otherwise a single-ref fetch leaves the local view authoritative.
        fetch_ref_optional(&self.git, root, &self.config.remote, &lease_glob(type_name))?;
        let sha = self
            .git
            .resolve_ref(root, &refname)?
            .ok_or_else(|| anyhow::anyhow!("no lease found"))?;
        let blob = self.git.read_ref_blob(root, &sha, "lease.json")?;
        let lease: Lease = serde_json::from_str(&blob)?;
        if lease.agent != expected_agent {
            bail!("{}", mismatch_msg(&lease.agent, expected_agent));
        }
        // CAS delete: refuse to delete if the remote ref no longer matches the sha we verified.
        self.git
            .delete_remote_ref(root, &self.config.remote, &refname, Some(&sha))?;
        self.git.delete_ref(root, &refname)?;
        Ok(())
    }

    pub fn heartbeat(
        &self,
        root: &Path,
        type_name: &str,
        id: &str,
        agent: &str,
        now: DateTime<Utc>,
    ) -> Result<Lease> {
        let refname = lease_ref(type_name, id);
        // Fetch-before-check: without this, a release+force-acquire by another agent is
        // invisible and the subsequent plain push would resurrect a deleted lease.
        fetch_ref_optional(&self.git, root, &self.config.remote, &lease_glob(type_name))?;
        let old_sha = self
            .git
            .resolve_ref(root, &refname)?
            .ok_or_else(|| anyhow::anyhow!("no lease found"))?;
        let blob = self.git.read_ref_blob(root, &old_sha, "lease.json")?;
        let lease: Lease = serde_json::from_str(&blob)?;
        if lease.agent != agent {
            bail!("lease held by '{}', not '{}'", lease.agent, agent);
        }

        let duration = parse_duration(&self.config.lease_duration)?;
        let updated = Lease {
            agent: agent.to_string(),
            acquired: lease.acquired,
            expires: now + duration,
        };
        let json = serde_json::to_string_pretty(&updated)?;
        let new_sha =
            self.git
                .create_commit(root, &refname, &[("lease.json", &json)], Some(&old_sha))?;
        // Remote CAS before local mutation: --force-with-lease=ref:old_sha rejects pushes
        // when the remote has moved or the ref is absent (no phantom resurrection).
        self.git.push_ref_with_lease(
            root,
            &self.config.remote,
            &refname,
            &new_sha,
            Some(&old_sha),
        )?;
        self.git.update_ref(root, &refname, &new_sha, &old_sha)?;
        Ok(updated)
    }

    pub fn force_acquire(
        &self,
        root: &Path,
        type_name: &str,
        id: &str,
        agent: &str,
        now: DateTime<Utc>,
    ) -> Result<Lease> {
        let refname = lease_ref(type_name, id);
        fetch_ref_optional(&self.git, root, &self.config.remote, &refname)?;
        let sha = self
            .git
            .resolve_ref(root, &refname)?
            .ok_or_else(|| anyhow::anyhow!("no lease found to force-acquire"))?;
        let blob = self.git.read_ref_blob(root, &sha, "lease.json")?;
        let _lease: Lease = serde_json::from_str(&blob)?;

        let last_touched = self.git.read_commit_timestamp(root, &sha)?;
        // Reject leases whose commit timestamp is implausibly far in the future. Defends
        // against GIT_COMMITTER_DATE forgery that would otherwise make a lease un-stealable.
        let max_skew = parse_duration(&self.config.max_clock_skew)?;
        if last_touched > now + max_skew {
            bail!(
                "lease commit timestamp {} is more than {} ahead of local clock {}",
                last_touched,
                self.config.max_clock_skew,
                now
            );
        }
        let duration = parse_duration(&self.config.lease_duration)?;
        let grace = parse_duration(&self.config.grace_period)?;
        let effective_expiry = last_touched + duration + grace;
        if now <= effective_expiry {
            bail!("lease not expired beyond grace period");
        }

        let new_lease = Lease {
            agent: agent.to_string(),
            acquired: now,
            expires: now + duration,
        };
        let json = serde_json::to_string_pretty(&new_lease)?;
        let new_sha =
            self.git
                .create_commit(root, &refname, &[("lease.json", &json)], Some(&sha))?;
        self.git
            .push_ref_with_lease(root, &self.config.remote, &refname, &new_sha, Some(&sha))?;
        self.git.update_ref(root, &refname, &new_sha, &sha)?;
        Ok(new_lease)
    }

    pub fn query(&self, root: &Path) -> Result<Vec<(String, Lease)>> {
        if let Err(e) = self
            .git
            .fetch_refs(root, &self.config.remote, "refs/lazyspec/leases/*")
        {
            eprintln!("warning: failed to fetch lease refs: {}", e);
        }
        let refs = self.git.list_refs(root, "refs/lazyspec/leases/")?;
        let mut result = Vec::new();
        for (refname, sha) in refs {
            let blob = self.git.read_ref_blob(root, &sha, "lease.json")?;
            let lease: Lease = serde_json::from_str(&blob)?;
            result.push((refname, lease));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use std::path::PathBuf;

    fn dummy_root() -> PathBuf {
        PathBuf::from("/tmp/fake")
    }

    fn test_config() -> CoordinationConfig {
        CoordinationConfig {
            remote: "origin".to_string(),
            lease_duration: "60m".to_string(),
            grace_period: "2m".to_string(),
            max_push_retries: 5,
            max_clock_skew: "5m".to_string(),
        }
    }

    fn fixed_now() -> DateTime<Utc> {
        "2025-01-15T12:00:00Z".parse().unwrap()
    }

    fn make_lease_json(agent: &str, acquired: DateTime<Utc>, expires: DateTime<Utc>) -> String {
        serde_json::to_string_pretty(&Lease {
            agent: agent.to_string(),
            acquired,
            expires,
        })
        .unwrap()
    }

    // --- parse_duration tests ---

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), Duration::seconds(30));
    }

    #[test]
    fn parse_duration_minutes() {
        assert_eq!(parse_duration("60m").unwrap(), Duration::minutes(60));
    }

    #[test]
    fn parse_duration_hours() {
        assert_eq!(parse_duration("2h").unwrap(), Duration::hours(2));
    }

    #[test]
    fn parse_duration_invalid_unit() {
        assert!(parse_duration("10d").is_err());
    }

    #[test]
    fn parse_duration_invalid_number() {
        assert!(parse_duration("abcm").is_err());
    }

    #[test]
    fn parse_duration_empty() {
        assert!(parse_duration("").is_err());
    }

    // --- acquire tests ---

    #[test]
    fn acquire_unclaimed_succeeds() {
        let now = fixed_now();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(None))
            .with_create_commit_result(Ok("sha1".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        let lease = engine
            .acquire(&dummy_root(), "story", "STORY-001", "agent-a", now)
            .unwrap();

        assert_eq!(lease.agent, "agent-a");
        assert_eq!(lease.acquired, now);
        assert_eq!(lease.expires, now + Duration::minutes(60));

        let calls = engine.git.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:"));
        assert!(calls[0].contains("refs/lazyspec/leases/story/*"));
        assert!(calls[1].starts_with("resolve_ref:"));
        assert!(calls[2].starts_with("create_commit:refs/lazyspec/leases/story/STORY-001"));
        assert_eq!(
            calls[3],
            format!(
                "push_ref_with_lease:origin:refs/lazyspec/leases/story/STORY-001:new_sha=sha1:expected_old=Some(\"{}\")",
                ZERO_SHA
            )
        );
        assert_eq!(
            calls[4],
            format!(
                "update_ref:refs/lazyspec/leases/story/STORY-001:sha1:{}",
                ZERO_SHA
            )
        );
    }

    #[test]
    fn acquire_uses_force_with_lease_zero_sha() {
        let now = fixed_now();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(None))
            .with_create_commit_result(Ok("sha1".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .acquire(&dummy_root(), "story", "STORY-001", "agent-a", now)
            .unwrap();

        let calls = engine.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("create_ref_commit")),
            "acquire must not use create_ref_commit (would advance local ref before remote CAS)"
        );
        assert!(
            !calls.iter().any(|c| c.starts_with("push_ref:")),
            "acquire must not use plain push_ref"
        );
    }

    #[test]
    fn acquire_does_not_advance_local_ref_when_push_fails() {
        let now = fixed_now();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(None))
            .with_create_commit_result(Ok("sha1".to_string()))
            .with_push_with_lease_result(Err(anyhow::anyhow!("stale info: ref exists on remote")));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .acquire(&dummy_root(), "story", "STORY-001", "agent-a", now)
            .unwrap_err();
        assert!(err.to_string().contains("stale info"));

        let calls = engine.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("update_ref:")),
            "local update_ref must not run if remote push fails"
        );
    }

    #[test]
    fn acquire_already_claimed_fails() {
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("existing-sha".to_string())));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .acquire(&dummy_root(), "story", "STORY-001", "agent-b", fixed_now())
            .unwrap_err();

        assert!(err.to_string().contains("lease held"));
    }

    // --- release tests ---

    #[test]
    fn release_by_holder_succeeds() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .release(&dummy_root(), "story", "STORY-001", "agent-a")
            .unwrap();

        let calls = engine.git.calls.borrow();
        let delete_remote = calls
            .iter()
            .find(|c| c.starts_with("delete_remote_ref:"))
            .expect("expected delete_remote_ref call");
        assert_eq!(
            delete_remote,
            "delete_remote_ref:origin:refs/lazyspec/leases/story/STORY-001:expected_old=Some(\"sha1\")"
        );
        assert!(calls.iter().any(|c| c.starts_with("delete_ref:")));
    }

    #[test]
    fn release_fetches_glob_with_prune() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .release(&dummy_root(), "story", "STORY-001", "agent-a")
            .unwrap();

        let calls = engine.git.calls.borrow();
        assert!(
            calls[0].starts_with("fetch_refs:")
                && calls[0].contains("refs/lazyspec/leases/story/*"),
            "release must glob-fetch (not single-ref fetch) so absent remote refs prune local: got {}",
            calls[0]
        );
    }

    #[test]
    fn release_by_non_holder_fails() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .release(&dummy_root(), "story", "STORY-001", "agent-b")
            .unwrap_err();

        assert!(err.to_string().contains("agent-a"));
        assert!(err.to_string().contains("agent-b"));
    }

    // --- admin_release tests ---

    #[test]
    fn admin_release_matching_holder_succeeds() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .admin_release(&dummy_root(), "story", "STORY-001", "agent-a")
            .unwrap();
    }

    #[test]
    fn admin_release_non_matching_holder_fails() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .admin_release(&dummy_root(), "story", "STORY-001", "agent-b")
            .unwrap_err();

        assert!(err.to_string().contains("expected holder 'agent-b'"));
        assert!(err.to_string().contains("agent-a"));
    }

    // --- heartbeat tests ---

    #[test]
    fn heartbeat_by_holder_extends_expiry() {
        let acquired = fixed_now();
        let old_expires = acquired + Duration::minutes(60);
        let heartbeat_time = acquired + Duration::minutes(30);
        let lease_json = make_lease_json("agent-a", acquired, old_expires);

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        let updated = engine
            .heartbeat(
                &dummy_root(),
                "story",
                "STORY-001",
                "agent-a",
                heartbeat_time,
            )
            .unwrap();

        assert_eq!(updated.agent, "agent-a");
        assert_eq!(updated.acquired, acquired);
        assert_eq!(updated.expires, heartbeat_time + Duration::minutes(60));

        let calls = engine.git.calls.borrow();
        assert!(calls.iter().any(|c| c.contains("create_commit:")));
        assert!(
            !calls.iter().any(|c| c.contains("create_ref_commit")),
            "heartbeat should use create_commit, not create_ref_commit"
        );
        let update_call = calls
            .iter()
            .find(|c| c.starts_with("update_ref:"))
            .expect("expected update_ref call");
        assert_eq!(
            update_call,
            "update_ref:refs/lazyspec/leases/story/STORY-001:new-sha:old-sha"
        );
        assert!(
            !calls.iter().any(|c| c.starts_with("push_ref:")),
            "heartbeat must not use plain push_ref"
        );
        let push_call = calls
            .iter()
            .find(|c| c.starts_with("push_ref_with_lease:"))
            .expect("expected push_ref_with_lease call");
        assert_eq!(
            push_call,
            "push_ref_with_lease:origin:refs/lazyspec/leases/story/STORY-001:new_sha=new-sha:expected_old=Some(\"old-sha\")"
        );
    }

    #[test]
    fn heartbeat_fetches_before_resolve() {
        let acquired = fixed_now();
        let lease_json = make_lease_json("agent-a", acquired, acquired + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .heartbeat(&dummy_root(), "story", "STORY-001", "agent-a", acquired)
            .unwrap();

        let calls = engine.git.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:"));
        assert!(calls[0].contains("refs/lazyspec/leases/story/*"));
        assert!(calls[1].starts_with("resolve_ref:"));
    }

    #[test]
    fn heartbeat_does_not_advance_local_ref_when_push_fails() {
        // Phantom-resurrection guard: if remote was deleted (e.g. force-acquire-then-release
        // by another agent), --force-with-lease=ref:old_sha must fail and local must not advance.
        let acquired = fixed_now();
        let lease_json = make_lease_json("agent-a", acquired, acquired + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Err(anyhow::anyhow!("stale info")));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .heartbeat(&dummy_root(), "story", "STORY-001", "agent-a", acquired)
            .unwrap_err();
        assert!(err.to_string().contains("stale info"));

        let calls = engine.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("update_ref:")),
            "local update_ref must not run if remote push fails"
        );
    }

    #[test]
    fn heartbeat_uses_create_commit_then_push_then_cas() {
        // Remote CAS via --force-with-lease must precede the local update_ref so that local
        // never advances past what the remote accepts.
        let acquired = fixed_now();
        let old_expires = acquired + Duration::minutes(60);
        let heartbeat_time = acquired + Duration::minutes(30);
        let lease_json = make_lease_json("agent-a", acquired, old_expires);

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .heartbeat(
                &dummy_root(),
                "story",
                "STORY-001",
                "agent-a",
                heartbeat_time,
            )
            .unwrap();

        let calls = engine.git.calls.borrow();
        assert_eq!(calls.len(), 6);
        assert!(calls[0].starts_with("fetch_refs:"));
        assert!(calls[1].starts_with("resolve_ref:"));
        assert!(calls[2].starts_with("read_ref_blob:"));
        assert!(calls[3].starts_with("create_commit:"));
        assert!(calls[3].contains("parent=Some(\"old-sha\")"));
        assert!(calls[4].starts_with("push_ref_with_lease:"));
        assert!(calls[5].starts_with("update_ref:"));
    }

    #[test]
    fn heartbeat_by_non_holder_fails() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .heartbeat(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap_err();

        assert!(err.to_string().contains("agent-a"));
    }

    // --- force_acquire tests ---

    #[test]
    fn force_acquire_expired_beyond_grace_succeeds() {
        let acquired = fixed_now();
        let expired = acquired + Duration::minutes(60);
        let now = expired + Duration::minutes(5); // well beyond 2m grace
        let lease_json = make_lease_json("agent-a", acquired, expired);
        // commit timestamp = acquired; acquired + 60m(duration) + 2m(grace) = acquired + 62m < now (acquired + 65m)
        let commit_timestamp = acquired;

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(commit_timestamp))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        let lease = engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap();

        assert_eq!(lease.agent, "agent-b");
        assert_eq!(lease.acquired, now);
    }

    #[test]
    fn force_acquire_within_grace_period_fails() {
        let acquired = fixed_now();
        let expired = acquired + Duration::minutes(60);
        let now = expired + Duration::minutes(1); // within 2m grace: acquired + 62m > now (acquired + 61m)
        let lease_json = make_lease_json("agent-a", acquired, expired);
        let commit_timestamp = acquired;

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(commit_timestamp));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap_err();

        assert!(err.to_string().contains("grace period"));
    }

    #[test]
    fn force_acquire_non_expired_fails() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        // commit timestamp = now, so now + 60m + 2m >> now; clearly not expired
        let commit_timestamp = now;

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(commit_timestamp));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap_err();

        assert!(err.to_string().contains("grace period"));
    }

    // --- query tests ---

    #[test]
    fn acquire_succeeds_when_remote_ref_missing() {
        let now = fixed_now();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Err(anyhow::anyhow!(
                "fatal: couldn't find remote ref refs/lazyspec/leases/story/STORY-NEW"
            )))
            .with_resolve_result(Ok(None))
            .with_create_ref_commit_result(Ok("sha1".to_string()))
            .with_push_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        let lease = engine
            .acquire(&dummy_root(), "story", "STORY-NEW", "agent-a", now)
            .unwrap();

        assert_eq!(lease.agent, "agent-a");
        assert_eq!(lease.acquired, now);
        assert_eq!(lease.expires, now + Duration::minutes(60));
    }

    #[test]
    fn acquire_propagates_real_network_errors() {
        let mock =
            MockGitRefClient::new().with_fetch_result(Err(anyhow::anyhow!("network timeout")));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .acquire(&dummy_root(), "story", "STORY-001", "agent-a", fixed_now())
            .unwrap_err();

        assert!(err.to_string().contains("network timeout"));
    }

    #[test]
    fn force_acquire_missing_remote_ref_fails_with_no_lease() {
        let now = fixed_now();
        let mock = MockGitRefClient::new()
            .with_fetch_result(Err(anyhow::anyhow!(
                "fatal: couldn't find remote ref refs/lazyspec/leases/story/STORY-NEW"
            )))
            .with_resolve_result(Ok(None));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .force_acquire(&dummy_root(), "story", "STORY-NEW", "agent-b", now)
            .unwrap_err();

        assert!(err.to_string().contains("no lease found to force-acquire"));
    }

    #[test]
    fn force_acquire_uses_push_with_lease() {
        let acquired = fixed_now();
        let expired = acquired + Duration::minutes(60);
        let now = expired + Duration::minutes(5);
        let lease_json = make_lease_json("agent-a", acquired, expired);
        let commit_timestamp = acquired;

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(commit_timestamp))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap();

        let calls = engine.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.contains("delete_remote_ref")),
            "force_acquire should not call delete_remote_ref"
        );
        assert!(
            !calls.iter().any(|c| c.contains("delete_ref")),
            "force_acquire should not call delete_ref"
        );
        assert!(
            !calls.iter().any(|c| c.starts_with("create_ref_commit")),
            "force_acquire should use create_commit, not create_ref_commit"
        );

        let push_lease_call = calls
            .iter()
            .find(|c| c.starts_with("push_ref_with_lease:"))
            .expect("expected push_ref_with_lease call");
        assert_eq!(
            push_lease_call,
            "push_ref_with_lease:origin:refs/lazyspec/leases/story/STORY-001:new_sha=new-sha:expected_old=Some(\"old-sha\")"
        );

        let update_call = calls
            .iter()
            .find(|c| c.starts_with("update_ref:"))
            .expect("expected update_ref call");
        assert_eq!(
            update_call,
            "update_ref:refs/lazyspec/leases/story/STORY-001:new-sha:old-sha"
        );
    }

    #[test]
    fn force_acquire_fails_if_ref_changed() {
        let acquired = fixed_now();
        let expired = acquired + Duration::minutes(60);
        let now = expired + Duration::minutes(5);
        let lease_json = make_lease_json("agent-a", acquired, expired);
        let commit_timestamp = acquired;

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(commit_timestamp))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Err(anyhow::anyhow!(
                "git push --force-with-lease failed: stale info"
            )));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap_err();

        assert!(err.to_string().contains("force-with-lease failed"));

        let calls = engine.git.calls.borrow();
        assert!(
            !calls.iter().any(|c| c.starts_with("update_ref:")),
            "update_ref should not be called when push_ref_with_lease fails"
        );
    }

    #[test]
    fn force_acquire_propagates_real_network_errors() {
        let now = fixed_now();
        let mock =
            MockGitRefClient::new().with_fetch_result(Err(anyhow::anyhow!("network timeout")));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap_err();

        assert!(err.to_string().contains("network timeout"));
    }

    #[test]
    fn query_returns_all_leases() {
        let now = fixed_now();
        let lease1_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let lease2_json = make_lease_json("agent-b", now, now + Duration::minutes(60));

        let refs = vec![
            (
                "refs/lazyspec/leases/story/STORY-001".to_string(),
                "sha1".to_string(),
            ),
            (
                "refs/lazyspec/leases/rfc/RFC-010".to_string(),
                "sha2".to_string(),
            ),
        ];

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(refs))
            .with_read_blob_result(Ok(lease1_json))
            .with_read_blob_result(Ok(lease2_json));

        let engine = LeaseEngine::new(mock, test_config());
        let result = engine.query(&dummy_root()).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, "refs/lazyspec/leases/story/STORY-001");
        assert_eq!(result[0].1.agent, "agent-a");
        assert_eq!(result[1].0, "refs/lazyspec/leases/rfc/RFC-010");
        assert_eq!(result[1].1.agent, "agent-b");
    }

    #[test]
    fn delete_lease_fetches_before_resolve() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("sha1".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_delete_remote_result(Ok(()))
            .with_delete_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .release(&dummy_root(), "story", "STORY-001", "agent-a")
            .unwrap();

        let calls = engine.git.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:"));
        assert!(calls[1].starts_with("resolve_ref:"));
    }

    #[test]
    fn query_fetches_before_list() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let refs = vec![(
            "refs/lazyspec/leases/story/STORY-001".to_string(),
            "sha1".to_string(),
        )];

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_list_result(Ok(refs))
            .with_read_blob_result(Ok(lease_json));

        let engine = LeaseEngine::new(mock, test_config());
        let result = engine.query(&dummy_root()).unwrap();

        assert_eq!(result.len(), 1);
        let calls = engine.git.calls.borrow();
        assert!(calls[0].starts_with("fetch_refs:"));
        assert!(calls[0].contains("refs/lazyspec/leases/*"));
        assert!(calls[1].starts_with("list_refs:"));
    }

    #[test]
    fn query_succeeds_when_fetch_fails() {
        let now = fixed_now();
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let refs = vec![(
            "refs/lazyspec/leases/story/STORY-001".to_string(),
            "sha1".to_string(),
        )];

        let mock = MockGitRefClient::new()
            .with_fetch_result(Err(anyhow::anyhow!("network timeout")))
            .with_list_result(Ok(refs))
            .with_read_blob_result(Ok(lease_json));

        let engine = LeaseEngine::new(mock, test_config());
        let result = engine.query(&dummy_root()).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.agent, "agent-a");
    }

    #[test]
    fn force_acquire_uses_commit_timestamp() {
        // Verify that force_acquire uses commit timestamp (not lease.expires) for expiry check.
        // Set lease.expires far in the future but commit timestamp old enough to be expired.
        let acquired = fixed_now();
        let far_future_expires = acquired + Duration::hours(24);
        let lease_json = make_lease_json("agent-a", acquired, far_future_expires);
        // commit timestamp is old: old_ts + 60m + 2m < now
        let old_commit_timestamp = acquired - Duration::hours(2);
        let now = acquired;

        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(old_commit_timestamp))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        let lease = engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap();

        assert_eq!(lease.agent, "agent-b");

        let calls = engine.git.calls.borrow();
        assert!(calls.iter().any(|c| c == "read_commit_timestamp:old-sha"));
    }

    #[test]
    fn force_acquire_rejects_commit_timestamp_beyond_skew_bound() {
        // Defends against GIT_COMMITTER_DATE forgery: if a holder stamps the lease commit
        // far in the future, force_acquire must reject the lease as untrusted rather than
        // honour the future timestamp (which would otherwise make the lease un-stealable).
        let now = fixed_now();
        let future_commit_timestamp = now + Duration::hours(2);
        let lease_json = make_lease_json("agent-a", now, now + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(future_commit_timestamp));

        let engine = LeaseEngine::new(mock, test_config());
        let err = engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap_err();

        assert!(
            err.to_string().contains("ahead of local clock"),
            "expected skew error, got: {}",
            err
        );
    }

    #[test]
    fn force_acquire_accepts_commit_timestamp_within_skew_bound() {
        // Trusted clock skew (NTP drift) is tolerated up to max_clock_skew.
        let acquired = fixed_now();
        let slightly_future_commit_ts = acquired + Duration::minutes(2);
        let now = slightly_future_commit_ts + Duration::hours(2);
        let lease_json = make_lease_json("agent-a", acquired, acquired + Duration::minutes(60));
        let mock = MockGitRefClient::new()
            .with_fetch_result(Ok(()))
            .with_resolve_result(Ok(Some("old-sha".to_string())))
            .with_read_blob_result(Ok(lease_json))
            .with_read_commit_timestamp_result(Ok(slightly_future_commit_ts))
            .with_create_commit_result(Ok("new-sha".to_string()))
            .with_push_with_lease_result(Ok(()))
            .with_update_ref_result(Ok(()));

        let engine = LeaseEngine::new(mock, test_config());
        engine
            .force_acquire(&dummy_root(), "story", "STORY-001", "agent-b", now)
            .unwrap();
    }
}
