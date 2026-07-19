---
title: Interactive init bootstrap from the starter config
type: iteration
status: complete
author: jkaloger
date: 2026-07-20
tags: []
related:
- implements: STORY-227
---
<!-- intent: plan the concrete changes that satisfy a story's acceptance criteria -->

Implements STORY-227 (RFC-062 bootstrap wizard, "start from starter + tweak" path). ONE slice: `init` on TTY, no flags -> walk author + naming + per-starter-type keep/drop + add-type, then run existing scaffold seeded from designed `Config`. From-scratch/blank-slate DAG designer = STORY-228, EXCLUDED.

## Objective

`lazyspec init` on TTY prompts user to tweak `starter_config()` (author, naming, keep/drop starter types, add types) before scaffold; non-TTY/`--json`/`--non-interactive` = today's behaviour byte-for-byte.

## Satisfies

STORY-227 all 5 AC. (blank-slate designer NOT an AC here — STORY-228.)

## Context (file:line)

- `Init` = unit variant, no args (`src/cli.rs:75`). Dispatch early-returns `init::run(&cwd)` (`src/main.rs:25-28`) BEFORE `Config::load` — no config file yet, same class as Completions/Fix/Skills.
- `init::run` (`src/cli/init.rs:42-66`): bail if `.lazyspec.toml` exists (44-46); `config = starter_config()` (48); mkdir per type (50-52); templates dir + `template.md` (53-55); `scaffold_skeleton_files` (57); write toml (59); `ensure_github_labels` (61); `ensure_gitignore` (62). Scaffold logic is INLINE in `run` — must extract to reuse.
- `starter_config()` (`src/cli/init.rs:14-40`): `starter_types()`, naming `{type}-{n:03}-{title}.md` (NOTE `.md` suffix; story says `{type}-{n:03}-{title}` — starter's real default wins for parity), `starter_relationships()`, `default_rules()`.
- Prompter seam DONE: `trait Prompter{ask,confirm,select}` + `StdinPrompter` + `ScriptedPrompter` (`src/cli/wizard.rs:7-139`). REUSE, add nothing.
- Add-type interactive collection loop (`src/cli/config.rs:197-416`): reads `.lazyspec.toml` from disk first (202-204) THEN prompts fields (214-389). Disk-read makes it un-reusable from `init` (no file yet) as-is — extract the prompt-only collection.
- Git author default: `git config user.name` reader exists at `src/engine/agent.rs:32-45` — reuse for author prefault.
- No `default_author` field on `Config`/`DocumentConfig` (`src/engine/config.rs:477-482`, `Naming` 667-668). Author has NO config home. See Notes.
- Prior slice pattern to mirror: `ITERATION-325` (add-type TTY wizard), `ITERATION-326` (STORY-226 attrs/lifecycle/gate).

## Approach

Split prompting (pure, testable) from scaffolding (IO). Three seams:

1. `Init { non_interactive: bool, json: bool }` (was unit). `--non-interactive` + `--json` flags. `--json` implies non-interactive (RFC-062 §json).
2. `design_config_interactive(base: Config, prompter: &mut dyn Prompter) -> Result<Config>` in `src/cli/init.rs` — PURE, no disk. Start from `base = starter_config()`; prompt author (prefault git `user.name`), naming pattern (default `base.documents.naming.pattern`), per starter type keep/drop, then add-type loop; return mutated `Config`. Accept-all-defaults => returns `base` unchanged (parity).
3. `write_project(root, &Config) -> Result<()>` — extract `init::run` body lines 50-62 verbatim. Both `run` (non-interactive) and interactive path call it. ONE scaffold path, ONE config writer (Convention P6).
4. Reuse add-type flow: extract `collect_type_interactive(cfg: &Config, prompter) -> Result<CollectedType>` from `config.rs:214-389` (prompt-only, in-memory cfg for dup/parent/gate checks, NO disk). `run_add_type_interactive` keeps disk read then delegates; init wizard calls it against the in-progress `Config` and applies result in-memory. If extraction proves heavy, MINIMUM: reuse core-field collection (name/plural/dir/prefix/icon/store/numbering/singleton/authorship + dup guard). Lifecycle/attrs/gate reuse is bonus, not blocking.

## Task breakdown

1. `src/cli.rs:75`: `Init` -> struct variant `{ #[arg(long)] non_interactive: bool, #[arg(long)] json: bool }`.
2. `src/main.rs:25-28`: destructure `Init { non_interactive, json }`; `interactive = !non_interactive && !json && stdin().is_terminal() && stdout().is_terminal()` (mirror `main.rs:650-652`); still return early. If `.lazyspec.toml` exists -> `init::run` bails as today (keep the existence check inside run/write path). Interactive -> `StdinPrompter::new()` + `run_init_interactive`; else `init::run`.
3. `src/cli/init.rs`: extract `write_project(root, &Config)` from `run` body; `run` = `write_project(root, &starter_config())` after existence bail.
4. `src/cli/init.rs`: `design_config_interactive(base, prompter)` — author prompt (prefault git user.name), naming prompt, keep/drop loop over `base.documents.types` (confirm "Keep type <name>?" default y; dropped names removed), add-type loop (`confirm "Add another type"` -> collect -> push, dup name/prefix re-prompt), final `confirm "Write this config"` (default y) else re-loop or abort-clean.
5. `src/cli/init.rs`: `run_init_interactive(root, prompter)` = existence bail + `write_project(root, &design_config_interactive(starter_config(), prompter)?)`.
6. `src/cli/config.rs`: extract `collect_type_interactive` (prompt-only) shared by `run_add_type_interactive` + init wizard.
7. README: TTY-triggered interactive `init`, `--non-interactive` opt-out, `--json`/non-TTY unchanged (project rule: CLI change -> README).

## Acceptance criteria (each test-backed)

- AC1 walk: **Given** `ScriptedPrompter` queued [author, naming, keep-all, no-add, write=y], **When** `design_config_interactive(starter_config(), p)`, **Then** returns Config with prompted author-effect + naming, all starter types present. -> `test design_prompts_author_naming_types`.
- AC2 parity: **Given** `ScriptedPrompter` all-blank (accept every default), **When** `design_config_interactive(starter_config(), p)`, **Then** result `== starter_config()` (assert `to_toml()` byte-equal). -> `test design_all_defaults_equals_starter`.
- AC3 suppression: **Given** `--non-interactive` OR `--json` OR non-TTY, **When** init dispatches, **Then** no prompt, `write_project` receives `starter_config()` unchanged. -> `test init_noninteractive_writes_starter` (call `write_project(tmp, &starter_config())`, assert toml == starter) + dispatch-gate unit `test json_suppresses_interactive`.
- AC4 exists-bails: **Given** `.lazyspec.toml` present, **When** init (interactive or not), **Then** bails "already exists", no overwrite. -> `test init_bails_when_config_exists`.
- AC5 drop/add valid: **Given** `ScriptedPrompter` dropping one starter type + adding one new type, **When** design then `validate`, **Then** dropped type absent, new type present, `write_project` output passes `lazyspec validate`; dirs/templates/skeletons match designed types. -> `test design_drop_and_add` (Config assertions) + `test write_project_scaffold_validates` (temp dir round-trip + validate).

## Test plan

- All unit, `ScriptedPrompter`-driven, NO real TTY (mirror `config.rs:990+` scripted tests). Cover: all-defaults parity byte-equal, author+naming applied, keep-all, drop-one, add-one dup-name re-prompt, write-confirm=n path, exists-bail.
- Scaffold round-trip: temp dir, `write_project(tmp, &designed)`, assert per-type dirs + `template.md` + skeletons, then load + `validate` clean (reuse init.rs test style `src/cli/init.rs:232-302`).
- Dispatch gating unit for `--json`/`--non-interactive` suppression (no prompter constructed).

## Out of scope

- STORY-228 from-scratch / blank-slate full DAG designer (custom lifecycles-from-zero, relation-vocabulary authoring, first-screen "starter vs blank" choice). This slice ONLY tweaks the starter.
- Persisting a project default author: `Config` has NO `default_author` field (`config.rs:477`). Adding one = schema change AND breaks AC2 byte-parity (starter carries no author). This slice PROMPTS author (prefault git user.name) but does NOT write it to config — deferred pending schema work. Naming pattern IS persisted (real `Naming.pattern` field, default == starter -> parity holds).
- Editing lifecycle/gates of the KEPT starter types (RFC-062 non-goal: add not edit).
- Remote-store auth (`setup github-issues`/`clickup`) — `init` may already trigger labels via `ensure_github_labels`; no new auth flow.
- TUI/web: none — CLI-only authoring, produces standard `.lazyspec.toml` all layers already read (RFC-062 §parity).

## Notes

- Convention: prompt seam stays in CLI layer (P3); `Prompter` trait fake at seam (P4); non-TTY/`--json` -> unchanged non-interactive path (byte parity is AC2/AC3). NO second config writer — `write_project` is the single scaffold path (P6).
- Parity trap: starter naming pattern carries `.md` suffix (`init.rs:19`); default the naming prompt to `base.documents.naming.pattern`, NOT the story's `{type}-{n:03}-{title}` (missing `.md`), or blank-accept diverges from starter and breaks AC2.
- `run_add_type_interactive` reads disk first (`config.rs:202`); init has no file. Extract prompt-only `collect_type_interactive` operating on in-memory `Config` so both callers share it.
- Author gap is the one real design fork — resolved above (prompt, don't persist). Flag to reviewer if they want a `default_author` field (own story).
