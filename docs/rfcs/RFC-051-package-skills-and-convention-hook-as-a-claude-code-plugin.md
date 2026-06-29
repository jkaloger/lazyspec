---
title: Package skills and convention hook as a Claude Code plugin
type: rfc
status: accepted
author: unknown
date: 2026-06-29
tags: []
related: []
provenance:
- 'Plugin mechanics verified against Claude Code docs: code.claude.com/docs/en/plugins-reference, plugin-marketplaces, hooks (2026-06-29)'
---## Summary

Package the lazyspec agent integration -- the on-disk verb skills under `skills/` plus the convention-preamble hook -- as a first-class Claude Code plugin. The plugin manifest lives at the repository root (`.claude-plugin/plugin.json`), reuses the existing `skills/` directory verbatim, adds a guarded `hooks/hooks.json` that injects the project convention on every prompt, and is published through a same-repo marketplace (`.claude-plugin/marketplace.json`). Users install with `/plugin marketplace add jkaloger/lazyspec` then `/plugin install`, instead of running `lazyspec skills install` and hand-editing `.claude/settings.json`.

## Motivation

Today the two halves of lazyspec's Claude Code integration are installed by different, manual paths. Skills arrive through `lazyspec skills install`, which embeds eight verb skills at build time and writes them into a project's `.claude/skills/` (or concatenates into `AGENTS.md`). The convention hook is not installed by anything: it exists only because this repo hand-wrote a `UserPromptSubmit` entry into `.claude/settings.json` calling `lazyspec convention --preamble`. A new user wiring up lazyspec must discover both mechanisms, run one command, and hand-edit settings JSON for the other.

Claude Code's plugin system is the distribution channel built for exactly this bundle: skills, hooks, and commands shipped together, version-pinned, installed and updated with one command, discoverable through a marketplace. Packaging the integration as a plugin collapses two manual steps into a single supported install, makes updates a `/plugin` operation rather than a re-run-and-merge, and gives the integration a stable public surface other repos can depend on.

Without this, the convention hook stays undocumented tribal knowledge, the skills install path remains the only entry point, and there is no version-pinned artifact for downstream projects to track.

## Goals

- A user with the `lazyspec` binary on PATH can install the full integration (all on-disk skills + convention hook) with `/plugin marketplace add` followed by `/plugin install`, no manual JSON editing.
- The plugin reuses the existing root `skills/` directory as its skill source; no second copy of skill prose to keep in sync.
- The convention hook injects `lazyspec convention --preamble` output on `UserPromptSubmit` and stays silent (no error, no blocked prompt) in any project lacking a `.lazyspec.toml`.
- The marketplace is hosted in the lazyspec repo itself, so a single `git` source serves both the tool and its plugin.
- `lazyspec skills install` continues to work unchanged for AGENTS.md targets and custom per-project router-entry naming.

## Non-goals

- Replacing or deprecating `lazyspec skills install`. The plugin is an additional channel; the CLI install stays for AGENTS.md output and custom `[skills] entry` renaming, which a static plugin cannot do.
- Bundling the lease-coordination hooks (`SessionStart`/`PostToolUse`/`SessionEnd` claim/heartbeat/release) from `hooks/claude-code-settings.json`. Those stay a separate, opt-in reference.
- Shipping or installing the `lazyspec` binary itself. The plugin assumes it is already on PATH; the hook degrades to a noop when it is absent.
- Per-project router-entry renaming (the `[skills] entry` rewrite done by `skills install`). The plugin ships the default `lazy` entry.
- Reconciling the build-time embedded skill set (8 skills) with the on-disk set (10). The plugin ships whatever lives in `skills/`; the embedded-set divergence is noted as a risk and left to a follow-up.

## Design

### Plugin manifest at repo root

Add `.claude-plugin/plugin.json` at the repository root. Only `name` is required; we also declare `description`, `version`, and `author`. With the manifest at the root, Claude Code's plugin loader auto-discovers components relative to the root (not relative to `.claude-plugin/`): the existing `skills/` directory and a new `hooks/hooks.json`. No `commands/` or `agents/` are declared in this iteration. Plugin name: `lazyspec`.

The plugin's skill set is exactly the contents of root `skills/`. Auto-discovery scans `skills/<name>/SKILL.md`, so the loose `skills/README.md` and `skills/MIGRATION-2026-06-23.md` files are ignored (no `SKILL.md`), and all ten skill directories ship: `lazy`, `scaffold`, `co-write`, `generate`, `advance`, `execute`, `review`, `systematic-debugging`, `configure-type`, `create-audit`. This is broader than the eight-skill embedded set used by `skills install`; the plugin deliberately ships the full on-disk set rather than maintain a curated subdirectory.

### Convention hook via hooks/hooks.json

Add `hooks/hooks.json` registering a single `UserPromptSubmit` hook. The hook's stdout is injected into the model's context, which is exactly how the convention preamble should reach the agent. The command must not error or block when run outside a lazyspec project: `lazyspec convention --preamble` exits 1 with a "no .lazyspec.toml found" message in an unconfigured directory (verified). The hook therefore guards the call:

```
lazyspec convention --preamble 2>/dev/null || true
```

On success, the preamble flows to stdout and is injected. With no config, stderr is suppressed, the `|| true` forces exit 0, stdout is empty, and nothing is injected -- a clean noop. The command runs under `/bin/sh`, where this construct is portable.

The existing `hooks/claude-code-settings.json` (lease coordination) is a different filename and is not auto-loaded by the plugin loader, so the two coexist without collision.

### Same-repo marketplace

