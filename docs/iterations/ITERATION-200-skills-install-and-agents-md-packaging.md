---
title: Skills install and AGENTS.md packaging
type: iteration
status: complete
author: agent
date: 2026-06-21
tags: []
related:
- implements: STORY-147
---

## Changes

Group B of STORY-147: INSTALL + PACKAGING. Take the skill set ITERATION-199
authors and place it. Placement, not transformation (RFC-048): same prose
becomes `.claude/skills/` for Claude and `AGENTS.md` for other agents. Each task
self-contained.

### 1. Add the `[skills] entry` config section (parse + default + write)

`[skills]` is a new top-level config section, sibling to `[agents]`. One key:
`entry` (string), names the router skill. Default `lazy`.

Verified paths:
- `@ref src/engine/config.rs#Config` (config.rs:260) — `agents: AgentsConfig`
  (config.rs:282-283) is the model: `#[serde(default)]` field on `Config`.
- `@ref src/engine/config.rs#AgentsConfig` (config.rs:351-355) — the
  zero-default-section pattern to mirror (`#[derive(Default)]`, one optional
  field).
- `RawConfig` (config.rs:357-375) — `agents: Option<AgentsConfig>`
  (config.rs:374); add `skills` the same way.
- `parse_inner` (config.rs:520) — final `Config{}` build (config.rs:596-618)
  does `agents: raw.agents.unwrap_or_default()` (config.rs:617); add `skills`
  beside it.
- `#[cfg(test)] Default for Config` (config.rs:463-489) — add the field.
- `starter_config` (`@ref src/cli/init.rs#starter_config`, init.rs:13-37) —
  the `init` builder; add the field so a fresh config carries the default.

Impl:
- New `SkillsConfig { entry: String }`, `#[derive(Serialize, Deserialize)]`.
  `entry` is `#[serde(default = "default_skills_entry")]` where the default
  returns `"lazy"`. Implement `Default` so `Default::default()` yields
  `entry = "lazy"` (so `unwrap_or_default()` produces the documented default).
- Add `#[serde(default)] pub skills: SkillsConfig` to `Config`.
- Add `skills: Option<SkillsConfig>` to `RawConfig`; in `parse_inner`,
  `skills: raw.skills.unwrap_or_default()`.
- Add `skills: SkillsConfig::default()` to the test `Default` and to
  `starter_config`.

A `[skills]` section is non-load-bearing for any existing path (zero-default,
ADR-015 style like `[agents]`), so pre-existing configs without it still load —
`unwrap_or_default()` fills `entry = lazy`.

ACs: foundation for AC4 (default `lazy`) and AC5 (custom name).

Verification: `cargo test -p lazyspec config` parse round-trips a config with
`[skills]\nentry = "x"` -> `entry == "x"`; a config with no `[skills]` ->
`entry == "lazy"`. `cargo run --quiet -- validate --json` on this repo stays
green (new optional section, no validation impact).

### 2. Embed the default skill set in the binary

`skills install` must work in any project, so the skill source ships inside the
binary — same intent as `default_template` (`@ref src/engine/fs_ops.rs#default_template`,
fs_ops.rs:49-140), which holds default doc templates as inline Rust string
literals fallback. The skill set is a directory tree (`<verb>/SKILL.md` +
`_common.md` + a router skill), not a single string, so embed the tree.

Verified paths:
- `@ref src/engine/fs_ops.rs#load_template` (fs_ops.rs:14-23) +
  `default_template` (fs_ops.rs:49-140) — the on-disk-then-embedded-fallback
  precedent to mirror in spirit.
- `skills/` (repo root) and `examples/spec/skills/` — existing skill-set shape:
  a dir of `<name>/SKILL.md`, a shared `_common.md`, and a `lazy/SKILL.md`
  router with `name:`/`description:` frontmatter. ITERATION-199 replaces this
  with the generic verb set; this task embeds whatever 199 produces.
- No `include_str!` / `include_dir` / `rust-embed` exists in `src/` today —
  this introduces the first embedded asset tree.

Impl decision (embed how):
- Source of truth lives at a fixed repo path (e.g. `assets/skills/`), authored
  by ITERATION-199, embedded at build time.
