---
title: 'User-defined hooks: external commands on lazyspec events'
type: rfc
status: draft
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- related-to: RFC-074
- related-to: RFC-071
- related-to: RFC-073
---

## Summary

Let a project attach its own commands to lazyspec events. A hook is an external program named in `.lazyspec.toml`. lazyspec sends it documents as JSON on stdin; it replies on stdout with findings and, where the event allows, body updates. The engine applies those updates through the store, so a hook never writes files or calls the CLI. Two events to start: `validate` and `pre-transition`. `lazyspec hook run` fires either by hand. Hooks from a repo's config run only after the user trusts them.

These are lazyspec hooks. They are unrelated to the agent-runtime hooks in RFC-071.

## Motivation

1. Validation stops at frontmatter and links. The built-in `Checker`s (`src/engine/validation.rs`) cover edges, IDs, refs, attributes and parts. Nothing checks the body: a required section missing, a heading convention broken, a checklist left unticked on a closed document. Every project has different rules, so none of them belong in the binary.
2. Status changes can't carry work. Moving a document to a status often means other documents should change too: roll child summaries into a parent, append a changelog entry, fold one document's content into another. Today that is an agent or a human remembering to do it, with no check that they did.
3. Workflow packs (RFC-074) ship types and templates but no behaviour. A pack can describe a method's documents, but not enforce or automate the method.

## Goals

- A project declares hooks in `.lazyspec.toml`, each bound to one event, optionally scoped by type and (for transitions) target status.
- `validate` hook findings appear in `lazyspec validate`, `validate --json`, `status`, and the TUI's validation display, looking the same as built-in findings.
- A `pre-transition` hook can block a status change with an error finding, or return body updates that are saved together with the new status: all or nothing.
- `lazyspec hook run <event> <id>` fires one event's hooks for one document without changing its status. `lazyspec hook list` shows the configured hooks and whether each is trusted. Both support `--json`.
- Updates go through the same engine path as `update --body` / `update --part`, so they work for every store backend.
- Hooks in a repo's config are skipped with a warning until the user trusts them.
- `init --template` copies a pack's `.lazyspec/hooks/` along with its templates.

## Non-goals

- Changing status or frontmatter from a hook. Updates are body and part content only. Status is the transition the hook runs inside.
- `post-*` events, `pre-create`, `pre-link`, and similar. Add each when there is a concrete use for it.
- Declarative body rules in config (e.g. "section X is required"). Revisit if packs keep shipping the same lint script.
- Embedded scripting (WASM, Lua, Rhai).
- Running hooks in the background or on file change.

## Design

### Events

As in git, the event a hook is bound to decides its whole contract. There is no separate kind.

| Event | Fires in | May return | Blocks on error |
|---|---|---|---|
| `validate` | `validate`, `status`, TUI validation refresh | findings | No. Findings are reported with their own severity. |
| `pre-transition` | `update --status`, TUI status change, `hook run` | findings, updates | Yes. Any error finding cancels the transition and discards every update. |

### Config

```toml
[[hooks]]
name = "required-sections"
event = "validate"
types = ["story", "rfc"]
run = [".lazyspec/hooks/required-sections"]

[[hooks]]
name = "rollup-children"
event = "pre-transition"
types = ["rfc"]
to = "complete"
context_types = ["story"]
run = [".lazyspec/hooks/rollup"]
```

- `run` is an argv array, resolved from the project root. No shell.
- `types` defaults to every type. `to` (and optional `from`) apply only to `pre-transition`.
- `context_types` names other types whose documents the hook needs to read. Default is none, which keeps the payload small.
- Hooks bound to the same event run in the order they are declared.

### Protocol

What lazyspec sends on stdin:

```json
{
  "event": "pre-transition",
  "hook": "rollup-children",
  "transition": { "from": "in-progress", "to": "complete" },
  "documents": [ /* show --json shape, including parts and a content hash */ ],
  "context":   [ /* documents of context_types, same shape */ ]
}
```

