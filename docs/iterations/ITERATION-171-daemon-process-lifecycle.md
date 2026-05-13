---
title: Daemon process lifecycle
type: iteration
status: accepted
author: agent
date: 2026-05-12
tags: []
related:
- implements: STORY-121
---

## Context

STORY-121 / RFC-041 slice 2. Foreground `lazyspec daemon` cmd. Bind UDS `.lazyspec/daemon.sock`. SIGTERM/SIGINT graceful: drain (no-op v1) → release this host's RFC-035 leases → close+unlink → exit 0. Stale sock cleanup. Single-instance per sock. Sample systemd unit + launchd plist in user guide. No fork. No PID file. No spawn yet.

Verified paths:
- CLI cmds: `src/cli/*.rs` (flat, e.g. `src/cli/lease.rs`). New: `src/cli/daemon.rs`. Wire in `src/cli.rs` enum + `src/main.rs` dispatch.
- Engine: `src/engine/*.rs` flat. New: `src/engine/daemon.rs`, `src/engine/host_id.rs`. Wire in `src/engine.rs`.
- Lease engine: `src/engine/lease.rs` — `LeaseEngine::release(root, type_name, id, agent)`, `admin_release(...)`. No release-by-host yet. `GitRefOps::list_refs(pattern)` enumerates → add `LeaseEngine::release_by_host_prefix(root, type_names, host_prefix)` that lists `refs/lazyspec/leases/<t>/*`, reads each blob, filters `lease.agent.starts_with("{host}:")`, calls `release` per match.
- User guide: `README.md` — has `## Coordination` section (line 173). Add `### Daemon Deployment` subsection there OR new top-level near end. Pick: new `## Daemon Deployment` after `## Coordination` for visibility.
- Cargo.toml: no tokio, no signal-hook, no ctrlc. Repo is sync. Add `signal-hook = "0.3"` (well-maintained, sync, ecosystem norm). UDS via `std::os::unix::net::{UnixListener, UnixStream}` (std lib, no dep).
- Tests: `tests/*.rs` flat + `tests/common/mod.rs` (TestFixture).
- Agent id derivation: `src/engine/agent.rs` — `resolve_agent_id`. host_id is new primitive distinct from agent id.

## Goal

Ship `lazyspec daemon` foreground cmd. Binds UDS, handles SIGTERM/SIGINT graceful, releases host-owned leases on shutdown, single-instance + stale cleanup. No agent spawning. Lays I/O seam for later slices.

## Test Plan

AC1 — `cargo run -- daemon` blocks: integration test `tests/cli_daemon_test.rs`. Spawn `lazyspec daemon` as `std::process::Command`. Poll for socket presence (max 2s). Assert process still alive, not exited. Send SIGTERM via `nix` or `libc::kill`, await exit. Fixtures: `TempDir` workspace w/ `.lazyspec/` init'd.

