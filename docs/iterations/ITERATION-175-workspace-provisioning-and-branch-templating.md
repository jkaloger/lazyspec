---
title: Workspace provisioning and branch templating
type: iteration
status: accepted
author: agent
date: 2026-05-13
tags: []
related:
- implements: STORY-127
---

## In Scope

STORY-127 AC group A. Branch name template render+sanitize. Worktree provision: first-claim, reuse-existing-ref, refresh-on-missing-ref. New crate dep: `minijinja`. New engine modules: `branch_template`, `workspace`. Config plumbing for `[orchestration]` keys: `branch_template`, `workspace_root`, `base_branch`.

## Out of Scope

- AgentRunner trait, ClaudeP stream-json (group B, AC1-6).
- Hook runner lifecycle (group C, AC11-16).
- Prompt content rendering (slice 5).
- Tick loop, lease (slice 4).
- IPC streaming, TUI surfaces, metadata ref persistence.

Existing `src/engine/template.rs` string-sub stays. Branch templating new module via minijinja. Prompt template migration slice 5 problem.

## Acceptance Criteria

**AC7: First claim provisions a worktree from the configured base branch**

Given no local branch ref for rendered branch name
When runtime provisions workspace for claim
Then new git worktree created from configured base branch (default `origin/main`) at claim-scoped path

**AC8: Existing local branch is reused without rewind**

Given local branch ref for rendered branch name exists
When runtime provisions workspace for claim against that branch
Then worktree attached to existing ref, commit history intact (no reset to base, no rewind)

**AC9: Missing local branch ref triggers fresh worktree from base**

Given local branch ref for rendered branch name deleted since previous run
When runtime provisions workspace for claim
Then fresh worktree created from configured base branch, as if first claim

**AC10: Branch names are templated and sanitized**

Given branch name template configured w/ placeholders `iteration_id`, `iteration_slug`, `agent_id`, `story_id`, `date`
When runtime resolves branch name for claim
Then template rendered via minijinja w/ strict-undefined + sandboxed eval, output sanitized via `git check-ref-format --branch`, sanitized value used as worktree branch

## Test Plan

Per DICTUM-004: real git via TempDir, no mocks for git seam.

**AC7 test**: TempDir bare repo + working clone. No pre-existing branch `agents/STORY-127`. Call `provision_workspace`. Assert: worktree dir exists, `git -C <worktree> rev-parse HEAD` == base branch tip, `git worktree list` lists worktree path.

**AC8 test**: TempDir w/ pre-existing branch `agents/STORY-127` w/ commit ahead of base. Call `provision_workspace`. Assert: worktree HEAD == pre-existing branch tip (not base), no rewind, branch still has its extra commit.

**AC9 test**: TempDir, create branch ref, delete it (`git branch -D`), call `provision_workspace`. Assert: branch recreated from base tip, worktree HEAD == base tip.

**AC10 render test**: fixed `BranchVars { iteration_id: "ITERATION-175", iteration_slug: "workspace", agent_id: "claude", story_id: "STORY-127", date: "2026-05-13" }`, template `"agents/{{ story_id }}/{{ iteration_slug }}"` -> `"agents/STORY-127/workspace"`. Deterministic.

**AC10 strict-undefined test**: template `"{{ missing }}"` -> render error (not empty/silent).

**AC10 sandbox test**: template `"{{ self.__class__ }}"` or similar introspection -> render error.

**AC10 sanitize test**: render produces `"agents/with space"` or `"agents/.."`, run sanitizer, assert error (matches `git check-ref-format --branch` reject). Render produces valid `"agents/STORY-127"`, sanitizer returns unchanged.

**Integration**: render -> sanitize -> provision pipeline w/ valid vars produces working worktree at expected path on expected branch.

## Changes

### Task 1: Add minijinja dep + branch_template module (AC10)

ACs: AC10 (render half).

