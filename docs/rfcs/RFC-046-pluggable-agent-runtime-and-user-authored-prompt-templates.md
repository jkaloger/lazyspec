---
title: Pluggable agent runtime and user-authored prompt templates
type: rfc
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- supersedes: RFC-016
---

## Problem

Interactive agent mode (RFC-016) shipped, but the implementation is rigid in three ways that this RFC removes.

`AgentSpawner::spawn` in `src/tui/agent.rs` hardcodes the runtime: `Command::new("claude").args(["-p", prompt])` with a fixed `--allowedTools` string. There is no seam to swap the backend, so adding another runtime (pi, opencode server) means editing the spawner.

Prompts are hardcoded in Rust. `build_expand_prompt` and `build_create_children_prompt` bake prompt text into the binary. A project cannot change what the agent is told without recompiling lazyspec, which contradicts the unopinionated direction of RFC-042 and the strict-config / no-engine-defaults stance of ADR-011.

Agent mode is always on. There is no per-type gate and no opt-in. Every document type offers the same fixed action set whether or not the project wants agents involved.

This RFC supersedes RFC-016. The orchestration-daemon line (RFC-036, RFC-041) is rejected; nothing here depends on or shares design with it. The scope is the interactive, TUI-initiated agent path only.

## Intent

Make interactive agent mode pluggable, user-authored, and opt-in, without baking any default behaviour into the engine.

- Headless background runs sit behind a trait so the backend is swappable. v1 ships one implementation that runs `claude -p`.
- A template declares one of two run modes. Headless is the existing background flow. Interactive hands over the terminal (like the `e` editor command but for a live agent session), and the command it launches is configured in toml: `claude`, `opencode`, `pi`, or a custom shell/tmux command.
- What the agent is told comes from user-authored prompt templates in `.lazyspec/agents/`, rendered with a real template engine. The engine ships no prompts.
- Whether a document type exposes agents at all is declared per type in config. Absent declaration means off.

Zero defaults is the organising principle. With no config and no template files, agent mode does not appear. There is no embedded prompt to fall back to and nothing for `init` to write. This matches ADR-011 (the engine carries no default ontology; config is the sole source) and RFC-042 (the tool is unopinionated about taxonomy).

## Design

### Runtime seam

A template declares one of two run modes, and they are built differently: headless is a background job behind a trait, interactive is a configured shell command. They are not two methods on one trait -- interactive has a single behaviour (run whatever the project configured), so per dictum 6 it earns no trait.

**Headless.** Spawning a background agent is subprocess I/O. Per dictum 4, I/O boundaries are defined by traits so production and test code share one interface; `AgentRunner` is that seam (a fake runner lets tests assert on the `AgentContext` without launching a process). v1 ships one implementation, `ClaudeP`, running `claude -p`. pi / opencode-server headless backends are admitted by adding impls later; out of scope for v1.

@draft AgentRunner {
fn spawn(&self, ctx: AgentContext) -> Result<AgentHandle>;
}

@draft AgentContext {
prompt: String,
allowed_tools: Option<String>,
doc_path: PathBuf,
session_id: String,
}

@draft AgentHandle {
session_id: String,
child: Child,
}

`ClaudeP` runs `claude -p <prompt> --session-id <id>` with stdio discarded, passing `--allowedTools` when `allowed_tools` is `Some`. The current spawn logic in @ref src/tui/agent.rs#AgentSpawner refactors onto this trait: `AgentSpawner` becomes the lifecycle/record owner (history, poll, status) and delegates process creation to an injected `AgentRunner`. The hardcoded `Command::new("claude")` call moves into `ClaudeP`.

