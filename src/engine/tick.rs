//! Orchestrator tick loop. Heartbeat of the daemon: polls eligible documents,
//! acquires RFC-035 leases, dispatches agents, heartbeats live leases.
//!
//! Iter A scope (AC1-7): dispatch + lease acquire/heartbeat. Reconciliation,
//! retry, stall detection (AC8-14) and boot recovery / preflight (AC15-16) live
//! in later iterations.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
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
use super::preflight::{run_preflight, PreflightChecks, PreflightReport, PreflightWatcher};
use super::runner::{AgentContext, AgentEvent, AgentHandle, AgentRunner};
use super::store::Store;
use super::workspace::{provision_workspace, remove as remove_workspace, Workspace};

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

    /// Tear down a worktree previously created by `provision`. Used by status
    /// reconcile when a doc transitions to a terminal state.
    fn remove(&self, repo_root: &std::path::Path, workspace_path: &std::path::Path) -> Result<()>;
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

    fn remove(&self, repo_root: &std::path::Path, workspace_path: &std::path::Path) -> Result<()> {
        remove_workspace(repo_root, workspace_path)
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

/// Per-agent state mutated by the reader thread and read by the tick
/// reconcile pass. `exit` distinguishes "still running" (`None`) from "exited"
/// (`Some(code)`); the outer `Option` is set when the reader observes
/// `SubprocessExited`, the inner mirrors the process exit code.
pub struct AgentObservation {
    pub last_event_at: Instant,
    pub tool_use_in_flight: bool,
    pub turn_started_at: Instant,
    pub attempt: u32,
    pub failure_attempt: u32,
    pub exit: Option<Option<i32>>,
}

impl AgentObservation {
    pub fn new(now: Instant) -> Self {
        Self {
            last_event_at: now,
            tool_use_in_flight: false,
            turn_started_at: now,
            attempt: 1,
            failure_attempt: 0,
            exit: None,
        }
    }
}

pub struct RunningAgent {
    pub session_id: String,
    pub doc_id: String,
    pub doc_type: String,
    pub agent_ident: String,
    pub workspace: PathBuf,
    pub branch: String,
    pub cancel: crossbeam_channel::Sender<()>,
    pub pid: u32,
    pub last_heartbeat: Instant,
    pub observation: Arc<Mutex<AgentObservation>>,
    pub reader_handle: Option<JoinHandle<()>>,
}

/// Why an agent is queued for retry. T3 emits `Stall`, T4 adds `TurnTimeout`,
/// T6 adds the remaining variants and wires the drain logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryReason {
    Stall,
    TurnTimeout,
    CleanExit,
    AbnormalExit,
    HookFailure,
}

/// A killed agent waiting to be re-dispatched. T6 owns the drain pass.
#[derive(Debug, Clone)]
pub struct PendingRetry {
    pub doc_id: String,
    pub doc_type: String,
    pub workspace: PathBuf,
    pub branch: String,
    pub agent_ident: String,
    pub session_id: String,
    pub attempt: u32,
    pub failure_attempt: u32,
    pub ready_at: Instant,
    pub kind: RetryReason,
}

/// Sink for failure-cap events emitted by the tick loop. Slice 6 wires a real
/// IPC implementation; tests use a recording fake; production uses an
/// eprintln-based default until the IPC sink lands.
pub trait AgentEventSink: Send + Sync {
    fn emit_failed(&self, doc_id: &str, agent_ident: &str, reason: &str);
}

pub struct NullEventSink;

impl AgentEventSink for NullEventSink {
    fn emit_failed(&self, _: &str, _: &str, _: &str) {}
}

pub struct EprintlnEventSink;

impl AgentEventSink for EprintlnEventSink {
    fn emit_failed(&self, doc_id: &str, agent_ident: &str, reason: &str) {
        eprintln!(
            "agent failed: doc={} agent={} reason={}",
            doc_id, agent_ident, reason
        );
    }
}