- Use `include_dir` (small crate) for the multi-file tree, OR a hand-written
  `&[(&str, &str)]` table of `(relative_path, include_str!(...))` entries if
  avoiding a new dependency is preferred. Both mirror `default_template`'s
  "binary carries the default" intent; `include_dir` is less boilerplate for a
  tree. Pick `include_dir` unless the dep is unwanted, then fall back to the
  `include_str!` table.
- Expose `fn embedded_skill_set() -> impl Iterator<Item = (PathBuf, &str)>`
  (relative path under the set root -> file contents) so the install task is
  agnostic to the embedding mechanism.
- The router skill's filename is `<entry>/SKILL.md`; embed it under a stable
  key (e.g. `lazy/SKILL.md`) and let the install task rename the directory to
  the configured `entry` (task 3, AC5).

ACs: prerequisite for AC4/AC5/AC6 (install has files to place even with no
on-disk source).

Verification: a unit test asserts `embedded_skill_set()` is non-empty and
contains the router skill entry. (Set content is 199's concern; this asserts
the embedding wiring, not the prose.)

### 3. The `skills install` subcommand

New top-level command `skills install [--runtime claude|agents-md]`. Placement
of the embedded set under `.claude/skills/` (Claude) and concatenation into
`AGENTS.md` (other agents), plus setting `[skills] entry`. Decoupled from
`init`.

Verified paths:
- `@ref src/cli.rs#Commands` (cli.rs:66-340) — the clap subcommand enum. Add a
  `Skills { #[command(subcommand)] command: SkillsCommand }` variant. The
  nested-subcommand pattern is `Reservations`/`Provenance`
  (cli.rs:279-288) delegating to `ReservationsCommand` /
  `ProvenanceCommand` (`@ref src/cli/reservations.rs`,
  `@ref src/cli/provenance.rs`).
- `src/main.rs` dispatch (main.rs:81-481) — the match over `Commands`. The
  `Reservations`/`Provenance` arms (main.rs:354-400) show the nested-subcommand
  dispatch shape.
- `init` special-casing (main.rs:21-24) and `fix --config`
  (main.rs:68-77) — both dispatch **before** `Config::load(&cwd, &fs)`
  (main.rs:79). `skills install` must do the same so it runs in a project with
  no `.lazyspec.toml` (AC6).
- `@ref src/engine/config_write.rs#write_config_in_place` (config_write.rs:9-26)
  — in-place TOML edit used to set `[skills] entry` when a config exists.
- `write_agents` (config_write.rs:304-309) — sets a scalar into an existing
  section; `write_certification` (config_write.rs:231-261) — fabricates an
  absent section. Combine: fabricate `[skills]` if absent, then set `entry`.
- `ensure_gitignore` (`@ref src/cli/init.rs`, init.rs:106-138) — the
  read-existing / append-if-absent file pattern AGENTS.md concatenation
  mirrors.

Impl:
- `SkillsCommand::Install { runtime: Option<Runtime>, ... }` where `Runtime` is
  a clap `ValueEnum { Claude, AgentsMd }`. Absent `--runtime` means do both
  (AC4 places skills AND AGENTS.md).
- New `src/cli/skills.rs` (declared in `src/cli.rs` mod list, cli.rs:1-24).
- Resolve the entry name: if `.lazyspec.toml` exists, read `[skills] entry`
  (default `lazy` from task 1); else use the default `lazy`. This is what lets
  AC5's pre-configured custom name drive the install.
- Claude placement: for each `(rel_path, contents)` from `embedded_skill_set()`,
  write under `.claude/skills/`. The router skill's directory is renamed from
  the embedded `lazy/` to `<entry>/` so `<entry>/SKILL.md` carries `name: <entry>`
  — invoking the custom name dispatches the router (AC5). `mkdir -p` parents;
  overwrite-or-skip behavior documented (idempotent re-install: overwrite).
- AGENTS.md concatenation: concatenate the same set's prose into `AGENTS.md` at
  project root (one markdown file; verb sections joined with separators). Same
  prose, no per-runtime transform (RFC-048 "skill == AGENTS.md").