**Interactive.** Interactive hands over the terminal: the agent runs in the foreground attached to the inherited stdio, blocking until it exits, exactly as the `e` editor command does today (@ref src/tui/infra/event_loop.rs#run_editor leaves the alternate screen, disables raw mode, runs the child to completion, then restores). Which tool runs is a property of the machine and project, not the document, so it is configured once in toml:

```toml
[agents]
interactive = 'claude "$LAZYSPEC_PROMPT"'
# or 'opencode -p "$LAZYSPEC_PROMPT"', 'pi', 'tmux new-window claude "$LAZYSPEC_PROMPT"'
```

@draft AgentsConfig {
interactive: Option<String>,
}

The string is run via `bash -lc`. The engine sets `LAZYSPEC_PROMPT` (the rendered template body) and `LAZYSPEC_DOC_PATH` in the child's environment; the command references them. Passing the prompt by environment variable rather than interpolating it into the command line avoids shell-quoting rendered markdown. Zero defaults (ADR-015) holds: there is no built-in interactive command. When `[agents] interactive` is unset, interactive actions are unavailable -- templates with `mode: interactive` do not run and are not offered.

The engine builds the `Command` (program, args, env) for both modes; it never touches terminal state. The alternate-screen / raw-mode dance around an interactive run stays in the TUI event loop (dictum 3: the engine makes no assumption about a terminal). The TUI suspends before running the command and restores (and drains buffered stdin) after, mirroring `run_editor`. CLI and TUI never depend on each other (dictum 3).

### Prompt templates

A prompt template is a markdown file at `.lazyspec/agents/<name>.md` with YAML frontmatter and a minijinja body:

```markdown
---
name: pair-session
description: Open a live Claude session on this document
mode: interactive
allowed_tools: "Read,Edit,Bash(lazyspec *)"
---
You are pairing on {{ document.path }}.
Here is the current {{ document.type }}:

{{ document.body }}
```

@draft AgentPrompt {
name: String,
description: String,
mode: RunMode,
allowed_tools: Option<String>,
body_template: String,
}

@draft RunMode {
Headless,
Interactive,
}

Discovery reads `*.md` under `.lazyspec/agents/`. Frontmatter requires `name` and `description`; `mode` and `allowed_tools` are optional. `mode` defaults to `headless` (background `claude -p`); `interactive` marks the action for terminal handover. The body is rendered with minijinja in strict-undefined mode, so a reference to an unknown variable is an error surfaced to the user, not a silent empty string. A file with missing or malformed frontmatter is skipped with a warning and does not appear in the dialog.

Render context:

- `document`: the selected document, exposing `id`, `title`, `type`, `body`, `status`, `path`.
- `child_types`: the list of child type names for `document.type`, derived from the parent-child rules already in config. This is what lets a user author a "create children" prompt without the engine hardcoding one: the template decides how to use `child_types`.
- `context`: the selected document's resolved lineage, so a template can carry parent RFCs, Stories, and ADRs into the prompt as constraints (e.g. "refine this iteration against the RFC it implements"). It exposes `context.ancestors` (the `implements` chain, nearest parent first) and `context.related` (adjacent `related-to` documents). Each entry exposes the same `document.*` fields, so a template iterates `{% for node in context.ancestors %}{{ node.type }} {{ node.id }}: {{ node.body }}{% endfor %}`. Sourced from the existing @ref src/engine/context.rs#resolve_chain (`ResolvedContext`), already consumed by the resolve-context CLI and the TUI Relations/Graph views; this RFC reuses it for the render scope rather than re-deriving the DAG. The descendant/forward direction is omitted -- a prompt's constraints come from what a document implements, not from what implements it.

The engine owns loading, frontmatter parsing, and rendering. The result is a fully rendered prompt string plus the template's metadata; the headless path passes the string to `AgentRunner::spawn`, the interactive path exports it as `$LAZYSPEC_PROMPT`.

`allowed_tools` is per template (frontmatter) and applies to the headless `claude -p` path: a template that sets it spawns with `--allowedTools`, one that omits it relies on the runtime's defaults. It is ignored for `mode: interactive`, where the configured command (claude, opencode, tmux, ...) owns its own tool policy. The only global `[agents]` key is the interactive launch command; action gating stays per type and tool scope stays per template.

### Per-type opt-in

@ref src/engine/config.rs#TypeDef gains an `agents` field:

```toml
[[types]]
name = "rfc"
agents = ["expand", "create-children"]

[[types]]
name = "story"
# no agents key -> agent mode off for stories
```

The list entries are template file stems under `.lazyspec/agents/`. An absent `agents` key (the default via `#[serde(default)]`) means the type exposes no agents. Resolution is: given the selected document's type, intersect the type's `agents` list with the templates that actually loaded; the result is the action set for that document. A configured name with no matching template file is reported (the user named an action they did not author); a template file not referenced by any type is simply unused.

This keeps gating per type with no global toggle, consistent with the locked direction. Strict-load semantics (ADR-011) apply to the field's shape, but an empty/absent `agents` list is the valid "off" state, not an error.

### TUI dialog

Pressing `a` on a selected document opens the action dialog. The dialog is fully template-driven: it lists the templates resolved for the document's type (each shown by its frontmatter `name` and `description`), plus one freeform "Custom prompt" entry that takes typed text and spawns with no template and no `allowed_tools` restriction beyond the runtime default. There are no built-in Expand / Create-children entries; those become ordinary templates the user authors if they want them.

Selecting an entry dispatches by its `mode`. A headless action spawns in the background through `AgentRunner::spawn`, records an `AgentRecord`, and returns immediately so the TUI stays responsive (the existing flow). An interactive action suspends the TUI, runs the configured `[agents] interactive` command (`bash -lc` with `$LAZYSPEC_PROMPT` / `$LAZYSPEC_DOC_PATH` exported), and blocks until the session exits, then restores the screen and drains buffered stdin -- the same suspend/run/restore sequence as `run_editor`. Interactive runs are foreground and synchronous; like `e`, they leave no `AgentRecord`. The dialog distinguishes the two so the user knows a selection will hand over the terminal; when `[agents] interactive` is unset, interactive templates are not offered.

If the document's type has no resolved templates, the dialog offers only Custom prompt (or, if the project wants agents fully off for that type and authors no Custom path, nothing). This supersedes STORY-051's fixed action set.

### History location cleanup

`agent_history_dir` in `src/tui/agent.rs` writes run records to `$HOME/.lazyspec/agents/`. Prompt templates now live at `<repo>/.lazyspec/agents/`. Same relative path, different roots, which is confusing and risks a future reader conflating the two. This RFC relocates run history to `<repo>/.lazyspec/cache/agents/` (alongside the existing `.lazyspec/cache/`), leaving `.lazyspec/agents/` exclusively for user-authored templates.

## Interfaces

- @draft AgentRunner -- headless background spawn seam; `ClaudeP` is the sole v1 impl.
- @draft AgentContext / @draft AgentHandle -- inputs and headless handle for a spawned agent.
- @draft AgentPrompt / @draft RunMode -- parsed template (frontmatter + minijinja body), `mode` selecting headless vs interactive.
- @draft AgentsConfig -- the `[agents]` block; `interactive` is the optional shell command for terminal handover.
- @ref src/engine/config.rs#TypeDef -- gains `agents: Vec<String>`.
- @ref src/engine/context.rs#resolve_chain -- existing DAG resolver; reused to populate the `context` render variable (ancestors + related).
- @ref src/tui/agent.rs#AgentSpawner -- refactored to delegate process creation to an `AgentRunner`; history dir relocated.

## Stories

1. **AgentRunner trait + ClaudeP impl.** Introduce the `AgentRunner` trait (`spawn` headless) and `AgentContext` / `AgentHandle` in the engine. Implement `ClaudeP` running `claude -p`. Refactor @ref src/tui/agent.rs#AgentSpawner so it owns records/polling and delegates spawning to an injected `AgentRunner`. No behaviour change to the existing actions yet; this is the seam.

2. **Prompt template load + render.** `.lazyspec/agents/*.md` discovery, frontmatter parse (`name`, `description`, optional `mode`, optional `allowed_tools`), minijinja strict-undefined render with `document.*`, `child_types`, and `context` (resolved lineage from @ref src/engine/context.rs#resolve_chain -- ancestors + related). Skip-and-warn on malformed files. Delete `build_expand_prompt` / `build_create_children_prompt`. Subsumes STORY-053 (custom agent prompts) -- STORY-053 should be superseded.

3. **Per-type opt-in config.** Add `agents: Vec<String>` to `TypeDef`, default empty. Resolution from a document's type to its allowed template set, with reporting of named-but-missing templates. Agent mode off when the list is absent/empty.

4. **TUI dialog rewired.** Replace the fixed action dialog with a template-driven list (frontmatter `name`/`description`) plus freeform Custom. Headless entries spawn through the runner with the rendered prompt and the template's `allowed_tools`. Supersedes STORY-051's fixed actions.

5. **Interactive run mode.** Add the `[agents]` config block with `interactive: Option<String>`. Add `mode: RunMode` to `AgentPrompt`/template frontmatter (default headless). Build the interactive command from `[agents] interactive` via `bash -lc` with `$LAZYSPEC_PROMPT` and `$LAZYSPEC_DOC_PATH` exported. Wire the TUI dialog to dispatch interactive entries through the suspend/run/restore sequence (leave alternate screen, disable raw mode, run to exit, restore, drain stdin) modelled on @ref src/tui/infra/event_loop.rs#run_editor. Interactive templates are offered only when `[agents] interactive` is set. No `AgentRecord` for interactive runs.

## Out of scope

- pi and opencode-server as headless backends. The `AgentRunner` trait is shaped to admit them; no impl ships here. (Interactive use of these tools needs no code -- it is just a different `[agents] interactive` command.)
- A configurable headless command. Headless stays `claude -p` (`ClaudeP`) in v1; only the interactive command is toml-driven.
- The orchestration daemon (RFC-036 / RFC-041, rejected). No daemon, lease, or worktree machinery.
- STORY-052 (agent management screen) behaviour. It continues to track running/past agents unchanged; only the history directory moves.