Files:
- `Cargo.toml`: add `minijinja = { version = "2", default-features = false, features = ["builtins"] }`. No loader/serde-json features needed.
- `src/engine/branch_template.rs` (new): pub struct `BranchVars { iteration_id, iteration_slug, agent_id, story_id, date: String }`. Pub fn `render_branch_name(template: &str, vars: &BranchVars) -> Result<String>`. Build minijinja `Environment` w/ `set_undefined_behavior(UndefinedBehavior::Strict)`. Add template, render w/ context from vars. Map minijinja errors to crate error type.
- `src/engine/mod.rs`: `pub mod branch_template;`.

Impl notes: minijinja default env already sandboxed (no fs/network). Strict undefined set explicitly. Vars serialized via `context!` macro or `BTreeMap<&str, &str>`.

Verify: `cargo test branch_template`, `cargo clippy`.

### Task 2: Branch name sanitizer (AC10)

ACs: AC10 (sanitize half).

Files:
- `src/engine/branch_template.rs`: add pub fn `sanitize_branch_name(name: &str) -> Result<String>`. Spawn `git check-ref-format --branch <name>`. Exit 0 -> return name as-is. Non-zero -> error w/ stderr captured.

Impl notes: direct `std::process::Command`. No trait seam needed yet — git binary assumed available (same assumption as rest of codebase, see `engine/git*` modules). Future: if multiple call sites need fake, extract `RefValidator` trait.

Verify: `cargo test sanitize`, `cargo clippy`.

### Task 3: Workspace provisioning (AC7, AC8, AC9)

ACs: AC7, AC8, AC9.

Files:
- `src/engine/workspace.rs` (new): pub struct `Workspace { pub path: PathBuf, pub branch: String }`. Pub fn `provision_workspace(repo_root: &Path, workspace_root: &Path, base_branch: &str, branch: &str, claim_id: &str) -> Result<Workspace>`.
- `src/engine/mod.rs`: `pub mod workspace;`.

Logic:
1. Resolve worktree path: `workspace_root.join(claim_id)`.
2. Check local branch ref: `git -C repo_root rev-parse --verify --quiet refs/heads/<branch>`.
3. If ref exists: `git -C repo_root worktree add <worktree_path> <branch>` (attaches existing) — AC8.
4. If ref missing: `git -C repo_root worktree add -b <branch> <worktree_path> <base_branch>` — AC7 first claim, AC9 fresh after deletion (same path; deletion already removed ref).
5. Return `Workspace { path, branch }`.

Pre-cleanup: if `<worktree_path>` already registered (stale), prune via `git worktree remove --force` or error w/ guidance. Decide: v1 errors out — operator removes. Note in iteration `## Notes`.

Verify: `cargo test workspace`, `cargo clippy`.

### Task 4: Config plumbing (AC7, AC10)

ACs: AC7 (base branch from config), AC10 (template from config).

Files:
- `src/config.rs` (or wherever `Config` lives — locate via `ast-grep -p 'struct Config'`): add `OrchestrationConfig { branch_template: String, workspace_root: PathBuf, base_branch: String }`. Defaults: `branch_template = "agents/{{ story_id }}"`, `workspace_root = ".lazyspec/work"`, `base_branch = "origin/main"`. Serde `#[serde(default)]` on each.
- Existing `[orchestration]` section (if present from prior iters) extended, not replaced.

Verify: `cargo test config`, `cargo clippy`. Toml round-trip test if existing pattern.

## Notes

- Minijinja chosen per RFC-041 design intent: Jinja2 dialect, strict-undefined catches typos at render not at git, sandboxed eval blocks template-injection escalation.
- Worktree-only v1: no cleanup/teardown logic this iter (group C / later slice). Stale worktree path errors out w/ operator-facing message.
- No rewind policy: AC8 explicit — never reset existing branch. Operator deletes ref to force fresh (AC9).
- Sanitizer behavior delegated to `git check-ref-format --branch` — single source of truth, matches what worktree add will accept anyway.
- Git seam: direct `Command` per existing engine pattern. No trait abstraction until second consumer needs fake.
- `engine/template.rs` string-sub stays for now. Migration to minijinja for prompt rendering = slice 5 problem (dictum 6: second-use refactor; prompt rendering will be that second use).