AC2 — SIGTERM graceful: integration `tests/cli_daemon_test.rs`. Pre-seed a lease ref `refs/lazyspec/leases/story/STORY-001` w/ agent `{host}:sess-1` (where host = the daemon's host_id, pre-written to `.lazyspec/daemon-host-id`). Spawn daemon, wait for sock, SIGTERM, await exit 0. Assert: (a) sock file gone, (b) lease ref gone (released), (c) stderr contains release log. Fixtures: `TempDir`, bare git init, pre-seeded ref via `git update-ref`. Tradeoff: real-process integration test is slower (~1-3s) but verifies signal handler wiring end-to-end. Mitigate w/ unit tests on engine.

AC3 — SIGINT: same as AC2 w/ SIGINT instead. Same fixtures. Asserts same shutdown path.

AC2/3 unit coverage — engine: `src/engine/daemon.rs` `#[cfg(test)]`. `Daemon::run(shutdown_rx)` accepts a `crossbeam_channel::Receiver<()>` (crossbeam already a dep). Test fires the channel directly, no real signal, asserts: lease engine `release_by_host_prefix` called w/ correct host id, socket unlinked. Mock lease engine via trait `LeaseReleaser` (one method, one impl, one test mock — dictum 6 OK because seam is tested-against).

AC4 — sock present + listening: integration `tests/cli_daemon_test.rs`. Spawn daemon, poll `.lazyspec/daemon.sock`. Assert `std::os::unix::fs::FileTypeExt::is_socket()` true. Open `UnixStream::connect()` — succeeds. SIGTERM, await exit, sock gone.

AC5 — stale sock cleanup: integration. Pre-create a non-socket file OR a leftover socket w/ no listener at `.lazyspec/daemon.sock`. Spawn daemon. Assert startup succeeds, daemon listens. Fixture: write a dead socket via `UnixListener::bind` then drop without unlink, OR use a regular file at the path. Test both.

AC6 — single-instance refuses: integration. Spawn daemon A, await listening. Spawn daemon B same workspace. Assert B exits non-zero within 1s. Assert A still alive + listening (connect to sock succeeds). SIGTERM A, cleanup.

AC7 — no fork, no PID file: integration. Spawn daemon as subprocess, capture child pid. Assert: (a) no file at `.lazyspec/daemon.pid`, (b) no file at `/var/run/lazyspec.pid`, (c) `/proc/<pid>` (linux) or `ps -p <pid>` (mac) shows pid alive AS the spawned child (not reparented to init). Cross-platform mac check: `ps -o ppid= -p <child_pid>` == test process pid. Tradeoff surfaced: "no fork" is asserted via parentage check, not absence-of-fork-syscall. Pragmatic + portable.

AC8 — user guide samples: doc-presence test `tests/cli_daemon_test.rs` or `tests/readme_daemon_samples_test.rs`. Read `README.md`, assert it contains `[Unit]`, `ExecStart=lazyspec daemon`, `<key>ProgramArguments</key>`, `lazyspec` + `daemon` strings in plist context. Cheaper than parsing systemd/plist. Tradeoff: doesn't validate semantic correctness of unit file; manual review covers that. Acceptable for slice 2.

## Changes

1. **Host id primitive** — ACs 2,3. New `src/engine/host_id.rs`. Pub `fn host_id(root: &Path) -> Result<String>`: read `.lazyspec/daemon-host-id` if exists, else gen UUID v4, write atomically (tempfile + rename), return `format!("{}-{}", gethostname, uuid)`. Use `uuid = "1"` (already a dep). `gethostname` via `libc::gethostname` (no new dep) OR add `hostname = "0.4"` crate — pick libc, zero new deps. Wire in `src/engine.rs`. Unit tests: idempotent (call twice → same id), survives across instances, file written under `.lazyspec/`. Verify by inspecting file + 2 calls return equal strings.

2. **Lease release-by-host engine method** — AC 2,3. Extend `src/engine/lease.rs`. New `LeaseEngine::release_by_host_prefix(&self, root, type_names: &[&str], host_prefix: &str) -> Result<Vec<ReleasedLease>>`. Impl: for each type, `git.list_refs("refs/lazyspec/leases/<t>/")`, read each blob `lease.json`, filter `lease.agent.starts_with(&format!("{}:", host_prefix))`, call `self.release(root, type_name, id, &lease.agent)`. Collect released ids. On per-lease error: log + continue (best-effort drain). Unit tests w/ MockGit: 3 leases, 2 owned by host, 1 by other host → 2 released, 1 left. Empty list → ok. Lease for foreign host → skipped.

3. **Daemon engine** — ACs 1,2,3,4,5,6. New `src/engine/daemon.rs`. Types:
   - `pub struct Daemon { root: PathBuf, sock_path: PathBuf, host_id: String, lease_engine: LeaseEngine<GitCli>, type_names: Vec<String> }`.
   - `pub fn new(root, config) -> Result<Self>` — derives sock_path = `root/.lazyspec/daemon.sock`, host_id via `host_id::host_id`, lease engine from `config.coordination`, type_names from `config.documents.types`.
   - `pub fn run(&self, shutdown_rx: crossbeam_channel::Receiver<()>) -> Result<()>` — single-instance + stale check, bind UDS, accept loop (no handlers, just accept-and-drop in v1), block on shutdown_rx, drain (no-op), release host's leases, unlink sock, return Ok.
   - Single-instance + stale logic: try `UnixStream::connect(&sock_path)`. Ok → live daemon → bail w/ exit-non-zero error. Err `ECONNREFUSED`/`ENOENT` → stale or absent → `fs::remove_file` (ignore NotFound) → `UnixListener::bind`. Listener set non-blocking; accept loop in dedicated thread w/ shutdown poll via `try_recv` on shutdown_rx clone, OR select on shutdown via `crossbeam_channel::select!` w/ a 100ms tick that polls listener.accept. Pick: dedicated accept thread + atomic shutdown flag (simplest, no async runtime needed).
   - Unit tests `#[cfg(test)]`: stale sock removed + bound; live sock refused (spawn two `Daemon::run` on separate threads w/ same path); shutdown channel triggers release+unlink. Use `tempfile::TempDir` workspaces. Mock lease engine via trait `LeaseReleaser { fn release_host_leases(&self, host_prefix: &str) -> Result<()> }` w/ prod impl wrapping `LeaseEngine::release_by_host_prefix`.

4. **CLI command** — ACs 1,2,3,7. New `src/cli/daemon.rs`:
   - `pub fn run(root: &Path, config: &Config) -> Result<()>`: build `Daemon::new`. Set up `signal_hook::iterator::Signals` for `SIGTERM, SIGINT`. Spawn signal thread that on first signal sends `()` on a `crossbeam_channel::bounded(1)` shutdown tx, then loops draining further signals (idempotent). Call `daemon.run(shutdown_rx)`. Return its result.
   - Wire `Commands::Daemon` variant in `src/cli.rs` (no args in v1) + dispatch in `src/main.rs`. Thin wrapper: signal wiring + delegate to engine.
   - No fork. No PID file. No daemonization.
   - Update `Cargo.toml`: `signal-hook = "0.3"`.

5. **User guide** — AC 8. `README.md`. New `## Daemon Deployment` section after `## Coordination` (line 173). Contents:
   - 2-line intro: daemon is foreground, supervise w/ systemd/launchd.
   - `### systemd` — full unit file: `[Unit] Description=lazyspec orchestration daemon`, `[Service] Type=simple ExecStart=/usr/local/bin/lazyspec daemon WorkingDirectory=/path/to/repo Restart=on-failure User=lazyspec`, `[Install] WantedBy=multi-user.target`.
   - `### launchd` — full plist: `<key>Label</key> au.com.inlight.lazyspec`, `<key>ProgramArguments</key>` w/ `lazyspec`/`daemon`, `<key>WorkingDirectory</key>`, `<key>RunAtLoad</key> <true/>`, `<key>KeepAlive</key> <true/>`.
   - Note: no `lazyspec daemon stop` — use `systemctl stop` / `launchctl unload` / `kill`.
   - Update CLI usage section (line 94) to mention `lazyspec daemon` cmd exists.

6. **Tests** — all ACs. New `tests/cli_daemon_test.rs`. Helper `spawn_daemon(workspace) -> Child` (waits for sock readiness w/ 2s timeout). Helper `kill_signal(child, sig)`. Use `nix = "0.27"` for kill if not already in deps — check; else `libc::kill` direct (one-liner, no dep). Verify: `grep nix Cargo.toml`. If absent, use libc. Test names: `daemon_blocks_until_signal`, `daemon_sigterm_releases_leases_and_unlinks_sock`, `daemon_sigint_same_path_as_sigterm`, `daemon_binds_and_listens`, `daemon_replaces_stale_socket`, `daemon_refuses_second_instance`, `daemon_does_not_fork_or_pidfile`, `daemon_deployment_samples_present_in_readme`.

7. **Validation + smoke** — Run `cargo build`, `cargo test --test cli_daemon_test`. Run `cargo run -- daemon` in temp workspace, ctrl-c, verify clean exit + no leftover sock.

## Notes

ADR-worthy:
- Single-instance enforced via UDS connect-probe (one mechanism: ECONNREFUSED→stale, OK→live). Alt: flock on `.lazyspec/daemon.lock`. Rejected — adds a file, two mechanisms, no win. Probe matches stale detection (one code path, two outcomes).
- Sync runtime, no tokio. Repo is fully sync (no `tokio` in Cargo.toml). Adding tokio for one slice would force migration elsewhere or split the codebase. Use std `UnixListener` + dedicated accept thread + `crossbeam_channel` (already a dep) for shutdown signalling. Revisit if slice 6 IPC pressure forces async.
- `signal-hook` over `ctrlc`: handles SIGTERM (ctrlc is SIGINT-only on some platforms). Sync, ecosystem norm.
- host_id distinct from agent_id: host_id is per-machine durable (`.lazyspec/daemon-host-id` UUID + gethostname). agent_id is per-session. Lease `agent` field = `{host}:{session}` per RFC-041 §"Lease ownership and orphan recovery".
- Drain in v1 is a no-op hook (`fn drain(&self) -> Result<()> { Ok(()) }`). Later slices (4, 6) hang inflight tracking onto it.
- "No fork" tested via parentage assertion, not syscall-absence. Pragmatic; covers the operationally-visible failure mode (daemonizing into init).
- Trait `LeaseReleaser` introduced ONLY to enable unit tests w/o real git (dictum 4 seam, not premature abstraction per dictum 6 — single prod impl, single mock, exact match).

