//! Orchestrator tick loop. Heartbeat of the daemon: polls eligible documents,
//! acquires RFC-035 leases, dispatches agents, heartbeats live leases.
//!
//! Iter A scope (AC1-7): dispatch + lease acquire/heartbeat. Reconciliation,
//! retry, stall detection (AC8-14) and boot recovery / preflight (AC15-16) live
//! in later iterations.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{DateTime, Utc};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use uuid::Uuid;

use super::agent::lease_agent_id;
use super::branch_template::{render_branch_name, BranchVars};
use super::config::Config;
use super::dispatcher::{Candidate, Dispatcher};
use super::git_ref::GitRefOps;
use super::lease::{fetch_ref_optional, local_lease_ids, LeaseEngine};
use super::runner::{AgentContext, AgentEvent, AgentHandle, AgentRunner};
use super::store::Store;
use super::workspace::{provision_workspace, Workspace};

/// Object-safe handle the daemon uses to run a tick loop on a background
/// thread. Erases the `TickLoop` generic parameters so the daemon can hold
/// `Option<Box<dyn TickRunner>>` regardless of which concrete impls of
/// `AgentRunner`, `GitRefOps`, `LeaseOps`, `Clock`, `WorkspaceProvisioner`
/// are wired in.
pub trait TickRunner: Send {
    fn run(self: Box<Self>, shutdown_rx: Receiver<()>) -> Result<()>;
}

impl<R, G, L, C, W> TickRunner for TickLoop<R, G, L, C, W>
where
    R: AgentRunner + Send + 'static,
    G: GitRefOps + Send + 'static,
    L: LeaseOps + Send + 'static,
    C: Clock + Send + 'static,
    W: WorkspaceProvisioner + Send + 'static,
{
    fn run(mut self: Box<Self>, shutdown_rx: Receiver<()>) -> Result<()> {
        self.run_until(shutdown_rx)
    }
}

pub trait Clock: Send + Sync {
    fn now_instant(&self) -> Instant;
    fn now_utc(&self) -> DateTime<Utc>;
    fn sleep(&self, dur: Duration);
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now_instant(&self) -> Instant {
        Instant::now()
    }
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
    fn sleep(&self, dur: Duration) {
        std::thread::sleep(dur);
    }
}

/// Subset of LeaseEngine ops the tick loop calls. Trait seam so tests can
/// inject a recording fake rather than wire MockGitRefClient sequences for
/// every git op LeaseEngine performs under the hood.
pub trait LeaseOps {
    fn acquire(&self, type_name: &str, doc_id: &str, agent: &str, now: DateTime<Utc>)
        -> Result<()>;
    fn heartbeat(
        &self,
        type_name: &str,
        doc_id: &str,
        agent: &str,
        now: DateTime<Utc>,
    ) -> Result<()>;
    fn release(&self, type_name: &str, doc_id: &str, agent: &str) -> Result<()>;
}

pub struct EngineLeaseOps<G: GitRefOps> {
    pub engine: LeaseEngine<G>,
    pub root: PathBuf,
}

/// Provisions a per-claim worktree. Trait seam for tests so they don't shell
/// out to `git worktree add`.
pub trait WorkspaceProvisioner {
    fn provision(
        &self,
        repo_root: &std::path::Path,
        workspace_root: &std::path::Path,
        base_branch: &str,
        branch: &str,
        claim_id: &str,
    ) -> Result<Workspace>;
}

pub struct GitWorktreeProvisioner;

impl WorkspaceProvisioner for GitWorktreeProvisioner {
    fn provision(
        &self,
        repo_root: &std::path::Path,
        workspace_root: &std::path::Path,
        base_branch: &str,
        branch: &str,
        claim_id: &str,
    ) -> Result<Workspace> {
        provision_workspace(repo_root, workspace_root, base_branch, branch, claim_id)
    }
}

impl<G: GitRefOps> LeaseOps for EngineLeaseOps<G> {
    fn acquire(
        &self,
        type_name: &str,
        doc_id: &str,
        agent: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.engine
            .acquire(&self.root, type_name, doc_id, agent, now)
            .map(|_| ())
    }
    fn heartbeat(
        &self,
        type_name: &str,
        doc_id: &str,
        agent: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        self.engine
            .heartbeat(&self.root, type_name, doc_id, agent, now)
            .map(|_| ())
    }
    fn release(&self, type_name: &str, doc_id: &str, agent: &str) -> Result<()> {
        self.engine.release(&self.root, type_name, doc_id, agent)
    }
}