/// Drain `events` into `observation`. Exits when the channel closes (which the
/// runner does after `SubprocessExited` is sent). Uses wall clock for
/// `last_event_at` / `turn_started_at` — reconcile reads via the abstract
/// `Clock`, so determinism lives at the comparison site, not here.
pub fn run_event_reader(events: Receiver<AgentEvent>, observation: Arc<Mutex<AgentObservation>>) {
    while let Ok(ev) = events.recv() {
        let mut obs = observation.lock().unwrap();
        obs.last_event_at = Instant::now();
        match ev {
            AgentEvent::ToolCallStarted { .. } => {
                obs.tool_use_in_flight = true;
            }
            AgentEvent::ToolCall { .. } => {
                obs.tool_use_in_flight = false;
            }
            AgentEvent::TurnCompleted { .. } => {
                obs.turn_started_at = obs.last_event_at;
                obs.tool_use_in_flight = false;
            }
            AgentEvent::SubprocessExited { code } => {
                obs.exit = Some(code);
                break;
            }
            AgentEvent::SessionStarted | AgentEvent::Text { .. } => {}
        }
    }
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
    pub event_sink: Box<dyn AgentEventSink>,
    pub running: HashMap<String, RunningAgent>,
    pub last_metadata_push: Option<Instant>,
    pub retry_queue: Vec<PendingRetry>,
    /// Latest preflight result. Dispatch is gated on `is_ok()`.
    pub preflight: PreflightReport,
    /// Notify-driven invalidator for config + prompt edits. `None` in tests
    /// (or production until slice 6 wires it through `Daemon::run`).
    pub preflight_watcher: Option<Box<dyn PreflightWatcher>>,
    /// Set when the watcher observes an event; cleared after `run_preflight`
    /// re-runs at the top of the next tick.
    pub preflight_dirty: bool,
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
        Self::with_event_sink(
            root,
            config,
            host_id,
            runner,
            git,
            lease_ops,
            clock,
            workspace_provisioner,
            Box::new(NullEventSink),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_event_sink(
        root: PathBuf,
        config: Config,
        host_id: String,
        runner: R,
        git: G,
        lease_ops: L,
        clock: C,
        workspace_provisioner: W,
        event_sink: Box<dyn AgentEventSink>,
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
            event_sink,
            running: HashMap::new(),
            last_metadata_push: None,
            retry_queue: Vec::new(),
            preflight: PreflightReport::all_ok(),
            preflight_watcher: None,
            preflight_dirty: false,
        }
    }

    /// Inject the initial preflight report + an optional watcher. Daemon
    /// production wiring calls this once after `run_preflight` at startup.
    /// Tests use it to seed a failing report or a fake watcher.
    pub fn with_preflight(
        mut self,
        preflight: PreflightReport,
        watcher: Option<Box<dyn PreflightWatcher>>,
    ) -> Self {
        self.preflight = preflight;
        self.preflight_watcher = watcher;
        self.preflight_dirty = false;
        self
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

        // AC16: drain the preflight watcher channel. Any event flips the dirty
        // flag; the actual `run_preflight` re-run is below so a dirty flag set
        // outside this loop (e.g. by a test) also triggers a re-run.
        if let Some(w) = self.preflight_watcher.as_ref() {
            if w.poll() {
                self.preflight_dirty = true;
            }
        }
        if self.preflight_dirty {
            match run_preflight(&PreflightChecks {
                root: &self.root,
                config: &self.config,
            }) {
                Ok(report) => {
                    self.preflight = report;
                }
                Err(e) => {
                    eprintln!(
                        "tick: preflight re-run failed: {}; keeping previous report",
                        e
                    );
                }
            }
            self.preflight_dirty = false;
        }

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

        // AC10/AC11: doc-status reconcile. Runs BEFORE exit classification so
        // a doc that transitioned to terminal/handoff in the same tick is
        // culled by the terminal kill path (no retry) rather than appearing as
        // a clean exit continuation. Daemon does NOT mutate doc status
        // (RFC-041 invariant); we only react to status set externally.
        self.reconcile_doc_status(&orch.active_statuses, &orch.handoff_states);

        // AC12/AC13: classify exited agents into the retry queue. Clean exits
        // continue; non-zero/signalled exits become failures.
        self.reap_exited(&orch.claim_type, now_instant);

        // AC8/AC9: stall + turn-timeout detection. Single classification pass
        // with precedence: TurnTimeout wins over Stall (turn timeout is the
        // hard wall, NOT suspended by tool_use_in_flight). One kill per agent
        // per tick. Collect into a kill list, then apply — `kill_agent_for_retry`
        // mutates `self.running`, so we can't act while iterating.
        let stall_timeout = Duration::from_millis(orch.stall_timeout_ms);
        let turn_timeout = Duration::from_millis(orch.runtime.turn_timeout_ms);
        let kills: Vec<(String, RetryReason)> = self
            .running
            .iter()
            .filter_map(|(doc_id, ra)| {
                let obs = ra.observation.lock().unwrap();
                let turn_elapsed = now_instant.duration_since(obs.turn_started_at);
                if turn_elapsed >= turn_timeout {
                    return Some((doc_id.clone(), RetryReason::TurnTimeout));
                }
                if obs.tool_use_in_flight {
                    return None;
                }
                let idle = now_instant.duration_since(obs.last_event_at);
                if idle >= stall_timeout {
                    Some((doc_id.clone(), RetryReason::Stall))
                } else {
                    None
                }
            })
            .collect();
        for (doc_id, reason) in kills {
            self.kill_agent_for_retry(&doc_id, reason, now_instant);
        }

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

        // AC12/AC13: drain retry queue entries that are ready. Runs BEFORE
        // fresh dispatch so retried agents take precedence over new candidates
        // when slots are scarce.
        self.drain_retry_queue(now_instant);

        // AC16: gate NEW dispatch on preflight. In-flight agents (heartbeat,
        // reconcile, exit classification, retry drain above) keep running —
        // hot-reload applies to future dispatches, not active sessions.
        let selected: Vec<Candidate> = if self.preflight.is_ok() {
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
            eligible.into_iter().take(slots).collect()
        } else {
            Vec::new()
        };

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

            let workspace_path = workspace.path.clone();
            let workspace_branch = workspace.branch.clone();
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

            let AgentHandle {
                pid,
                events,
                cancel,
            } = handle;
            let observation = Arc::new(Mutex::new(AgentObservation::new(now_instant)));
            let reader_obs = Arc::clone(&observation);
            let reader_handle = std::thread::spawn(move || {
                run_event_reader(events, reader_obs);
            });

            self.running.insert(
                cand.doc_id.clone(),
                RunningAgent {
                    session_id,
                    doc_id: cand.doc_id,
                    doc_type: cand.doc_type,
                    agent_ident,
                    workspace: workspace_path,
                    branch: workspace_branch,
                    cancel,
                    pid,
                    last_heartbeat: now_instant,
                    observation,
                    reader_handle: Some(reader_handle),
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
        // On shutdown: cancel + release every running agent. Join the reader
        // thread so the channel drains and no orphan threads outlive the loop.
        let agents: Vec<(String, RunningAgent)> = self.running.drain().collect();
        for (_, mut ra) in agents {
            let _ = ra.cancel.send(());
            if let Some(rh) = ra.reader_handle.take() {
                let _ = rh.join();
            }
            let _ = self
                .lease_ops
                .release(&ra.doc_type, &ra.doc_id, &ra.agent_ident);
        }
        Ok(())
    }

    fn kill_agent_for_retry(&mut self, doc_id: &str, kind: RetryReason, now: Instant) {
        let Some(mut ra) = self.running.remove(doc_id) else {
            return;
        };
        let _ = ra.cancel.send(());
        if let Some(rh) = ra.reader_handle.take() {
            let _ = rh.join();
        }
        let _ = self
            .lease_ops
            .release(&ra.doc_type, &ra.doc_id, &ra.agent_ident);

        let orch = match self.config.orchestration.as_ref() {
            Some(o) => o,
            None => return,
        };

        let (attempt, failure_attempt, ready_at, capped) = {
            let obs = ra.observation.lock().unwrap();
            match kind {
                RetryReason::Stall
                | RetryReason::TurnTimeout
                | RetryReason::AbnormalExit
                | RetryReason::HookFailure => {
                    let n = obs.failure_attempt + 1;
                    if n > orch.max_failure_attempts {
                        (obs.attempt, n, None, Some("max_failure_attempts"))
                    } else {
                        let shift = (n - 1).min(63);
                        let exp = 10_000_u64.saturating_mul(1u64 << shift);
                        let delay_ms = std::cmp::min(exp, orch.max_retry_backoff_ms);
                        (
                            obs.attempt,
                            n,
                            Some(now + Duration::from_millis(delay_ms)),
                            None,
                        )
                    }
                }
                RetryReason::CleanExit => {
                    let new_attempt = obs.attempt + 1;
                    if new_attempt > orch.max_turns {
                        (new_attempt, obs.failure_attempt, None, Some("max_turns"))
                    } else {
                        (
                            new_attempt,
                            obs.failure_attempt,
                            Some(now + Duration::from_millis(orch.continuation_delay_ms)),
                            None,
                        )
                    }
                }
            }
        };

        if let Some(reason) = capped {
            self.event_sink
                .emit_failed(&ra.doc_id, &ra.agent_ident, reason);
            return;
        }
        let ready_at = ready_at.expect("non-capped retry must have ready_at");
        self.retry_queue.push(PendingRetry {
            doc_id: ra.doc_id,
            doc_type: ra.doc_type,
            workspace: ra.workspace,
            branch: ra.branch,
            agent_ident: ra.agent_ident,
            session_id: ra.session_id,
            attempt,
            failure_attempt,
            ready_at,
            kind,
        });
    }

    /// AC10/AC11. Re-read each running agent's doc status. If the status is no
    /// longer "active", kill + release. Handoff states keep the workspace for
    /// later operator resumption; everything else (including a missing doc)
    /// removes it. No retry is enqueued either way — status reconcile is
    /// terminal.
    fn reconcile_doc_status(&mut self, active_statuses: &[String], handoff_states: &[String]) {
        let store = match Store::load(&self.root, &self.config) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("tick: reconcile store load failed: {}; skipping", e);
                return;
            }
        };
        let status_by_id: HashMap<String, String> = store
            .all_docs()
            .into_iter()
            .map(|m| (m.id.clone(), m.status.to_string()))
            .collect();

        let actions: Vec<(String, bool)> = self
            .running
            .keys()
            .filter_map(|doc_id| match status_by_id.get(doc_id) {
                // Missing from store: leave alone. Daemon does not infer
                // intent from absence — could be a transient load error or a
                // shorthand id mismatch. Status reconcile only fires when we
                // can observe a definite non-active status.
                None => None,
                Some(s) if active_statuses.iter().any(|a| a == s) => None,
                Some(s) if handoff_states.iter().any(|h| h == s) => Some((doc_id.clone(), false)),
                // Definite non-active, non-handoff status: terminal.
                Some(_) => Some((doc_id.clone(), true)),
            })
            .collect();

        for (doc_id, remove_workspace) in actions {
            self.kill_agent_terminal(&doc_id, remove_workspace);
        }
    }

    /// Terminal kill — cancel + join + release, optionally remove workspace.
    /// Sibling to `kill_agent_for_retry`; this path does NOT enqueue a retry.
    fn kill_agent_terminal(&mut self, doc_id: &str, remove_workspace: bool) {
        let Some(mut ra) = self.running.remove(doc_id) else {
            return;
        };
        let _ = ra.cancel.send(());
        if let Some(rh) = ra.reader_handle.take() {
            let _ = rh.join();
        }
        let _ = self
            .lease_ops
            .release(&ra.doc_type, &ra.doc_id, &ra.agent_ident);
        if remove_workspace {
            if let Err(e) = self.workspace_provisioner.remove(&self.root, &ra.workspace) {
                eprintln!(
                    "tick: workspace remove for {} at {} failed: {}",
                    ra.doc_id,
                    ra.workspace.display(),
                    e
                );
            }
        }
    }

    /// AC12/AC13: drain ready retry queue entries. For each entry whose
    /// `ready_at <= now`, re-acquire the lease (CAS-against-zeros) with the
    /// same agent_ident, then re-spawn in the SAME workspace (worktree was
    /// left intact at kill time). CAS failure → emit failed, abandon (NOT
    /// re-enqueued). Successful re-spawn carries `attempt`/`failure_attempt`
    /// forward and starts a fresh `turn_started_at`.
    fn drain_retry_queue(&mut self, now: Instant) {
        let mut remaining = Vec::with_capacity(self.retry_queue.len());
        let drained: Vec<PendingRetry> = std::mem::take(&mut self.retry_queue);
        let now_utc = self.clock.now_utc();
        for retry in drained {
            if retry.ready_at > now {
                remaining.push(retry);
                continue;
            }
            if let Err(e) =
                self.lease_ops
                    .acquire(&retry.doc_type, &retry.doc_id, &retry.agent_ident, now_utc)
            {
                eprintln!(
                    "tick: retry lease re-acquire {}/{} failed: {}; abandoning",
                    retry.doc_type, retry.doc_id, e
                );
                self.event_sink
                    .emit_failed(&retry.doc_id, &retry.agent_ident, "lease_cas_failed");
                continue;
            }
            let ctx = AgentContext {
                workspace: retry.workspace.clone(),
                doc_id: retry.doc_id.clone(),
                agent_id: retry.agent_ident.clone(),
                branch: retry.branch.clone(),
            };
            let handle = match self.runner.spawn(ctx) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("tick: retry spawn {} failed: {}", retry.doc_id, e);
                    let _ =
                        self.lease_ops
                            .release(&retry.doc_type, &retry.doc_id, &retry.agent_ident);
                    self.event_sink.emit_failed(
                        &retry.doc_id,
                        &retry.agent_ident,
                        "respawn_failed",
                    );
                    continue;
                }
            };
            let AgentHandle {
                pid,
                events,
                cancel,
            } = handle;
            let mut obs = AgentObservation::new(now);
            obs.attempt = retry.attempt;
            obs.failure_attempt = retry.failure_attempt;
            let observation = Arc::new(Mutex::new(obs));
            let reader_obs = Arc::clone(&observation);
            let reader_handle = std::thread::spawn(move || {
                run_event_reader(events, reader_obs);
            });
            self.running.insert(
                retry.doc_id.clone(),
                RunningAgent {
                    session_id: retry.session_id,
                    doc_id: retry.doc_id,
                    doc_type: retry.doc_type,
                    agent_ident: retry.agent_ident,
                    workspace: retry.workspace,
                    branch: retry.branch,
                    cancel,
                    pid,
                    last_heartbeat: now,
                    observation,
                    reader_handle: Some(reader_handle),
                },
            );
        }
        self.retry_queue = remaining;
    }

    /// Classify subprocess exits and route into the retry queue. Clean exits
    /// (code 0) become `CleanExit` continuations; non-zero or signalled exits
    /// become `AbnormalExit` failures. Status reconcile runs BEFORE this so a
    /// doc that already transitioned to terminal/handoff in the same tick is
    /// culled by the terminal kill path and never reaches classification.
    fn reap_exited(&mut self, _claim_type: &str, now: Instant) {
        let exited: Vec<(String, Option<i32>)> = self
            .running
            .iter()
            .filter_map(|(doc_id, ra)| {
                let obs = ra.observation.lock().unwrap();
                obs.exit.map(|code| (doc_id.clone(), code))
            })
            .collect();
        for (doc_id, code) in exited {
            let kind = match code {
                Some(0) => RetryReason::CleanExit,
                _ => RetryReason::AbnormalExit,
            };
            self.kill_agent_for_retry(&doc_id, kind, now);
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
            stall_timeout_ms: 300_000,
            max_turns: 20,
            max_failure_attempts: 5,
            max_retry_backoff_ms: 300_000,
            handoff_states: vec!["in-review".to_string()],
            continuation_delay_ms: 1_000,
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

    #[derive(Default)]
    struct FakeProvisioner {
        remove_calls: Mutex<Vec<PathBuf>>,
    }

    impl FakeProvisioner {
        fn remove_calls(&self) -> Vec<PathBuf> {
            self.remove_calls.lock().unwrap().clone()
        }
    }

    impl WorkspaceProvisioner for Arc<FakeProvisioner> {
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

        fn remove(&self, _repo: &std::path::Path, workspace_path: &std::path::Path) -> Result<()> {
            self.remove_calls
                .lock()
                .unwrap()
                .push(workspace_path.to_path_buf());
            Ok(())
        }
    }

    fn build_loop(
        td: &TempDir,
        cfg: Config,
        runner: Arc<FakeRunner>,
        git: MockGitRefClient,
        lease: Arc<FakeLeaseOps>,
        clock: FakeClock,
    ) -> TickLoop<
        Arc<FakeRunner>,
        MockGitRefClient,
        Arc<FakeLeaseOps>,
        FakeClock,
        Arc<FakeProvisioner>,
    > {
        build_loop_with_provisioner(
            td,
            cfg,
            runner,
            git,
            lease,
            clock,
            Arc::new(FakeProvisioner::default()),
        )
    }

    fn build_loop_with_provisioner(
        td: &TempDir,
        cfg: Config,
        runner: Arc<FakeRunner>,
        git: MockGitRefClient,
        lease: Arc<FakeLeaseOps>,
        clock: FakeClock,
        provisioner: Arc<FakeProvisioner>,
    ) -> TickLoop<
        Arc<FakeRunner>,
        MockGitRefClient,
        Arc<FakeLeaseOps>,
        FakeClock,
        Arc<FakeProvisioner>,
    > {
        TickLoop::new(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            runner,
            git,
            lease,
            clock,
            provisioner,
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
            Arc::new(FakeProvisioner::default()),
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
            Arc<FakeProvisioner>,
        >,
        doc_id: &str,
        agent_ident: &str,
        last_hb: Instant,
    ) {
        let observation = Arc::new(Mutex::new(AgentObservation::new(last_hb)));
        insert_fake_running_with_obs(t, doc_id, agent_ident, last_hb, observation);
    }

    /// Returns a `Sender<()>` clone (kept alive via the loop's `RunningAgent`)
    /// and the observation handle so tests can mutate state and inspect cancel
    /// receipt.
    fn insert_fake_running_with_obs(
        t: &mut TickLoop<
            Arc<FakeRunner>,
            MockGitRefClient,
            Arc<FakeLeaseOps>,
            FakeClock,
            Arc<FakeProvisioner>,
        >,
        doc_id: &str,
        agent_ident: &str,
        last_hb: Instant,
        observation: Arc<Mutex<AgentObservation>>,
    ) -> Receiver<()> {
        let (cn_tx, cn_rx) = unbounded::<()>();
        // Dummy reader thread — finishes immediately, gives us a real
        // JoinHandle without spawning a reader against a live channel.
        let reader_handle = std::thread::spawn(|| {});
        t.running.insert(
            doc_id.to_string(),
            RunningAgent {
                session_id: "sess".to_string(),
                doc_id: doc_id.to_string(),
                doc_type: "story".to_string(),
                agent_ident: agent_ident.to_string(),
                workspace: PathBuf::from(format!("/tmp/fake-ws/{}", doc_id)),
                branch: format!("agents/{}", doc_id),
                cancel: cn_tx,
                pid: 1,
                last_heartbeat: last_hb,
                observation,
                reader_handle: Some(reader_handle),
            },
        );
        cn_rx
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
            Arc::new(FakeProvisioner::default()),
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
            Arc::new(FakeProvisioner::default()),
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
            Arc::new(FakeProvisioner::default()),
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
            Arc::new(FakeProvisioner::default()),
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

    // ===========================================================
    // Per-agent event reader (Iter B, task 2)
    // ===========================================================

    use crate::engine::runner::ToolStatus;

    fn spawn_reader(
        rx: Receiver<AgentEvent>,
    ) -> (Arc<Mutex<AgentObservation>>, std::thread::JoinHandle<()>) {
        let obs = Arc::new(Mutex::new(AgentObservation::new(Instant::now())));
        let reader_obs = Arc::clone(&obs);
        let handle = std::thread::spawn(move || run_event_reader(rx, reader_obs));
        (obs, handle)
    }

    #[test]
    fn reader_records_last_event_at_on_any_event() {
        let (tx, rx) = unbounded::<AgentEvent>();
        let (obs, handle) = spawn_reader(rx);
        let before = Instant::now();
        tx.send(AgentEvent::SessionStarted).unwrap();
        // Close channel so reader exits and we can deterministically read state.
        drop(tx);
        handle.join().unwrap();
        let after = Instant::now();
        let g = obs.lock().unwrap();
        assert!(g.last_event_at >= before);
        assert!(g.last_event_at <= after);
    }

    #[test]
    fn reader_sets_tool_use_in_flight_on_tool_call_started() {
        let (tx, rx) = unbounded::<AgentEvent>();
        let (obs, handle) = spawn_reader(rx);
        tx.send(AgentEvent::ToolCallStarted {
            name: "Bash".into(),
        })
        .unwrap();
        drop(tx);
        handle.join().unwrap();
        assert!(obs.lock().unwrap().tool_use_in_flight);
    }

    #[test]
    fn reader_clears_tool_use_in_flight_on_tool_call() {
        let (tx, rx) = unbounded::<AgentEvent>();
        let (obs, handle) = spawn_reader(rx);
        tx.send(AgentEvent::ToolCallStarted {
            name: "Bash".into(),
        })
        .unwrap();
        tx.send(AgentEvent::ToolCall {
            name: "Bash".into(),
            summary: "ok".into(),
            status: ToolStatus::Ok,
        })
        .unwrap();
        drop(tx);
        handle.join().unwrap();
        assert!(!obs.lock().unwrap().tool_use_in_flight);
    }

    #[test]
    fn reader_clears_tool_use_in_flight_on_turn_completed() {
        let (tx, rx) = unbounded::<AgentEvent>();
        let (obs, handle) = spawn_reader(rx);
        tx.send(AgentEvent::ToolCallStarted {
            name: "Read".into(),
        })
        .unwrap();
        tx.send(AgentEvent::TurnCompleted {
            input_tokens: 1,
            output_tokens: 2,
        })
        .unwrap();
        drop(tx);
        handle.join().unwrap();
        assert!(!obs.lock().unwrap().tool_use_in_flight);
    }

    #[test]
    fn reader_resets_turn_started_at_on_turn_completed() {
        let (tx, rx) = unbounded::<AgentEvent>();
        let (obs, handle) = spawn_reader(rx);
        let initial_turn_start = obs.lock().unwrap().turn_started_at;
        tx.send(AgentEvent::TurnCompleted {
            input_tokens: 0,
            output_tokens: 0,
        })
        .unwrap();
        drop(tx);
        handle.join().unwrap();
        let new_turn_start = obs.lock().unwrap().turn_started_at;
        assert!(new_turn_start > initial_turn_start);
    }

    #[test]
    fn reader_records_exit_on_subprocess_exited() {
        let (tx, rx) = unbounded::<AgentEvent>();
        let (obs, handle) = spawn_reader(rx);
        tx.send(AgentEvent::SubprocessExited { code: Some(7) })
            .unwrap();
        // Reader should exit on its own after SubprocessExited; join confirms.
        handle.join().unwrap();
        assert_eq!(obs.lock().unwrap().exit, Some(Some(7)));
    }

    #[test]
    fn reader_exits_loop_on_channel_close() {
        let (tx, rx) = unbounded::<AgentEvent>();
        let (_obs, handle) = spawn_reader(rx);
        drop(tx);
        // join returns once recv() errors out on disconnect.
        handle.join().unwrap();
    }

    // ===========================================================
    // AC8 — stall detection w/ tool_use suspension (Iter B, task 3)
    // ===========================================================

    #[test]
    fn stall_kills_agent_when_idle_exceeds_timeout() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-S1",
            "host-test:sess-1",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        assert!(!t.running.contains_key("STORY-S1"));
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].kind, RetryReason::Stall);
        assert_eq!(t.retry_queue[0].doc_id, "STORY-S1");
        assert!(cn_rx.try_recv().is_ok(), "cancel signal should be sent");
        let releases: Vec<_> = lease
            .calls()
            .into_iter()
            .filter(|c| matches!(c, LeaseCall::Release { .. }))
            .collect();
        assert_eq!(releases.len(), 1, "lease release should fire once");
    }

    #[test]
    fn stall_suspended_during_tool_use() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().tool_use_in_flight = true;
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-S2",
            "host-test:sess-2",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(30_000));
        t.run_once().unwrap();

        assert!(
            t.running.contains_key("STORY-S2"),
            "tool_use_in_flight suspends stall kill"
        );
        assert!(t.retry_queue.is_empty());
    }

    #[test]
    fn stall_resumes_after_tool_result() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        // Simulate: tool started then completed. After result, in_flight is
        // cleared but last_event_at remains in the past relative to fake clock.
        {
            let mut g = obs.lock().unwrap();
            g.tool_use_in_flight = false;
            g.last_event_at = now;
        }
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-S3",
            "host-test:sess-3",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        assert!(!t.running.contains_key("STORY-S3"));
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].kind, RetryReason::Stall);
    }

    // ===========================================================
    // AC9 — turn timeout (hard wall, independent of tool_use)
    // ===========================================================

    #[test]
    fn turn_timeout_kills_agent_independent_of_tool_use() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000_000; // ensure stall does NOT fire
        orch.runtime.turn_timeout_ms = 60_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        // Tool call in flight — would suspend stall, must NOT suspend turn timeout.
        obs.lock().unwrap().tool_use_in_flight = true;
        let cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-T1",
            "host-test:sess-t1",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(65_000));
        t.run_once().unwrap();

        assert!(!t.running.contains_key("STORY-T1"));
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].kind, RetryReason::TurnTimeout);
        assert_eq!(t.retry_queue[0].doc_id, "STORY-T1");
        assert!(cn_rx.try_recv().is_ok(), "cancel signal should be sent");
        let releases: Vec<_> = lease
            .calls()
            .into_iter()
            .filter(|c| matches!(c, LeaseCall::Release { .. }))
            .collect();
        assert_eq!(releases.len(), 1, "lease release should fire once");
    }

    #[test]
    fn turn_timeout_classified_as_failure() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000_000;
        orch.runtime.turn_timeout_ms = 60_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        {
            let mut g = obs.lock().unwrap();
            g.attempt = 3;
            g.failure_attempt = 2;
        }
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-T2",
            "host-test:sess-t2",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(65_000));
        t.run_once().unwrap();

        assert_eq!(t.retry_queue.len(), 1);
        let retry = &t.retry_queue[0];
        assert_eq!(retry.kind, RetryReason::TurnTimeout);
        // T6 bumps failure_attempt at kill time; attempt unchanged.
        assert_eq!(retry.attempt, 3);
        assert_eq!(retry.failure_attempt, 3);
    }

    #[test]
    fn turn_within_timeout_does_not_kill() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000_000;
        orch.runtime.turn_timeout_ms = 60_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-T3",
            "host-test:sess-t3",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(30_000));
        t.run_once().unwrap();

        assert!(t.running.contains_key("STORY-T3"));
        assert!(t.retry_queue.is_empty());
    }

    #[test]
    fn turn_timeout_takes_precedence_over_stall() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        // Both thresholds will be exceeded by the same clock advance.
        orch.stall_timeout_ms = 10_000;
        orch.runtime.turn_timeout_ms = 20_000;
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
        let now = t.clock.now_instant();
        // turn_started_at == last_event_at == now; tool_use NOT in flight.
        // After advance both stall (idle >= 10s) and turn (elapsed >= 20s) fire.
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-T4",
            "host-test:sess-t4",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(25_000));
        t.run_once().unwrap();

        assert!(!t.running.contains_key("STORY-T4"));
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(
            t.retry_queue[0].kind,
            RetryReason::TurnTimeout,
            "turn timeout must win over stall when both apply"
        );
    }

    #[test]
    fn stall_classified_as_failure() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        {
            let mut g = obs.lock().unwrap();
            g.attempt = 2;
            g.failure_attempt = 1;
        }
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-S4",
            "host-test:sess-4",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        assert_eq!(t.retry_queue.len(), 1);
        let retry = &t.retry_queue[0];
        assert_eq!(retry.kind, RetryReason::Stall);
        // T6 bumps failure_attempt at kill time; attempt unchanged.
        assert_eq!(retry.attempt, 2);
        assert_eq!(retry.failure_attempt, 2);
    }

    // ===========================================================
    // AC10/AC11 — doc-status reconcile (Iter B, task 5)
    // ===========================================================

    #[test]
    fn terminal_status_kills_releases_and_removes_workspace() {
        let td = TempDir::new().unwrap();
        // active_statuses: ["draft"]; handoff_states: ["review"]; doc status
        // will be "complete" → terminal.
        let mut orch = base_orch(vec!["draft"]);
        orch.handoff_states = vec!["review".to_string()];
        let cfg = cfg(orch);
        make_stories_status(&td, 1, "complete");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let provisioner = Arc::new(FakeProvisioner::default());
        let mut t = build_loop_with_provisioner(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::clone(&provisioner),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let cn_rx = insert_fake_running_with_obs(&mut t, "STORY-000", "host-test:sess-1", now, obs);

        t.run_once().unwrap();

        assert!(!t.running.contains_key("STORY-000"));
        assert!(cn_rx.try_recv().is_ok(), "cancel signal should be sent");
        let releases: Vec<_> = lease
            .calls()
            .into_iter()
            .filter(|c| matches!(c, LeaseCall::Release { .. }))
            .collect();
        assert_eq!(releases.len(), 1, "lease release should fire once");
        let removes = provisioner.remove_calls();
        assert_eq!(removes.len(), 1, "workspace::remove should fire once");
        assert_eq!(removes[0], PathBuf::from("/tmp/fake-ws/STORY-000"));
    }

    #[test]
    fn terminal_status_does_not_enqueue_retry() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.handoff_states = vec!["review".to_string()];
        let cfg = cfg(orch);
        make_stories_status(&td, 1, "complete");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let provisioner = Arc::new(FakeProvisioner::default());
        let mut t = build_loop_with_provisioner(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            provisioner,
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let _cn_rx =
            insert_fake_running_with_obs(&mut t, "STORY-000", "host-test:sess-1", now, obs);

        t.run_once().unwrap();

        assert!(
            t.retry_queue.is_empty(),
            "terminal status must not enqueue retry"
        );
    }

    #[test]
    fn handoff_status_kills_and_releases_but_keeps_workspace() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.handoff_states = vec!["review".to_string()];
        let cfg = cfg(orch);
        // Doc status "review" → handoff.
        make_stories_status(&td, 1, "review");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let provisioner = Arc::new(FakeProvisioner::default());
        let mut t = build_loop_with_provisioner(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::clone(&provisioner),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let cn_rx = insert_fake_running_with_obs(&mut t, "STORY-000", "host-test:sess-1", now, obs);

        t.run_once().unwrap();

        assert!(!t.running.contains_key("STORY-000"));
        assert!(cn_rx.try_recv().is_ok(), "cancel signal should be sent");
        let releases: Vec<_> = lease
            .calls()
            .into_iter()
            .filter(|c| matches!(c, LeaseCall::Release { .. }))
            .collect();
        assert_eq!(releases.len(), 1, "lease release should fire once");
        assert!(
            provisioner.remove_calls().is_empty(),
            "handoff status must NOT remove workspace"
        );
        assert!(t.retry_queue.is_empty(), "handoff must not enqueue retry");
    }

    #[test]
    fn missing_doc_left_alone() {
        // Daemon does not infer terminal intent from absence — could be a
        // transient store load issue. Reconcile only fires on a definite
        // non-active status.
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.handoff_states = vec!["review".to_string()];
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let provisioner = Arc::new(FakeProvisioner::default());
        let mut t = build_loop_with_provisioner(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::clone(&provisioner),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let _cn_rx =
            insert_fake_running_with_obs(&mut t, "STORY-000", "host-test:sess-1", now, obs);

        t.run_once().unwrap();

        assert!(t.running.contains_key("STORY-000"));
        assert!(provisioner.remove_calls().is_empty());
    }

    #[test]
    fn status_reconcile_pre_empts_stall_retry() {
        // If a doc transitions to terminal AND the agent is stalled, status
        // reconcile must win — no retry enqueue.
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
        orch.handoff_states = vec!["review".to_string()];
        let cfg = cfg(orch);
        make_stories_status(&td, 1, "complete");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let provisioner = Arc::new(FakeProvisioner::default());
        let mut t = build_loop_with_provisioner(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            provisioner,
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let _cn_rx =
            insert_fake_running_with_obs(&mut t, "STORY-000", "host-test:sess-1", now, obs);

        // Advance past stall threshold so stall WOULD fire.
        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        assert!(
            t.retry_queue.is_empty(),
            "status reconcile must pre-empt stall retry"
        );
    }

    // ===========================================================
    // AC12/AC13/AC14 — retry queue (Iter B, task 6)
    // ===========================================================

    #[derive(Default)]
    struct RecordingEventSink {
        calls: Mutex<Vec<(String, String, String)>>,
    }

    impl RecordingEventSink {
        fn snapshot(&self) -> Vec<(String, String, String)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl AgentEventSink for Arc<RecordingEventSink> {
        fn emit_failed(&self, doc_id: &str, agent_ident: &str, reason: &str) {
            self.calls.lock().unwrap().push((
                doc_id.to_string(),
                agent_ident.to_string(),
                reason.to_string(),
            ));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn build_loop_with_sink(
        td: &TempDir,
        cfg: Config,
        runner: Arc<FakeRunner>,
        git: MockGitRefClient,
        lease: Arc<FakeLeaseOps>,
        clock: FakeClock,
        provisioner: Arc<FakeProvisioner>,
        sink: Arc<RecordingEventSink>,
    ) -> TickLoop<
        Arc<FakeRunner>,
        MockGitRefClient,
        Arc<FakeLeaseOps>,
        FakeClock,
        Arc<FakeProvisioner>,
    > {
        TickLoop::with_event_sink(
            td.path().to_path_buf(),
            cfg,
            "host-test".to_string(),
            runner,
            git,
            lease,
            clock,
            provisioner,
            Box::new(sink),
        )
    }

    // ---- AC12: Clean exit continuation ----

    #[test]
    fn clean_exit_enqueues_continuation_with_delay() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.continuation_delay_ms = 1_000;
        orch.max_turns = 20;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().exit = Some(Some(0));
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-CE1",
            "host-test:sess-ce1",
            now,
            Arc::clone(&obs),
        );

        t.run_once().unwrap();

        assert!(!t.running.contains_key("STORY-CE1"));
        assert_eq!(t.retry_queue.len(), 1);
        let retry = &t.retry_queue[0];
        assert_eq!(retry.kind, RetryReason::CleanExit);
        assert_eq!(retry.attempt, 2, "clean exit increments attempt");
        assert_eq!(retry.failure_attempt, 0);
        assert_eq!(retry.ready_at, now + Duration::from_millis(1_000));
    }

    #[test]
    fn continuation_reuses_same_workspace() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.continuation_delay_ms = 5_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().exit = Some(Some(0));
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-CE2",
            "host-test:sess-ce2",
            now,
            Arc::clone(&obs),
        );
        let original_workspace = PathBuf::from("/tmp/fake-ws/STORY-CE2");

        // First tick: classify exit + enqueue retry (ready_at == now + 5s).
        t.run_once().unwrap();
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].workspace, original_workspace);
        // Drain didn't fire because ready_at is still in the future.
        assert_eq!(runner.spawn_count(), 0);

        // Advance past delay; second tick drains → spawn in same workspace.
        t.clock.advance(Duration::from_millis(6_000));
        t.run_once().unwrap();
        assert_eq!(runner.spawn_count(), 1);
        let calls = runner.calls.lock().unwrap();
        let ctx = &calls[0];
        assert_eq!(ctx.workspace, original_workspace);
        assert_eq!(ctx.doc_id, "STORY-CE2");
    }

    #[test]
    fn continuation_caps_at_max_turns() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.max_turns = 3;
        orch.continuation_delay_ms = 0;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let sink = Arc::new(RecordingEventSink::default());
        let mut t = build_loop_with_sink(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::new(FakeProvisioner::default()),
            Arc::clone(&sink),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        {
            let mut g = obs.lock().unwrap();
            g.attempt = 3;
            g.exit = Some(Some(0));
        }
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-CECAP",
            "host-test:sess-cap",
            now,
            Arc::clone(&obs),
        );

        t.run_once().unwrap();

        assert!(t.retry_queue.is_empty(), "cap must NOT enqueue retry");
        assert!(!t.running.contains_key("STORY-CECAP"));
        let calls = sink.snapshot();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "STORY-CECAP");
        assert_eq!(calls[0].2, "max_turns");
        let releases: Vec<_> = lease
            .calls()
            .into_iter()
            .filter(|c| matches!(c, LeaseCall::Release { .. }))
            .collect();
        assert_eq!(releases.len(), 1);
    }

    #[test]
    fn continuation_does_not_increment_failure_attempt() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.continuation_delay_ms = 10_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        {
            let mut g = obs.lock().unwrap();
            g.failure_attempt = 2;
            g.attempt = 5;
            g.exit = Some(Some(0));
        }
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-CE3",
            "host-test:sess-ce3",
            now,
            Arc::clone(&obs),
        );

        t.run_once().unwrap();
        assert_eq!(t.retry_queue.len(), 1);
        let retry = &t.retry_queue[0];
        assert_eq!(retry.failure_attempt, 2, "clean exit must NOT bump failure");
        assert_eq!(retry.attempt, 6);
    }

    // ---- AC13: Failure backoff ----

    #[test]
    fn failure_backoff_exponential_capped() {
        // n -> expected delay_ms, cap at 60_000.
        let cases = [
            (1u32, 10_000u64),
            (2, 20_000),
            (3, 40_000),
            (4, 60_000), // would be 80_000, capped
            (5, 60_000), // would be 160_000, capped
        ];
        for (n_minus_one, expected_ms) in cases {
            let td = TempDir::new().unwrap();
            let mut orch = base_orch(vec!["draft"]);
            orch.stall_timeout_ms = 10_000;
            orch.max_retry_backoff_ms = 60_000;
            orch.max_failure_attempts = 100;
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
            let now = t.clock.now_instant();
            let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
            obs.lock().unwrap().failure_attempt = n_minus_one - 1;
            let _cn_rx = insert_fake_running_with_obs(
                &mut t,
                "STORY-FB",
                "host-test:sess-fb",
                now,
                Arc::clone(&obs),
            );

            t.clock.advance(Duration::from_millis(15_000));
            t.run_once().unwrap();

            assert_eq!(t.retry_queue.len(), 1, "n={}", n_minus_one);
            let retry = &t.retry_queue[0];
            let expected_at = t.clock.now_instant() + Duration::from_millis(expected_ms);
            assert_eq!(retry.ready_at, expected_at, "n={}", n_minus_one);
            assert_eq!(retry.failure_attempt, n_minus_one, "n={}", n_minus_one);
        }
    }

    #[test]
    fn failure_increments_failure_attempt_only() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        {
            let mut g = obs.lock().unwrap();
            g.attempt = 7;
            g.failure_attempt = 1;
        }
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-FFA",
            "host-test:sess-ffa",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        assert_eq!(t.retry_queue.len(), 1);
        let retry = &t.retry_queue[0];
        assert_eq!(retry.attempt, 7, "failure must NOT change attempt");
        assert_eq!(retry.failure_attempt, 2);
    }

    #[test]
    fn stall_and_abnormal_share_counter() {
        // First kill: Stall raises failure_attempt 0 -> 1. Re-insert agent with
        // that observation (mimic a re-spawn) then kill via AbnormalExit; counter
        // continues 1 -> 2.
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-MX",
            "host-test:sess-mx",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].kind, RetryReason::Stall);
        assert_eq!(t.retry_queue[0].failure_attempt, 1);

        // Clear queue, simulate next-cycle agent w/ carried counter, then abnormal exit.
        t.retry_queue.clear();
        let now2 = t.clock.now_instant();
        let obs2 = Arc::new(Mutex::new(AgentObservation::new(now2)));
        {
            let mut g = obs2.lock().unwrap();
            g.failure_attempt = 1;
            g.exit = Some(Some(7));
        }
        let _cn_rx2 = insert_fake_running_with_obs(
            &mut t,
            "STORY-MX",
            "host-test:sess-mx",
            now2,
            Arc::clone(&obs2),
        );
        t.run_once().unwrap();
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].kind, RetryReason::AbnormalExit);
        assert_eq!(t.retry_queue[0].failure_attempt, 2);
    }

    #[test]
    fn abnormal_exit_classified_as_failure() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().exit = Some(Some(9));
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-AE",
            "host-test:sess-ae",
            now,
            Arc::clone(&obs),
        );

        t.run_once().unwrap();
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].kind, RetryReason::AbnormalExit);
        assert_eq!(t.retry_queue[0].failure_attempt, 1);
    }

    #[test]
    fn signal_exit_classified_as_failure() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
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
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().exit = Some(None); // killed by signal
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-SIG",
            "host-test:sess-sig",
            now,
            Arc::clone(&obs),
        );

        t.run_once().unwrap();
        assert_eq!(t.retry_queue.len(), 1);
        assert_eq!(t.retry_queue[0].kind, RetryReason::AbnormalExit);
    }

    // ---- AC14: Post-cap ----

    #[test]
    fn post_cap_releases_lease() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
        orch.max_failure_attempts = 2;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let sink = Arc::new(RecordingEventSink::default());
        let mut t = build_loop_with_sink(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::new(FakeProvisioner::default()),
            Arc::clone(&sink),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().failure_attempt = 2;
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-PC1",
            "host-test:sess-pc1",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        let releases: Vec<_> = lease
            .calls()
            .into_iter()
            .filter(|c| matches!(c, LeaseCall::Release { .. }))
            .collect();
        assert_eq!(releases.len(), 1);
    }

    #[test]
    fn post_cap_emits_failed_event() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
        orch.max_failure_attempts = 2;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let sink = Arc::new(RecordingEventSink::default());
        let mut t = build_loop_with_sink(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::new(FakeProvisioner::default()),
            Arc::clone(&sink),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().failure_attempt = 2;
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-PC2",
            "host-test:sess-pc2",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        let calls = sink.snapshot();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "STORY-PC2");
        assert_eq!(calls[0].1, "host-test:sess-pc2");
        assert_eq!(calls[0].2, "max_failure_attempts");
    }

    #[test]
    fn post_cap_does_not_enqueue_retry() {
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
        orch.max_failure_attempts = 2;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let sink = Arc::new(RecordingEventSink::default());
        let mut t = build_loop_with_sink(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::new(FakeProvisioner::default()),
            Arc::clone(&sink),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().failure_attempt = 2;
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-PC3",
            "host-test:sess-pc3",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        assert!(t.retry_queue.is_empty());
    }

    #[test]
    fn post_cap_does_not_mutate_doc_status() {
        // RFC-041 conservative posture: hitting the failure cap releases the
        // lease and emits `failed`, but the engine never touches the doc's
        // status — human triage owns that transition.
        let td = TempDir::new().unwrap();
        make_stories_status(&td, 1, "draft");
        let story_path = td.path().join("docs/stories/STORY-000-s.md");
        let before = std::fs::read_to_string(&story_path).unwrap();

        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
        orch.max_failure_attempts = 2;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let sink = Arc::new(RecordingEventSink::default());
        let mut t = build_loop_with_sink(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::new(FakeProvisioner::default()),
            Arc::clone(&sink),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().failure_attempt = 2;
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-000",
            "host-test:sess-pc-status",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        // Sanity: cap actually fired.
        let calls = sink.snapshot();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, "max_failure_attempts");

        let after = std::fs::read_to_string(&story_path).unwrap();
        assert_eq!(before, after, "doc file must be untouched on cap");
    }

    #[test]
    fn post_cap_does_not_remove_workspace() {
        // RFC-041 conservative posture: workspace is left in place on cap so a
        // human can inspect / resume. `kill_agent_for_retry` must not call
        // `workspace_provisioner.remove`.
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.stall_timeout_ms = 10_000;
        orch.max_failure_attempts = 2;
        let cfg = cfg(orch);
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let sink = Arc::new(RecordingEventSink::default());
        let provisioner = Arc::new(FakeProvisioner::default());
        let mut t = build_loop_with_sink(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::clone(&provisioner),
            Arc::clone(&sink),
        );
        let now = t.clock.now_instant();
        let obs = Arc::new(Mutex::new(AgentObservation::new(now)));
        obs.lock().unwrap().failure_attempt = 2;
        let _cn_rx = insert_fake_running_with_obs(
            &mut t,
            "STORY-PC4",
            "host-test:sess-pc4",
            now,
            Arc::clone(&obs),
        );

        t.clock.advance(Duration::from_millis(15_000));
        t.run_once().unwrap();

        // Sanity: cap actually fired.
        let calls = sink.snapshot();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].2, "max_failure_attempts");

        assert!(
            provisioner.remove_calls().is_empty(),
            "workspace must be left in place on failure cap, got {:?}",
            provisioner.remove_calls()
        );
    }

    // ---- Drain ----

    fn enqueue_retry(
        t: &mut TickLoop<
            Arc<FakeRunner>,
            MockGitRefClient,
            Arc<FakeLeaseOps>,
            FakeClock,
            Arc<FakeProvisioner>,
        >,
        doc_id: &str,
        agent_ident: &str,
        ready_at: Instant,
        attempt: u32,
        failure_attempt: u32,
    ) {
        t.retry_queue.push(PendingRetry {
            doc_id: doc_id.to_string(),
            doc_type: "story".to_string(),
            workspace: PathBuf::from(format!("/tmp/fake-ws/{}", doc_id)),
            branch: format!("agents/{}", doc_id),
            agent_ident: agent_ident.to_string(),
            session_id: "sess".to_string(),
            attempt,
            failure_attempt,
            ready_at,
            kind: RetryReason::CleanExit,
        });
    }

    #[test]
    fn drain_respawns_when_ready() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
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
        let now = t.clock.now_instant();
        enqueue_retry(&mut t, "STORY-DR1", "host-test:sess-dr1", now, 3, 1);

        t.run_once().unwrap();

        assert_eq!(runner.spawn_count(), 1);
        assert!(t.running.contains_key("STORY-DR1"));
        assert!(t.retry_queue.is_empty());
        let ra = t.running.get("STORY-DR1").unwrap();
        let g = ra.observation.lock().unwrap();
        assert_eq!(g.attempt, 3, "carried attempt");
        assert_eq!(g.failure_attempt, 1, "carried failure_attempt");
    }

    #[test]
    fn drain_skips_not_yet_ready() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
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
        let now = t.clock.now_instant();
        let future = now + Duration::from_secs(3600);
        enqueue_retry(&mut t, "STORY-DR2", "host-test:sess-dr2", future, 1, 0);

        t.run_once().unwrap();

        assert_eq!(runner.spawn_count(), 0);
        assert_eq!(t.retry_queue.len(), 1, "entry retained for later tick");
        assert!(!t.running.contains_key("STORY-DR2"));
    }

    #[test]
    fn drain_cas_failure_emits_failed_and_abandons() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        // Make the re-acquire fail.
        lease
            .acquire_results
            .lock()
            .unwrap()
            .push(Err(anyhow::anyhow!("CAS rejected")));
        let sink = Arc::new(RecordingEventSink::default());
        let mut t = build_loop_with_sink(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
            Arc::new(FakeProvisioner::default()),
            Arc::clone(&sink),
        );
        let now = t.clock.now_instant();
        enqueue_retry(&mut t, "STORY-DRC", "host-test:sess-drc", now, 2, 1);

        t.run_once().unwrap();

        assert_eq!(runner.spawn_count(), 0);
        assert!(t.retry_queue.is_empty(), "abandoned on CAS failure");
        assert!(!t.running.contains_key("STORY-DRC"));
        let calls = sink.snapshot();
        let has_lease_cas = calls
            .iter()
            .any(|(d, _, r)| d == "STORY-DRC" && r == "lease_cas_failed");
        assert!(
            has_lease_cas,
            "expected lease_cas_failed emit, got {:?}",
            calls
        );
    }

    #[test]
    fn drain_runs_before_dispatch() {
        // When slots are tight, retried agents must take precedence over fresh
        // candidates. Concurrency cap = 1, one queued retry + one fresh
        // candidate; only the retry should spawn.
        let td = TempDir::new().unwrap();
        let mut orch = base_orch(vec!["draft"]);
        orch.max_concurrent_agents = 1;
        let cfg = cfg(orch);
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
        let now = t.clock.now_instant();
        enqueue_retry(&mut t, "STORY-RET", "host-test:sess-ret", now, 2, 0);

        t.run_once().unwrap();

        assert_eq!(runner.spawn_count(), 1);
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[0].doc_id, "STORY-RET", "retry must spawn first");
    }

    // ===========================================================
    // AC16 — preflight gate + notify-driven re-run
    // ===========================================================

    use crate::engine::preflight::PreflightWatcher;

    /// Fake watcher: pop one queued bool per `poll`. Empty queue → returns false.
    struct FakePreflightWatcher {
        results: Mutex<Vec<bool>>,
    }

    impl FakePreflightWatcher {
        fn new(results: Vec<bool>) -> Self {
            Self {
                results: Mutex::new(results),
            }
        }
    }

    impl PreflightWatcher for FakePreflightWatcher {
        fn poll(&self) -> bool {
            let mut q = self.results.lock().unwrap();
            if q.is_empty() {
                false
            } else {
                q.remove(0)
            }
        }
    }

    fn write_prompt(root: &std::path::Path, contents: &str) {
        let dir = root.join(".lazyspec/prompts");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("builder.md"), contents).unwrap();
    }

    #[test]
    fn preflight_failure_gates_dispatch() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
        make_stories_status(&td, 3, "draft");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let failing = PreflightReport {
            workflow_readable: false,
            prompt_renders: false,
            agent_users_non_empty: false,
        };
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        )
        .with_preflight(failing, None);

        t.run_once().unwrap();

        assert_eq!(
            runner.spawn_count(),
            0,
            "preflight=fail must block new dispatches"
        );
        let acquired = lease
            .calls()
            .iter()
            .filter(|c| matches!(c, LeaseCall::Acquire { .. }))
            .count();
        assert_eq!(acquired, 0, "preflight=fail must skip acquire");
    }

    #[test]
    fn preflight_watcher_marks_dirty_on_config_change() {
        let td = TempDir::new().unwrap();
        // Make preflight pass on re-run so we can observe `dirty` getting
        // cleared while the report stays ok.
        write_prompt(td.path(), "Doc {{ doc.id }}");
        let cfg = cfg(base_orch(vec!["draft"]));
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let watcher = Box::new(FakePreflightWatcher::new(vec![true]));
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        )
        .with_preflight(PreflightReport::all_ok(), Some(watcher));

        t.run_once().unwrap();

        // Dirty cleared after the re-run.
        assert!(!t.preflight_dirty, "dirty flag must be cleared after rerun");
        // Report stays ok because on-disk state is valid.
        assert!(t.preflight.is_ok(), "expected ok after rerun");
    }

    #[test]
    fn preflight_rerun_after_dirty_flag() {
        let td = TempDir::new().unwrap();
        // Start invalid (no prompt on disk) but seed a passing in-memory
        // report. Watcher fires → run_preflight reads disk → report flips to
        // fail → dispatch is gated on the very next tick.
        let cfg = cfg(base_orch(vec!["draft"]));
        make_stories_status(&td, 1, "draft");
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let watcher = Box::new(FakePreflightWatcher::new(vec![true]));
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        )
        .with_preflight(PreflightReport::all_ok(), Some(watcher));

        t.run_once().unwrap();

        assert!(
            !t.preflight.is_ok(),
            "preflight should have flipped to fail on rerun (prompt missing)"
        );
        assert_eq!(
            runner.spawn_count(),
            0,
            "dispatch gated immediately on rerun-failure"
        );

        // Now fix the on-disk state and fire the watcher again. Preflight
        // should pass and dispatch resume on the next tick.
        write_prompt(td.path(), "Doc {{ doc.id }}");
        t.preflight_watcher = Some(Box::new(FakePreflightWatcher::new(vec![true])));
        t.run_once().unwrap();

        assert!(t.preflight.is_ok(), "preflight should be ok after fix");
        assert_eq!(
            runner.spawn_count(),
            1,
            "dispatch resumes once preflight passes"
        );
    }

    #[test]
    fn preflight_failure_does_not_yank_in_flight_agents() {
        let td = TempDir::new().unwrap();
        let cfg = cfg(base_orch(vec!["draft"]));
        let runner = Arc::new(FakeRunner::new());
        let lease = Arc::new(FakeLeaseOps::new());
        let failing = PreflightReport {
            workflow_readable: true,
            prompt_renders: true,
            agent_users_non_empty: false,
        };
        let mut t = build_loop(
            &td,
            cfg,
            Arc::clone(&runner),
            MockGitRefClient::new(),
            Arc::clone(&lease),
            FakeClock::new(),
        )
        .with_preflight(failing, None);

        let now = t.clock.now_instant();
        insert_fake_running(&mut t, "STORY-IF1", "host-test:sess-if1", now);

        t.run_once().unwrap();

        assert!(
            t.running.contains_key("STORY-IF1"),
            "in-flight agent must NOT be yanked on preflight failure"
        );
        assert_eq!(runner.spawn_count(), 0, "no new dispatches");
    }
}