Add `.claude-plugin/marketplace.json` declaring a marketplace named `lazyspec`. Required fields: `name`, `owner` (an object with `name`, here `jkaloger`), and `plugins` (an array). The single plugin entry uses `source: "."` -- the plugin *is* the marketplace root -- which is what lets it reuse the root `skills/` and `hooks/`. Because a `plugin.json` also lives at that same root, the entry must set `strict: false`; otherwise the loader treats the marketplace entry as the sole component authority and ignores the root `plugin.json`. Both `marketplace.json` and `plugin.json` therefore coexist under `.claude-plugin/`, each serving its distinct role.

Install flow:

```
/plugin marketplace add jkaloger/lazyspec
/plugin install lazyspec@lazyspec
```

Hosting the marketplace in the lazyspec repo keeps a single `git` source of truth; no separate marketplace repository to maintain.

The alternative spec-idiomatic layout -- `marketplace.json` at root listing a plugin in a `plugins/lazyspec/` subdirectory via a relative `source` -- is rejected: a subdir plugin cannot reuse the root `skills/` without a copy or symlink, which violates the single-source goal.

### Coexistence with skills install

`lazyspec skills install` is untouched. It remains the path for AGENTS.md output and for projects that rename the router entry via `[skills] entry`. A project may use either channel; the plugin is the recommended path for Claude Code users who want skills + convention hook together with one command.

## Interfaces

New files (all proposed, `@draft`):

- `.claude-plugin/plugin.json` -- plugin manifest. Required: `name` (`lazyspec`). Also set: `description`, `version`, `author`.
- `.claude-plugin/marketplace.json` -- marketplace named `lazyspec`. Required: `name`, `owner` (`{ "name": "jkaloger" }`), `plugins` (array). The single entry sets `source: "."` and `strict: false` (the latter required because a root `plugin.json` coexists).
- `hooks/hooks.json` -- top-level `{ "hooks": { "UserPromptSubmit": [ { "hooks": [ { "type": "command", "command": "..." } ] } ] } }`; the matcher is omitted (matches all). The command is the guarded `lazyspec convention --preamble 2>/dev/null || true`. No `${CLAUDE_PLUGIN_ROOT}` needed -- the command invokes the on-PATH binary, not a bundled script.

Install: `/plugin marketplace add jkaloger/lazyspec` then `/plugin install lazyspec@lazyspec`.

No Rust CLI signatures change. `lazyspec convention --preamble` and `lazyspec skills install` keep their current behavior. The README gains a "Install as a Claude Code plugin" section alongside the existing `skills install` documentation.

## Decisions (ADRs to emit)

- **Plugin as an additional distribution channel, not a replacement for `skills install`.** The CLI install retains capabilities a static plugin lacks (AGENTS.md target, custom entry renaming); the two coexist.
- **Reuse root `skills/` as the plugin skill source rather than a CLI-generated bundle.** Avoids a second copy and a new generator command; accepts that the plugin ships the full on-disk set (10) while `skills install` ships the embedded set (8).
- **Guard the convention hook in the shell rather than hardening the CLI.** `2>/dev/null || true` keeps the hook silent in non-lazyspec repos with no Rust change; CLI hardening (exit 0 + empty output when no config) is the alternative, deferred.
- **Marketplace and plugin manifest coexist at repo root via `source: "."` + `strict: false`.** This is the layout that lets the plugin reuse the root `skills/` and `hooks/`. The spec-idiomatic alternative (marketplace at root, plugin in a `plugins/` subdir with relative source) is rejected because it forces a copy of the skill sources.

## Stories

1. **Plugin shell.** Add `.claude-plugin/plugin.json` + `.claude-plugin/marketplace.json`; verify `/plugin marketplace add` and `/plugin install` load the skills from root `skills/`. (No dependency.)
2. **Convention hook.** Add `hooks/hooks.json` with the guarded `UserPromptSubmit` command; verify injection in a lazyspec project and clean noop in a non-lazyspec project. (Depends on 1 for the manifest.)
3. **Docs.** Add the plugin install section to README, alongside `skills install`; note the lazyspec-binary prerequisite. (Depends on 1, 2.)
4. **End-to-end validation.** Install the plugin into a scratch project, confirm all ten skills resolve and the preamble fires on prompt submit. (Depends on 1, 2.)

## Risks and tradeoffs

- **Binary is a runtime dependency.** The plugin does not install `lazyspec`; the hook is inert if the binary is absent from PATH. Mitigated by the guard (silent noop) and documented as a prerequisite. Accepted cost: a freshly installed plugin does nothing useful until the binary is present.
- **Skill-set divergence.** The plugin ships the on-disk ten; `skills install` ships the embedded eight (missing `configure-type`, `create-audit`). Two channels can hand a user different skill sets. Noted for a follow-up that reconciles the embedded set; out of scope here.
- **Preamble injected on every prompt.** In any project with a `.lazyspec.toml`, the convention preamble is added to context on each `UserPromptSubmit` -- a per-prompt token cost. This is the current behavior in this repo, accepted as the point of the hook.
- **No custom entry naming through the plugin.** Projects that rename the router entry must still use `skills install`. Accepted: the plugin targets the common default-entry case.
- **Shell-guard portability.** The `|| true` guard assumes a POSIX `/bin/sh`. Claude Code runs shell-form hook commands (no `args` field) under `/bin/sh -c`, so this holds; hardening the CLI remains the fallback if that assumption ever breaks.
- **`source: "." ` + `strict: false` is a less-trodden configuration.** Most marketplaces point at subdir plugins with relative sources. The root-as-plugin path is documented and supported, but Story 4's end-to-end validation must confirm the loader picks up the root `skills/` and `hooks/hooks.json` under this config before the RFC is accepted.