pub struct RunningAgent {
    pub session_id: String,
    pub doc_id: String,
    pub doc_type: String,
    pub agent_ident: String,
    pub handle: AgentHandle,
    pub last_heartbeat: Instant,
}

pub struct TickLoop<R: AgentRunner, G: GitRefOps, L: LeaseOps, C: Clock, W: WorkspaceProvisioner> {
    pub root: PathBuf,
    pub config: Config,
    pub host_id: String,
    pub runner: R,
    pub git: G,
    pub lease_ops: L,
    pub clock: C,
    pub workspace_provisioner: W,
    pub running: HashMap<String, RunningAgent>,
    pub last_metadata_push: Option<Instant>,
}

fn lease_glob(type_name: &str) -> String {
    format!("refs/lazyspec/leases/{}/*", type_name)
}

impl<R: AgentRunner, G: GitRefOps, L: LeaseOps, C: Clock, W: WorkspaceProvisioner>
    TickLoop<R, G, L, C, W>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        root: PathBuf,
        config: Config,
        host_id: String,
        runner: R,
        git: G,
        lease_ops: L,
        clock: C,
        workspace_provisioner: W,
    ) -> Self {
        Self {
            root,
            config,
            host_id,
            runner,
            git,
            lease_ops,
            clock,
            workspace_provisioner,
            running: HashMap::new(),
            last_metadata_push: None,
        }
    }

    pub fn run_once(&mut self) -> Result<()> {
        let orch = self
            .config
            .orchestration
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("orchestration config missing"))?
            .clone();
        let coord_remote = self
            .config
            .coordination
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("coordination config missing"))?
            .remote
            .clone();

        let now_instant = self.clock.now_instant();

        // AC7: gated metadata-push fetch. Per-tick code never fetches outside this gate.
        let push_due = match self.last_metadata_push {
            None => true,
            Some(t) => {
                now_instant.duration_since(t)
                    >= Duration::from_millis(orch.metadata_push_interval_ms)
            }
        };
        if push_due {
            for type_def in &self.config.documents.types {
                let glob = lease_glob(&type_def.name);
                if let Err(e) = fetch_ref_optional(&self.git, &self.root, &coord_remote, &glob) {
                    eprintln!("tick: fetch leases {} failed: {}", glob, e);
                }
            }
            self.last_metadata_push = Some(now_instant);
        }

        // Reap exited agents (Iter A: any exit reason).
        self.reap_exited(&orch.claim_type);

        // AC5: heartbeat sweep.
        let hb_interval = Duration::from_millis(orch.heartbeat_interval_ms);
        let mut dead_after_hb: Vec<String> = Vec::new();
        for (doc_id, ra) in self.running.iter_mut() {
            if now_instant.duration_since(ra.last_heartbeat) >= hb_interval {
                match self.lease_ops.heartbeat(
                    &ra.doc_type,
                    &ra.doc_id,
                    &ra.agent_ident,
                    self.clock.now_utc(),
                ) {
                    Ok(()) => {
                        ra.last_heartbeat = now_instant;
                    }
                    Err(e) => {
                        eprintln!(
                            "tick: heartbeat {}/{} failed: {}; dropping agent",
                            ra.doc_type, ra.doc_id, e
                        );
                        dead_after_hb.push(doc_id.clone());
                    }
                }
            }
        }
        for doc_id in dead_after_hb {
            if let Some(ra) = self.running.remove(&doc_id) {
                let _ = self
                    .lease_ops
                    .release(&ra.doc_type, &ra.doc_id, &ra.agent_ident);
            }
        }

        // AC2: fetch candidates from store, sliced by claim_type.
        let candidates = self.load_candidates(&orch.claim_type)?;

        // AC2: build local active_lease_ids from local refs (no fetch).
        let active_lease_ids = self.local_active_lease_ids(&orch.claim_type);

        let running_ids: HashSet<String> = self.running.keys().cloned().collect();

        let dispatcher = Dispatcher {
            orchestration: &orch,
            active_lease_ids: &active_lease_ids,
            running_ids: &running_ids,
        };
        let eligible = dispatcher.eligible(&candidates);
        let slots = dispatcher.slots_available(self.running.len());
        let selected: Vec<Candidate> = eligible.into_iter().take(slots).collect();

        // AC4: acquire then spawn.
        for cand in selected {
            let session_id = Uuid::new_v4().to_string();
            let agent_ident = lease_agent_id(&self.host_id, &session_id);
            let now_utc = self.clock.now_utc();
            if let Err(e) =
                self.lease_ops
                    .acquire(&cand.doc_type, &cand.doc_id, &agent_ident, now_utc)
            {
                eprintln!(
                    "tick: lease acquire {}/{} failed: {}; skipping",
                    cand.doc_type, cand.doc_id, e
                );
                continue;
            }

            let branch = match render_branch_name(
                &orch.branch_template,
                &BranchVars {
                    iteration_id: cand.doc_id.clone(),
                    iteration_slug: String::new(),
                    agent_id: agent_ident.clone(),
                    story_id: cand.doc_id.clone(),
                    date: now_utc.date_naive().to_string(),
                },
            ) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("tick: branch render failed for {}: {}", cand.doc_id, e);
                    let _ = self
                        .lease_ops
                        .release(&cand.doc_type, &cand.doc_id, &agent_ident);
                    continue;
                }
            };

            let workspace = match self.workspace_provisioner.provision(
                &self.root,
                &orch.workspace_root,
                &orch.base_branch,
                &branch,
                &cand.doc_id,
            ) {
                Ok(ws) => ws,
                Err(e) => {
                    eprintln!(
                        "tick: workspace provision failed for {}: {}",
                        cand.doc_id, e
                    );
                    let _ = self
                        .lease_ops
                        .release(&cand.doc_type, &cand.doc_id, &agent_ident);
                    continue;
                }
            };

            let ctx = AgentContext {
                workspace: workspace.path,
                doc_id: cand.doc_id.clone(),
                agent_id: agent_ident.clone(),
                branch: workspace.branch,
            };
            let handle = match self.runner.spawn(ctx) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("tick: spawn {} failed: {}", cand.doc_id, e);
                    let _ = self
                        .lease_ops
                        .release(&cand.doc_type, &cand.doc_id, &agent_ident);
                    continue;
                }
            };

            self.running.insert(
                cand.doc_id.clone(),
                RunningAgent {
                    session_id,
                    doc_id: cand.doc_id,
                    doc_type: cand.doc_type,
                    agent_ident,
                    handle,
                    last_heartbeat: now_instant,
                },
            );
        }

        // AC1: pace ticks.
        self.clock
            .sleep(Duration::from_millis(orch.poll_interval_ms));
        Ok(())
    }

    pub fn run_until(&mut self, shutdown_rx: Receiver<()>) -> Result<()> {
        loop {
            match shutdown_rx.recv_timeout(Duration::ZERO) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if let Err(e) = self.run_once() {
                eprintln!("tick: run_once error: {}", e);
            }
        }
        // On shutdown: cancel + release every running agent.
        let agents: Vec<(String, RunningAgent)> = self.running.drain().collect();
        for (_, ra) in agents {
            let _ = ra.handle.cancel.send(());
            let _ = self
                .lease_ops
                .release(&ra.doc_type, &ra.doc_id, &ra.agent_ident);
        }
        Ok(())
    }

    fn reap_exited(&mut self, _claim_type: &str) {
        let mut exited: Vec<String> = Vec::new();
        for (doc_id, ra) in self.running.iter() {
            // Drain non-blockingly; flag if SubprocessExited seen. Other events
            // are silently discarded — Iter B owns event consumption.
            while let Ok(ev) = ra.handle.events.try_recv() {
                if matches!(ev, AgentEvent::SubprocessExited { .. }) {
                    exited.push(doc_id.clone());
                    break;
                }
            }
        }
        for doc_id in exited {
            if let Some(ra) = self.running.remove(&doc_id) {
                let _ = self
                    .lease_ops
                    .release(&ra.doc_type, &ra.doc_id, &ra.agent_ident);
            }
        }
    }

    fn load_candidates(&self, claim_type: &str) -> Result<Vec<Candidate>> {
        let store = Store::load(&self.root, &self.config)?;
        let mut out: Vec<Candidate> = Vec::new();
        for meta in store.all_docs() {
            if meta.doc_type.as_str() != claim_type {
                continue;
            }
            let created_at = meta
                .date
                .and_hms_opt(0, 0, 0)
                .map(|ndt| ndt.and_utc())
                .unwrap_or_else(Utc::now);
            out.push(Candidate {
                doc_id: meta.id.clone(),
                doc_type: meta.doc_type.as_str().to_string(),
                status: meta.status.to_string(),
                priority: None,
                created_at,
                assignees: meta.assignees.clone(),
            });
        }
        Ok(out)
    }

    fn local_active_lease_ids(&self, claim_type: &str) -> HashSet<String> {
        // Tick boundary: swallow enumeration errors so a transient git failure
        // doesn't abort the whole tick. Helper returns Result; we log + fall
        // back to an empty set, which means no eligibility filtering this tick
        // (safe — acquire's CAS is the real linearization point).
        match local_lease_ids(&self.git, &self.root, claim_type) {
            Ok(ids) => ids,
            Err(e) => {
                eprintln!("tick: list_refs leases failed: {}", e);
                HashSet::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::config::{Config, CoordinationConfig, OrchestrationConfig};
    use crate::engine::git_ref::test_support::MockGitRefClient;
    use crate::engine::runner::{AgentContext, AgentEvent, AgentHandle, AgentRunner};
    use chrono::TimeZone;
    use crossbeam_channel::{unbounded, Sender};
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    // ---- FakeClock ------------------------------------------------------

    struct FakeClock {
        now_instant: Mutex<Instant>,
        now_utc: Mutex<DateTime<Utc>>,
        sleeps: Mutex<Vec<Duration>>,
    }

    impl FakeClock {
        fn new() -> Self {
            Self {
                now_instant: Mutex::new(Instant::now()),
                now_utc: Mutex::new(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
                sleeps: Mutex::new(Vec::new()),
            }
        }
        fn advance(&self, d: Duration) {
            let mut g = self.now_instant.lock().unwrap();
            *g += d;
            let mut u = self.now_utc.lock().unwrap();
            *u += chrono::Duration::from_std(d).unwrap();
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

    // ---- FakeLeaseOps ---------------------------------------------------

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum LeaseCall {
        Acquire {
            type_name: String,
            doc_id: String,
            agent: String,
        },
        Heartbeat {
            type_name: String,
            doc_id: String,
            agent: String,
        },
        Release {
            type_name: String,
            doc_id: String,
            agent: String,
        },
    }

    struct FakeLeaseOps {
        calls: Mutex<Vec<LeaseCall>>,
        acquire_results: Mutex<Vec<Result<()>>>,
        heartbeat_results: Mutex<Vec<Result<()>>>,
    }

    impl FakeLeaseOps {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                acquire_results: Mutex::new(Vec::new()),
                heartbeat_results: Mutex::new(Vec::new()),
            }
        }
        fn calls(&self) -> Vec<LeaseCall> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl LeaseOps for Arc<FakeLeaseOps> {
        fn acquire(
            &self,
            type_name: &str,
            doc_id: &str,
            agent: &str,
            _now: DateTime<Utc>,
        ) -> Result<()> {
            self.calls.lock().unwrap().push(LeaseCall::Acquire {
                type_name: type_name.to_string(),
                doc_id: doc_id.to_string(),
                agent: agent.to_string(),
            });
            let mut q = self.acquire_results.lock().unwrap();
            if q.is_empty() {
                Ok(())
            } else {
                q.remove(0)
            }
        }
        fn heartbeat(
            &self,
            type_name: &str,
            doc_id: &str,
            agent: &str,
            _now: DateTime<Utc>,
        ) -> Result<()> {
            self.calls.lock().unwrap().push(LeaseCall::Heartbeat {
                type_name: type_name.to_string(),
                doc_id: doc_id.to_string(),
                agent: agent.to_string(),
            });
            let mut q = self.heartbeat_results.lock().unwrap();
            if q.is_empty() {
                Ok(())
            } else {
                q.remove(0)
            }
        }
        fn release(&self, type_name: &str, doc_id: &str, agent: &str) -> Result<()> {
            self.calls.lock().unwrap().push(LeaseCall::Release {
                type_name: type_name.to_string(),
                doc_id: doc_id.to_string(),
                agent: agent.to_string(),
            });
            Ok(())
        }
    }

    // ---- FakeRunner -----------------------------------------------------

    /// Records spawn calls. Returns a controllable AgentHandle.
    struct FakeRunner {
        calls: Mutex<Vec<AgentContext>>,
        next_pid: AtomicUsize,
        // Per-spawn channels we keep alive (so try_recv returns Empty, not Disconnected).
        event_senders: Mutex<Vec<Sender<AgentEvent>>>,
        cancel_receivers: Mutex<Vec<Receiver<()>>>,
    }

    impl FakeRunner {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                next_pid: AtomicUsize::new(1),
                event_senders: Mutex::new(Vec::new()),
                cancel_receivers: Mutex::new(Vec::new()),
            }
        }
        fn spawn_count(&self) -> usize {
            self.calls.lock().unwrap().len()
        }
    }

    impl AgentRunner for Arc<FakeRunner> {
        fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle> {
            self.calls.lock().unwrap().push(ctx);
            let (ev_tx, ev_rx) = unbounded::<AgentEvent>();
            let (cn_tx, cn_rx) = unbounded::<()>();
            self.event_senders.lock().unwrap().push(ev_tx);
            self.cancel_receivers.lock().unwrap().push(cn_rx);
            let pid = self.next_pid.fetch_add(1, Ordering::SeqCst) as u32;
            Ok(AgentHandle {
                pid,
                events: ev_rx,
                cancel: cn_tx,
            })
        }
    }

    // ---- Call-order recorder shared between fakes ------------------------

    #[derive(Default)]
    struct CallOrder {
        sequence: Mutex<Vec<String>>,
    }

    impl CallOrder {
        fn record(&self, label: &str) {
            self.sequence.lock().unwrap().push(label.to_string());
        }
        fn snapshot(&self) -> Vec<String> {
            self.sequence.lock().unwrap().clone()
        }
    }

    struct OrderRecordingLease {
        inner: Arc<FakeLeaseOps>,
        order: Arc<CallOrder>,
    }

    impl LeaseOps for OrderRecordingLease {
        fn acquire(
            &self,
            type_name: &str,
            doc_id: &str,
            agent: &str,
            now: DateTime<Utc>,
        ) -> Result<()> {
            self.order.record(&format!("acquire:{}", doc_id));
            self.inner.acquire(type_name, doc_id, agent, now)
        }
        fn heartbeat(
            &self,
            type_name: &str,
            doc_id: &str,
            agent: &str,
            now: DateTime<Utc>,
        ) -> Result<()> {
            self.inner.heartbeat(type_name, doc_id, agent, now)
        }
        fn release(&self, type_name: &str, doc_id: &str, agent: &str) -> Result<()> {
            self.inner.release(type_name, doc_id, agent)
        }
    }

    struct OrderRecordingRunner {
        inner: Arc<FakeRunner>,
        order: Arc<CallOrder>,
    }

    impl AgentRunner for OrderRecordingRunner {
        fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle> {
            self.order.record(&format!("spawn:{}", ctx.doc_id));
            self.inner.spawn(ctx)
        }
    }

    // ---- helpers --------------------------------------------------------

    fn base_orch(active_statuses: Vec<&str>) -> OrchestrationConfig {
        OrchestrationConfig {
            agent_users: vec!["claude-bot".into()],
            claim_type: "story".into(),
            branch_template: "agents/{{ story_id }}".into(),
            workspace_root: PathBuf::from(".lazyspec/work"),
            base_branch: "main".into(),
            runtime: Default::default(),
            hooks: Default::default(),
            poll_interval_ms: 1_000,
            max_concurrent_agents: 4,
            active_statuses: active_statuses.into_iter().map(String::from).collect(),
            heartbeat_interval_ms: 5_000,
            metadata_push_interval_ms: 10_000,
        }
    }

    fn base_coord() -> CoordinationConfig {
        CoordinationConfig {
            remote: "origin".into(),
            lease_duration: "60m".into(),
            grace_period: "2m".into(),
            max_push_retries: 5,
            max_clock_skew: "5m".into(),
        }
    }

    fn cfg(orch: OrchestrationConfig) -> Config {
        Config {
            orchestration: Some(orch),
            coordination: Some(base_coord()),
            ..Config::default()
        }
    }

    /// Build a TickLoop with fakes injected. Note the engine's Status enum
    /// doesn't have "todo", so tests use "draft" (a real Status value) and
    /// configure active_statuses to match the Display form.
    fn make_stories_status(td: &TempDir, n: usize, status: &str) {
        let dir = td.path().join("docs/stories");
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..n {
            let content = format!(
                "---\n\
title: \"S{i}\"\n\
type: story\n\
status: {status}\n\
author: test\n\
date: 2026-01-01\n\
tags: []\n\
assignees: [\"claude-bot\"]\n\
---\n\
body\n",
                i = i,
                status = status
            );
            std::fs::write(dir.join(format!("STORY-{:03}-s.md", i)), content).unwrap();
        }
    }

    struct FakeProvisioner;
    impl WorkspaceProvisioner for FakeProvisioner {
        fn provision(
            &self,
            _r: &std::path::Path,
            _ws: &std::path::Path,
            _bb: &str,
            branch: &str,
            claim: &str,
        ) -> Result<Workspace> {
            Ok(Workspace {
                path: PathBuf::from(format!("/tmp/fake-ws/{}", claim)),
                branch: branch.to_string(),
            })
        }
    }

    fn build_loop(
        td: &TempDir,
        cfg: Config,
        runner: Arc<FakeRunner>,
        git: MockGitRefClient,
        lease: Arc<FakeLeaseOps>,
        clock: FakeClock,
    ) -> TickLoop<Arc<FakeRunner>, MockGitRefClient, Arc<FakeLeaseOps>, FakeClock, FakeProvisioner>
    {
        TickLoop::new(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            runner,
            git,
            lease,
            clock,
            FakeProvisioner,
        )
    }

    // ===========================================================
    // AC1 — polling cadence
    // ===========================================================

    #[test]
    fn run_once_invokes_clock_sleep_with_poll_interval() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.poll_interval_ms = 1234;
        let cfg = cfg(orch);
        let clock = FakeClock::new();
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            clock,
        );
        t.run_once().unwrap();
        let durs = t.clock.sleep_durations();
        assert_eq!(durs.len(), 1);
        assert_eq!(durs[0], Duration::from_millis(1234));
    }

    #[test]
    fn run_until_fires_n_ticks_in_window() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.poll_interval_ms = 30_000;
        let cfg = cfg(orch);
        let clock = FakeClock::new();
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            clock,
        );
        // Tick twice manually to mirror the "60s window / 30s interval" intent
        // without a real timer.
        t.run_once().unwrap();
        t.run_once().unwrap();
        let durs = t.clock.sleep_durations();
        assert_eq!(durs.len(), 2);
        assert!(durs.iter().all(|d| *d == Duration::from_millis(30_000)));
    }

    // ===========================================================
    // AC3 — concurrency cap
    // ===========================================================

    #[test]
    fn dispatch_takes_at_most_slots_available_candidates() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.max_concurrent_agents = 3;
        let cfg = cfg(orch);
        make_stories_status(&td, 5, "draft");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        );
        t.run_once().unwrap();
        assert_eq!(runner.spawn_count(), 3);
    }

    // ===========================================================
    // AC4 — CAS acquire before spawn
    // ===========================================================

    #[test]
    fn cas_failure_skips_spawn() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
        make_stories_status(&td, 1, "draft");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        lease
            .acquire_results
            .lock()
            .unwrap()
            .push(Err(anyhow::anyhow!("CAS rejected")));
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        );
        t.run_once().unwrap();
        assert_eq!(runner.spawn_count(), 0);
        // acquire was attempted, but no spawn followed.
        let kinds: Vec<_> = lease.calls();
        assert!(kinds.iter().any(|c| matches!(c, LeaseCall::Acquire { .. })));
    }

    #[test]
    fn lease_acquired_before_spawn() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
        make_stories_status(&td, 1, "draft");
        let runner_inner = Arc::new(FakeRunner::new());
        let lease_inner = Arc::new(FakeLeaseOps::new());
        let order = Arc::new(CallOrder::default());

        let recording_lease = OrderRecordingLease {
            inner: Arc::clone(&lease_inner),
            order: Arc::clone(&order),
        };
        let recording_runner = OrderRecordingRunner {
            inner: Arc::clone(&runner_inner),
            order: Arc::clone(&order),
        };

        let mut t = TickLoop::new(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            recording_runner,
            MockGitRefClient::new(),
            recording_lease,
            FakeClock::new(),
            FakeProvisioner,
        );
        t.run_once().unwrap();

        let seq = order.snapshot();
        let acq_idx = seq.iter().position(|s| s.starts_with("acquire:")).unwrap();
        let sp_idx = seq.iter().position(|s| s.starts_with("spawn:")).unwrap();
        assert!(
            acq_idx < sp_idx,
            "acquire must precede spawn, got {:?}",
            seq
        );
    }

    // ===========================================================
    // AC5 — heartbeat cadence
    // ===========================================================

    fn insert_fake_running(
        t: &mut TickLoop<
            Arc<FakeRunner>,
            MockGitRefClient,
            Arc<FakeLeaseOps>,
            FakeClock,
            FakeProvisioner,
        >,
        doc_id: &str,
        agent_ident: &str,
        last_hb: Instant,
    ) {
        let (_ev_tx, ev_rx) = unbounded::<AgentEvent>();
        let (cn_tx, _cn_rx) = unbounded::<()>();
        // Keep ev_tx alive by leaking — it just needs to stay open for the
        // duration of the test so try_recv returns Empty rather than
        // Disconnected. Box::leak is acceptable in a #[cfg(test)] helper.
        Box::leak(Box::new(_ev_tx));
        Box::leak(Box::new(_cn_rx));
        t.running.insert(
            doc_id.to_string(),
            RunningAgent {
                session_id: "sess".to_string(),
                doc_id: doc_id.to_string(),
                doc_type: "story".to_string(),
                agent_ident: agent_ident.to_string(),
                handle: AgentHandle {
                    pid: 1,
                    events: ev_rx,
                    cancel: cn_tx,
                },
                last_heartbeat: last_hb,
            },
        );
    }

    #[test]
    fn heartbeat_fires_when_interval_elapsed() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.heartbeat_interval_ms = 5_000;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let clock = FakeClock::new();
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            clock,
        );
        let old = t.clock.now_instant() - Duration::from_millis(6_000);
        insert_fake_running(&mut t, "S-1", "host-test:sess-1", old);

        t.run_once().unwrap();

        let hb_count = lease
            .calls()
            .iter()
            .filter(|c| matches!(c, LeaseCall::Heartbeat { .. }))
            .count();
        assert_eq!(hb_count, 1);
    }

    #[test]
    fn heartbeat_not_fired_before_interval() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.heartbeat_interval_ms = 5_000;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        );
        // Fresh agent: last_heartbeat == now.
        let now = t.clock.now_instant();
        insert_fake_running(&mut t, "S-1", "host-test:sess-1", now);

        t.run_once().unwrap();

        let hb_count = lease
            .calls()
            .iter()
            .filter(|c| matches!(c, LeaseCall::Heartbeat { .. }))
            .count();
        assert_eq!(hb_count, 0);
    }

    #[test]
    fn heartbeat_is_daemon_side() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.heartbeat_interval_ms = 5_000;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        );
        let stored_ident = "host-test:abcd-uuid-1234".to_string();
        let old = t.clock.now_instant() - Duration::from_millis(10_000);
        insert_fake_running(&mut t, "S-1", &stored_ident, old);

        t.run_once().unwrap();

        let hb = lease
            .calls()
            .into_iter()
            .find(|c| matches!(c, LeaseCall::Heartbeat { .. }))
            .expect("expected heartbeat call");
        match hb {
            LeaseCall::Heartbeat { agent, .. } => assert_eq!(agent, stored_ident),
            _ => unreachable!(),
        }
    }

    // ===========================================================
    // AC6 — lease agent identifier shape at tick level
    // ===========================================================

    #[test]
    fn dispatch_uses_host_colon_session_for_lease_agent() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
        make_stories_status(&td, 1, "draft");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        );
        t.run_once().unwrap();

        let re = regex::Regex::new(r"^host-test:[0-9a-f-]{36}$").unwrap();
        let acquire = lease
            .calls()
            .into_iter()
            .find(|c| matches!(c, LeaseCall::Acquire { .. }))
            .expect("expected acquire call");
        match acquire {
            LeaseCall::Acquire { agent, .. } => {
                assert!(re.is_match(&agent), "agent ident shape mismatch: {}", agent);
            }
            _ => unreachable!(),
        }
    }

    // ===========================================================
    // AC7 — batched lease fetch
    // ===========================================================

    /// Counts fetch_refs invocations against a configurable always-Ok response.
    struct CountingGit {
        fetches: RefCell<Vec<String>>,
        lists: RefCell<Vec<String>>,
    }

    impl CountingGit {
        fn new() -> Self {
            Self {
                fetches: RefCell::new(Vec::new()),
                lists: RefCell::new(Vec::new()),
            }
        }
        fn fetch_count(&self) -> usize {
            self.fetches.borrow().len()
        }
        fn fetch_patterns(&self) -> Vec<String> {
            self.fetches.borrow().clone()
        }
    }

    impl GitRefOps for CountingGit {
        fn resolve_ref(&self, _r: &std::path::Path, _n: &str) -> Result<Option<String>> {
            Ok(None)
        }
        fn list_refs(&self, _r: &std::path::Path, p: &str) -> Result<Vec<(String, String)>> {
            self.lists.borrow_mut().push(p.to_string());
            Ok(vec![])
        }
        fn read_ref_blob(&self, _r: &std::path::Path, _s: &str, _p: &str) -> Result<String> {
            Ok(String::new())
        }
        fn create_commit(
            &self,
            _r: &std::path::Path,
            _n: &str,
            _f: &[(&str, &str)],
            _p: Option<&str>,
        ) -> Result<String> {
            Ok("sha".into())
        }
        fn create_ref_commit(
            &self,
            _r: &std::path::Path,
            _n: &str,
            _f: &[(&str, &str)],
        ) -> Result<String> {
            Ok("sha".into())
        }
        fn update_ref(&self, _r: &std::path::Path, _n: &str, _ns: &str, _os: &str) -> Result<()> {
            Ok(())
        }
        fn delete_ref(&self, _r: &std::path::Path, _n: &str) -> Result<()> {
            Ok(())
        }
        fn fetch_refs(&self, _r: &std::path::Path, _remote: &str, pattern: &str) -> Result<()> {
            self.fetches.borrow_mut().push(pattern.to_string());
            Ok(())
        }
        fn push_ref(&self, _r: &std::path::Path, _rem: &str, _n: &str) -> Result<()> {
            Ok(())
        }
        fn delete_remote_ref(
            &self,
            _r: &std::path::Path,
            _rem: &str,
            _n: &str,
            _eo: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }
        fn push_ref_with_lease(
            &self,
            _r: &std::path::Path,
            _rem: &str,
            _n: &str,
            _ns: &str,
            _eo: Option<&str>,
        ) -> Result<()> {
            Ok(())
        }
        fn read_commit_timestamp(&self, _r: &std::path::Path, _s: &str) -> Result<DateTime<Utc>> {
            Ok(Utc::now())
        }
    }

    #[test]
    fn fetch_not_called_per_tick_within_window() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.poll_interval_ms = 1_000;
        orch.metadata_push_interval_ms = 10_000;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let git = CountingGit::new();
        let mut t = TickLoop::new(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            Arc::clone(&runner),
            git,
            Arc::clone(&lease),
            FakeClock::new(),
            FakeProvisioner,
        );
        for _ in 0..5 {
            t.run_once().unwrap();
        }
        // First tick fetches. Subsequent 4 ticks are within the 10s window so
        // do not fetch. One fetch per configured type — default config has
        // several types but fetch_count counts every fetch_refs call; assert
        // that the *number of fetch batches* (ticks containing fetches) is 1
        // by inspecting that the first batch's set repeats only once.
        // Simpler: count distinct ticks. With N types per fetch tick the batch
        // appears as N consecutive fetches followed by zero across 4 ticks.
        // Total fetch_count == N. We get N from config.documents.types.len().
        let n_types = t.config.documents.types.len();
        assert_eq!(t.git.fetch_count(), n_types);
    }

    #[test]
    fn fetch_called_again_after_metadata_push_interval() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.poll_interval_ms = 1_000;
        orch.metadata_push_interval_ms = 10_000;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let git = CountingGit::new();
        let mut t = TickLoop::new(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            Arc::clone(&runner),
            git,
            Arc::clone(&lease),
            FakeClock::new(),
            FakeProvisioner,
        );
        t.run_once().unwrap();
        let n_types = t.config.documents.types.len();
        assert_eq!(t.git.fetch_count(), n_types);
        // Advance past the window.
        t.clock.advance(Duration::from_millis(11_000));
        t.run_once().unwrap();
        assert_eq!(t.git.fetch_count(), n_types * 2);
    }

    #[test]
    fn fetch_covers_all_configured_type_globs() {
        let td = TempDir::new().unwrap();
        let orch = base_orch(vec!["draft"]);
        let mut cfg = cfg(orch);
        // Strip types down to a known set so we can assert exactly.
        cfg.documents.types = vec![
            crate::engine::config::TypeDef::test_fixture(
                "story",
                crate::engine::config::StoreBackend::Filesystem,
            ),
            crate::engine::config::TypeDef::test_fixture(
                "iteration",
                crate::engine::config::StoreBackend::Filesystem,
            ),
        ];
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let git = CountingGit::new();
        let mut t = TickLoop::new(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            Arc::clone(&runner),
            git,
            Arc::clone(&lease),
            FakeClock::new(),
            FakeProvisioner,
        );
        t.run_once().unwrap();
        let patterns = t.git.fetch_patterns();
        assert!(patterns.iter().any(|p| p == "refs/lazyspec/leases/story/*"));
        assert!(patterns
            .iter()
            .any(|p| p == "refs/lazyspec/leases/iteration/*"));
        assert_eq!(patterns.len(), 2);
    }

    #[test]
    fn eligibility_path_uses_local_only_reads() {
        // Run two ticks, where the second is within the metadata-push window.
        // No fetch should occur on tick 2, but list_refs should still run.
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.poll_interval_ms = 1_000;
        orch.metadata_push_interval_ms = 100_000;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let git = CountingGit::new();
        let mut t = TickLoop::new(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            Arc::clone(&runner),
            git,
            Arc::clone(&lease),
            FakeClock::new(),
            FakeProvisioner,
        );
        t.run_once().unwrap();
        let after_first = t.git.fetch_count();
        let lists_after_first = t.git.lists.borrow().len();
        t.run_once().unwrap();
        // No new fetches in tick 2.
        assert_eq!(t.git.fetch_count(), after_first);
        // But list_refs ran again on tick 2 (eligibility path).
        assert!(t.git.lists.borrow().len() > lists_after_first);
    }
}
