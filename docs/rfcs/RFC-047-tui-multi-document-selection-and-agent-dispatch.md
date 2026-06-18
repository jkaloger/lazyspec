---
title: TUI multi-document selection and agent dispatch
type: rfc
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-046
---

## Problem

RFC-046 makes the TUI agent dialog template-driven, but every action operates on exactly one document: the cursor selection (`selected_doc: usize` in @ref src/tui/state/app.rs#App). A user who wants to act on several documents at once -- mark iterations 1-5 and run one agent that drives all five to a shared goal -- has no way to express the set. The render scope (`document`, `child_types`, `context`) is singular, the dialog resolves templates for one type, and `AgentContext` carries one `doc_path`.

The TUI has no multi-selection primitive at all. List navigation moves a single cursor; there is no notion of a marked set. This is a gap below the agent feature: selection is a general TUI capability that other operations (bulk status change, bulk linking) would also consume, but none exists today.

This is not orchestration. The rejected daemon line (RFC-036, RFC-041) proposed a long-lived scheduler leasing work across many agents. This RFC proposes the opposite: a user marks a set by hand and hands it to a single foreground or background agent, exactly as the single-document path does today, with one more document in scope.

## Intent

Add a general multi-selection primitive to the TUI, and make agent dispatch its first and only v1 consumer.

- A marked set lives in TUI state, keyed by document id. The user toggles membership with `space`; marked documents render with a marker. The existing cursor is unchanged.
- Dispatching an agent over a marked set is single-launch: one agent, the whole set in scope. There is no fan-out (no per-document spawn) and so no orchestration lifecycle to manage.
- A multi-document run renders against a `documents` list rather than a singular `document`. A template declares which world it belongs to.
- Headless and interactive both extend to the set the same way they handle one document in RFC-046: one background spawn, or one terminal handover, over the set.

The primitive is deliberately larger than its consumer. Marking documents is useful independent of agents; bulk non-agent operations are admitted later (dictum 6: the abstraction earns its place once a second consumer exists). v1 ships only the agent consumer.

## Design

### Selection primitive

@ref src/tui/state/app.rs#App gains a marked set alongside the existing cursor:

@draft MarkedSet {
ids: BTreeSet<String>,
}

Membership is keyed by **document id, not list index**. The list re-sorts and filters (search, type filter); an index identifies a different document after a re-sort, an id does not. `space` toggles the cursor document's id in the set. Marked documents render with a leading marker in the list. The set is cleared after an action dispatches and on `Esc`. The cursor (`selected_doc`) is untouched: marking and navigating are independent, so the user moves the cursor to each target and toggles it without losing the set.

This is engine-free TUI state (dictum 3: selection is a rendering/interaction concern, not an engine one). No new engine interface is required to track a set of ids the TUI already knows.

### Dispatch is single-launch, not fan-out

When the user opens the agent dialog with a non-empty marked set, a selected action spawns **one** agent over the whole set. It does not spawn one agent per document. Fan-out (N spawns, N records, N lifecycles to poll and reconcile) is the orchestration shape this project rejected (RFC-036 / RFC-041); single-launch keeps a multi-document run identical in lifecycle to a single-document run -- one `AgentRecord` for headless, none for interactive (RFC-046).

Because there is exactly one launch, no per-template `scope: each|all` selector is needed. The set is the unit of work.

### Render scope for a set

A multi-document run renders against a list, not a singular document:

- `documents`: the marked set, each entry exposing the same fields as RFC-046's `document` (`id`, `title`, `type`, `body`, `status`, `path`). A template iterates it: `{% for d in documents %}{{ d.id }} {{ d.title }}{% endfor %}`.

There is no singular `document` in a multi-document run. Per-document lineage `context` (the `resolve_chain` DAG variable RFC-046 adds) is out of scope for sets in v1: a per-document ancestor chain across an arbitrary set is heavy and has no single consumer yet. Templates reference per-document fields only.

### Template arity and dialog gating

Because the multi render scope exposes `documents` and no `document`, a single-document template (`{{ document.id }}`) rendered over a set hits minijinja strict-undefined and errors, and vice versa. The arity is a property of the template, so it is declared in frontmatter. RFC-046's `AgentPrompt` gains an optional flag:

@draft AgentPrompt {
multi: bool,
}

`multi` defaults to `false` (the single-document template of RFC-046). A `multi: true` template renders against `documents`.

The dialog gates on the marked set so every offered action is runnable:

- Marked set empty: offer the single-document (`multi: false`) templates, exactly as RFC-046 does for the cursor document.
- Marked set non-empty: offer only `multi: true` templates, rendered against the set.

A template is never offered in a mode it cannot render, so a selection cannot produce a strict-undefined error.

### Type gating across a heterogeneous set

RFC-046 gates templates per type (the `agents` list on `TypeDef`). A marked set may span types (iterations and a story). The resolved action set is the **intersection**: a `multi: true` template is offered only if every marked document's type lists it. A template valid for iterations but not stories disappears the moment a story joins the set. Intersection is the safe rule -- every offered action is one the project sanctioned for every document it will touch. Union would offer actions a document's type never opted into.

### Headless vs interactive over a set

RFC-046's `AgentContext` carries a single `doc_path`. A multi-document run carries the set:

@draft AgentContext {
doc_paths: Vec<PathBuf>,
}

The single-document path is the one-element case. The headless multi run is one `ClaudeP` spawn with the rendered `documents` prompt; it records one `AgentRecord` and returns immediately (RFC-046's responsive background flow, unchanged in lifecycle).

The interactive multi run is one terminal handover -- a single handover cannot fan out to N concurrent sessions. The engine exports `$LAZYSPEC_DOC_PATHS` (the set's paths, newline-separated) instead of RFC-046's singular `$LAZYSPEC_DOC_PATH`, alongside `$LAZYSPEC_PROMPT`. The configured `[agents] interactive` command references it. The suspend/run/restore sequence is unchanged (it is indifferent to how many paths the command sees).

### Custom prompt over a set

RFC-046's freeform Custom entry extends to the set: the typed text is the prompt, the marked set's paths are the document context, and it spawns one headless agent (no template, runtime-default tools). It mirrors the single-document Custom entry with `documents` in place of `document`.

## Interfaces

- @draft MarkedSet -- the TUI marked set, keyed by document id; toggled by `space`, cleared on dispatch/`Esc`.
- @ref src/tui/state/app.rs#App -- gains the marked set alongside `selected_doc`.
- @draft AgentPrompt -- RFC-046's template model gains `multi: bool` (default `false`).
- @draft AgentContext -- RFC-046's spawn input generalises `doc_path: PathBuf` to `doc_paths: Vec<PathBuf>`.
- @ref src/tui/agent.rs#AgentSpawner -- dispatches a single spawn over the marked set; renders the `documents` scope.

## Stories

1. **Marked-set selection primitive.** Add the marked set (by id) to TUI state. `space` toggles the cursor document. Render a marker on marked rows. Clear on dispatch and `Esc`. No agent behaviour yet -- this slice is the visible, independently shippable primitive.

2. **Multi render scope + template arity + dialog gating.** Add `documents` to the render context. Add `multi: bool` (default `false`) to `AgentPrompt` frontmatter. Gate the dialog: single-document templates when the set is empty, `multi: true` templates (intersection across the marked types) when it is non-empty. No template is offered in a mode it cannot render.

3. **Headless multi dispatch.** Generalise `AgentContext` to `doc_paths: Vec<PathBuf>`. Dispatch a selected `multi: true` template as one headless spawn over the set, rendering `documents`, recording one `AgentRecord`. Extend Custom prompt to the set.

4. **Interactive multi dispatch.** Export `$LAZYSPEC_DOC_PATHS` (newline-separated) for an interactive multi run. Dispatch a `multi: true` interactive template through the existing suspend/run/restore handover over the set.

## Out of scope

- Fan-out / per-document spawn. Dispatch is single-launch; one agent over the set. The orchestration lifecycle (per-document records, polling, reconciliation) is the rejected daemon line (RFC-036 / RFC-041).
- Bulk non-agent operations (status change, linking, deletion) over the marked set. The selection primitive admits them; no second consumer ships here (dictum 6).
- Range / visual select (`V`-style motion to mark a span). v1 is toggle-only; range select layers on the same set later.
- Per-document lineage `context` for a set. The singular path keeps RFC-046's `context`; sets expose per-document fields only.
- Any daemon, lease, or worktree machinery.