For `validate`, lazyspec calls each hook once with every matching document, not once per document. For `pre-transition`, `documents` holds just the document being moved.

What the hook prints on stdout:

```json
{
  "findings": [
    { "id": "RFC-012", "part": null, "line": 14,
      "severity": "error", "message": "missing ## Goals" }
  ],
  "updates": [
    { "id": "RFC-012", "part": "summary.md", "hash": "…", "body": "…" }
  ]
}
```

- A non-zero exit, or output that isn't valid JSON, becomes one error finding naming the hook and including its stderr. For `pre-transition` that blocks the move.
- `updates` returned from a `validate` hook is an error.
- Each update carries the content hash lazyspec sent. If the document changed while the hook ran, the whole batch is refused.
- A hook that runs longer than the timeout (default 30s, set per hook with `timeout`) is killed and becomes an error finding.

### Why hooks return updates instead of writing them

- **Every store works.** The engine saves through the same function as `update --body` / `update --part`, so git, github-issues and ClickUp documents are handled like local files.
- **All or nothing.** Every update is checked before any is saved, then saved together with the status change. A hook that fails halfway leaves nothing behind.
- **No loops.** The hook never calls lazyspec, so hooks can't trigger other hooks.
- **Easy to test.** A hook is JSON in, JSON out. Its tests need no project and no store.

### Engine

- A `HookRunner` trait is the I/O boundary: a real implementation that spawns the process (building on `subprocess::output_with_timeout`), and a fake for tests.
- A `HookRule` implements `Checker` and turns `validate` hook findings into a new `ValidationIssue::Hook { hook, id, part, line, message }`. It is added in `default_checkers`.
- Transitions go through one engine function that runs the matching hooks, merges their updates, checks hashes, then saves the updates and the status together. The CLI and TUI both call it; neither re-implements it.

### Surfaces

- **CLI:**
  - `validate` and `status` include hook findings.
  - `update --status` runs `pre-transition` hooks.
  - `hook run <event> <id> [--dry-run]` fires an event by hand. `--dry-run` prints the updates without saving them.
  - `hook list` shows each hook's event, scope and trust state.
  - `hook trust` trusts the current hook config.
  - Every command takes `--json`.
- **TUI:**
  - Hook findings show in the validation display.
  - A blocked status change shows its findings instead of moving.
  - The command palette (RFC-073) lists `hook run` for the selected document.
- **Web view:** the same findings and blocked transitions, read from the engine. There is no separate code path.

### Trust

A cloned repo's `.lazyspec.toml` is someone else's code. `lazyspec hook trust` records a hash of the `[[hooks]]` table, plus each `run` target's file hash where the target is inside the repo, in user-local state outside the repo. If the hash doesn't match, every hook is skipped with a warning finding. Editing the hooks or their scripts means trusting them again. `--no-hooks` skips hooks for one command. The pattern is direnv's.

### Performance

The TUI refreshes validation often. `validate` hook results are cached by hook name plus the hash of their input, the same way `StalenessCache` caches staleness. The TUI's quick refresh (`validate_without_stale`) reads the cache and leaves it alone. The full validate refills it.

## Alternatives

- **Declarative rules in config.** No trust problem and cheap to run, but they only cover structure checks and can't express updates made on a status change. Could come later for the common checks.
- **Embedded WASM or Lua.** Sandboxed, so no trust step. But it forces one runtime on pack authors and adds a large dependency to a simple doc tool.
- **Hooks call the CLI directly.** No protocol to design, but we lose the all-or-nothing save, the stale-edit check, and support for non-file stores, and we'd need guards against hooks triggering hooks.

## Open questions

- Should `hook trust` be scoped per repo or per user? Scoped per repo means a fork has to be trusted again.
- Can `hook run` add documents by creating them, or only update existing ones? Default: updates only.
- Should a `validate` hook be able to scope itself to changed documents, i.e. take a diff instead of the full set?