- Set `[skills] entry` to the resolved name: if a config exists, edit in place
  via `write_config_in_place` (write task in config_write, see below); if no
  config exists (AC6), still place files — recording `entry` in config is a
  no-op when there's no config to record into (the default IS `lazy`, and a
  custom name with no config is out of scope: AC5 says "with that entry
  configured", which requires a config). Document: install never creates
  `.lazyspec.toml`; it writes config only if one is present.
- `--runtime claude` -> skills only; `--runtime agents-md` -> AGENTS.md only;
  none -> both.

Config-write task (in `@ref src/engine/config_write.rs`):
- Add `write_skills(doc, buffer)` mirroring `write_certification`
  (config_write.rs:231-261): fabricate `[skills]` if absent and `entry`
  differs from default, then `set_str_defaulted(skills, "entry", entry, "lazy")`
  (the `set_str_defaulted` helper at config_write.rs:447). Call it from
  `write_config_in_place` (config_write.rs:9-24) alongside `write_agents`.

README: add `skills install` to the CLI surface and refresh the `## Skills`
table (README.md:79-92) — the hard-coded `write-rfc`/`create-story`/... list is
superseded by the generic verb set + `install` (CLAUDE.md: update README when
CLI changes).

ACs: AC4 (no skills installed -> places set + AGENTS.md, entry = `lazy`),
AC5 (custom entry configured -> router installed under that name, entry
recorded), AC6 (no `init` -> still places skills + AGENTS.md).

Verification: see Test Plan. Manually: in a temp dir with no `.lazyspec.toml`,
`cargo run --quiet -- skills install` creates `.claude/skills/` + `AGENTS.md`
and exits 0 (AC6). `cargo run --quiet -- skills install --help` lists
`--runtime`. README documents the command.

## Test Plan

No test code here — author against ACs in implementation.

### AC4 — install places files + sets default entry

Given a project with no skills installed (`.claude/skills/` absent), when
`skills install` runs (no `--runtime`):
- `.claude/skills/` exists and contains the router skill at `<lazy>/SKILL.md`
  plus the verb skills (assert the dir is populated from the embedded set).
- `AGENTS.md` exists at project root and contains the concatenated prose
  (assert non-empty and contains a known verb section marker).
- `[skills] entry` reads `lazy` (when a config is present): parse the config
  after install, assert `skills.entry == "lazy"`.
- Idempotent: a second `install` succeeds and leaves the same result.

### AC5 — custom entry

Given a config with `[skills] entry = "go"` (or any non-default name), when
`skills install` runs:
- The router skill is installed under `.claude/skills/go/SKILL.md` (directory
  named for the entry), and its `name:` frontmatter is `go`.
- The config's `[skills] entry` still reads `go` after install (recorded /
  preserved).
- The other verb skills are placed unchanged (only the router is renamed).

### AC6 — install without init

Given a project that never ran `init` (no `.lazyspec.toml`), when
`skills install` runs:
- It exits 0 (does NOT error with "no .lazyspec.toml found" — proving dispatch
  happens before `Config::load`).
- `.claude/skills/` and `AGENTS.md` are placed using the default entry `lazy`.
- No `.lazyspec.toml` is created as a side effect.

### Runtime selection

- `--runtime claude` -> `.claude/skills/` placed, `AGENTS.md` NOT created.
- `--runtime agents-md` -> `AGENTS.md` created, `.claude/skills/` NOT created.

## Notes

Depends on ITERATION-199 (authors the generic verb skill set + `lazy` router
this iteration embeds and installs) and on task 1 of this iteration (the
`[skills] entry` key). Sibling within STORY-147, group B; sequence is 199 then
200.

Boundary with 199: 199 owns the skill *prose* (verb authoring, ceiling logic,
router dispatch); 200 owns *placement* (embedding, `.claude/skills/`,
`AGENTS.md`, `[skills] entry`). 200 treats the set as opaque files.

Embedding mechanism (task 2) is the one open design choice: `include_dir` crate
(less boilerplate, new dep) vs a hand-written `include_str!` table (no dep,
mirrors `default_template` exactly). Decide at implementation; the
`embedded_skill_set()` iterator boundary keeps task 3 agnostic either way.

AC5's "custom name with no config" is intentionally out of scope: AC5 reads
"with that entry configured", which presupposes a `.lazyspec.toml`. Install
never creates a config; it records `entry` only into an existing one.
