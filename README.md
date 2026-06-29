<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    A little TUI & CLI for project documentation.
</p>

<p align="center">
  <a href="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml"><img src="https://github.com/jkaloger/lazyspec/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <img src="https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white" alt="Rust 2021">
  <img src="https://img.shields.io/badge/status-experimental-blueviolet" alt="Status: Experimental">
  <img src="https://img.shields.io/github/v/tag/jkaloger/lazyspec?label=version&color=blue" alt="Version">
  <a href="https://github.com/jkaloger/lazyspec/commits/main"><img src="https://img.shields.io/github/last-commit/jkaloger/lazyspec?logo=git&logoColor=white" alt="Last commit"></a>
  <a href="https://github.com/jkaloger/lazyspec/blob/main/flake.nix"><img src="https://img.shields.io/badge/nix-flake-5277C3?logo=nixos&logoColor=white" alt="Nix Flake"></a>
</p>

<img alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/91f308d1-8d03-4815-b2ec-fa445159c563" />

> [!WARNING]
> Lazyspec is experimental. APIs and CLI interfaces will change frequently and without notice.

## Features

Lazyspec manages project documentation as version-controlled markdown files with YAML frontmatter. Documents live in your repo, so agents and humans read from the same source of truth.

- Create, update, link, and validate documents. Config-driven relationships (the starter set is `implements`, `supersedes`, `blocks`, `related-to`) keep the chain explicit.
- Catch broken links, orphaned documents, and incomplete frontmatter before they rot. `lazyspec validate` exits non-zero on errors, so it slots into CI.
- Embed `@ref` directives in your specs to point at source code. Lazyspec expands them inline using `git show`, with symbol-level extraction for Rust and TypeScript.
- Fuzzy search, markdown preview, live file watching, and document creation without leaving the terminal.
- Every command supports `--json` output for automation and agent integration.
- Define your own types, templates, and directory layout in `.lazyspec.toml`.

## Install

### Nix

```sh
nix profile install github:jkaloger/lazyspec
```

Or run without installing:

```sh
nix run github:jkaloger/lazyspec
```

### Cargo

```sh
cargo install --git https://github.com/jkaloger/lazyspec
```

### From Source

```sh
git clone https://github.com/jkaloger/lazyspec
cd lazyspec
cargo install --path .
```

### Shell Completions

Generate and source a completion script for your shell:

```sh
# zsh
source <(lazyspec completions zsh)

# bash
source <(lazyspec completions bash)

# fish
lazyspec completions fish | source
```

Add the appropriate line to your shell profile (`~/.zshrc`, `~/.bashrc`, etc.) to load completions on startup. Completions include subcommands, flags, document IDs, and relationship types.

## Skills

Lazyspec ships a set of config-driven generic verb skills that enforce its
workflow against whatever document types your `.lazyspec.toml` defines. The
`lazy` router is the entry point: it reads the configured lifecycle DAG and the
user's position, then dispatches the right verb.

| Skill      | Purpose                                                                           |
| ---------- | --------------------------------------------------------------------------------- |
| `lazy`     | Entry-point router -- reads the DAG and position, dispatches the right verb       |
| `scaffold` | Create a new document's file and frontmatter, hand the body back to the human     |
| `co-write` | Collaboratively draft a document body -- AI proposes, human edits, iterate        |
| `generate` | Author a full document body from context (only when the type's ceiling allows it) |
| `advance`  | Move a document to its next status along the type's lifecycle DAG, checking gates |
| `execute`  | Carry out the work a delivery document describes against its tasks and ACs        |
| `review`   | Critique a document against its intent and acceptance criteria before advancing   |

### Installing skills

`skills install` places the embedded skill set into the project. It works with
or without a `.lazyspec.toml` (and never creates one):

```sh
lazyspec skills install                  # both runtimes (default)
lazyspec skills install --runtime claude     # .claude/skills/ only
lazyspec skills install --runtime agents-md  # ./AGENTS.md only
```

For Claude, each skill is written under `.claude/skills/<verb>/SKILL.md`; the
router is installed under the configured `[skills] entry` name (default `lazy`).
For other agents, the same prose is concatenated into `./AGENTS.md`. Re-running
is idempotent. Configure the router name via `[skills] entry` in
`.lazyspec.toml`.

### Install as a Claude Code plugin

Claude Code users can install the skills and the convention hook together
through the plugin marketplace hosted in this repo, instead of running `skills
install` and hand-editing settings. Two commands:

```
/plugin marketplace add jkaloger/lazyspec
/plugin install lazyspec@lazyspec
```

This loads every on-disk skill under `skills/` and registers a
`UserPromptSubmit` hook that injects the project convention (`lazyspec
convention --preamble`) into the agent's context on each prompt.

**Prerequisite:** the `lazyspec` binary must be on `PATH`. The hook shells out
to it; without the binary the hook is inert (a silent noop), and in any
directory lacking a `.lazyspec.toml` it injects nothing.

The plugin is an additional channel, not a replacement for `skills install`.
Use `skills install` when you need the `AGENTS.md` target or a renamed router
entry via `[skills] entry` -- a static plugin ships the default `lazy` entry and
the Claude runtime only.

## Usage

### Quick Start

Initialise a new project, then launch the TUI:

```sh
lazyspec init
lazyspec
```

> [!TIP]
> Check the `examples/` directory for a complete project setup including config, templates, and agent skill definitions you can use as a starting point.
> This repo dogfoods lazyspec, so you can also check out the `docs/` directory or run `lazyspec` from this repo.

### TUI

Running `lazyspec` with no subcommand opens the interactive dashboard. It provides fuzzy search, markdown preview, document creation, and live file watching -- documents update automatically when changed on disk. An external edit of `.lazyspec.toml` (e.g. a `git pull`) reloads the running session automatically; press `R` to reload it manually. Press `?` for the full keybindings overlay.

| Key       | Action                                              |
| --------- | --------------------------------------------------- |
| `j` / `k` | Navigate up/down                                    |
| `h` / `l` | Switch document type                                |
| `Enter`   | Open document fullscreen                            |
| `/`       | Fuzzy search                                        |
| `n`       | Create new document                                 |
| `e`       | Edit document in `$EDITOR`                          |
| `d`       | Delete document                                     |
| `r`       | Add relation                                        |
| `R`       | Reload config from `.lazyspec.toml`                 |
| `w`       | Warnings / validation panel                         |
| `5`       | Open the Settings view                              |
| `` ` ``   | Cycle view (documents / filters / graph / settings) |
| `q`       | Quit                                                |
| `?`       | Toggle keybindings help                             |

#### Settings View

Press `5` (or cycle to it with `` ` ``) to open the Settings view, which edits `.lazyspec.toml` in place. Categories are listed on the left; the right panel shows the fields (or entries) of the selected category. Saving rewrites `.lazyspec.toml`, preserving its comments and formatting, after validating the whole config; an invalid config is reported and not written.

| Key            | Action                                                                                                                                                     |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `h` / `l`      | Switch category (also `Left` / `Right`)                                                                                                                    |
| `j` / `k`      | Move between fields / entries (also `Down` / `Up`)                                                                                                         |
| `Enter`        | Drill into a collection entry, or start editing a field                                                                                                    |
| `n`            | Add a new entry to a collection (Document Types / Relationships / Validation Rules seed a default and drill in; Certification prompts for a spec-path key) |
| `d`            | Delete the selected collection entry, behind a confirm (refuses the last relationship)                                                                     |
| `Space`        | Toggle a boolean / cycle an enum field                                                                                                                     |
| `g`            | When a dependency section is auto-scaffolded (e.g. cycling numbering to `sqids`), jump to the required field it needs filled                               |
| type + `Enter` | Confirm a text / number / duration / list edit                                                                                                             |
| `Esc`          | Cancel an in-progress edit, or undrill from an entry                                                                                                       |
| `w` / `Ctrl-S` | Save changes to `.lazyspec.toml` (validates the whole config)                                                                                              |
| `q` / `Esc`    | Quit; with unsaved changes, prompts `(s)ave / (d)iscard / (Esc) cancel`                                                                                    |

#### Graph View

Cycle to the Graph view with `` ` ``. The left panel is a pivot picker (`h` / `l` to re-root the forest on a document type or a tag, or `All` for the whole store); the right panel renders the dependency forest as a nested table sharing the documents table's styling (git-status gutter, slim `ID` column, selection bar, scrolling). The `DOC` column is the document tree, with indentation and connector art showing the `implements` lineage; each configured column follows. A document reachable from more than one parent (a diamond) is drawn once under each parent; cyclic edges are hidden. Siblings under a shared parent can be sorted by any column while the parent grouping and topological order are preserved.

| Key                 | Action                                                                         |
| ------------------- | ------------------------------------------------------------------------------ |
| `j` / `k`           | Navigate up/down                                                               |
| `Ctrl-d` / `Ctrl-u` | Half page down/up                                                              |
| `h` / `l`           | Pivot the anchor (whole store → types → tags)                                  |
| `o`                 | Cycle the sibling sort column (`path` → `status` → declared attributes → wrap) |
| `O`                 | Reverse the sort direction                                                     |
| `g` / `G`           | Jump to top/bottom                                                             |
| `Enter`             | Open the selected document                                                     |
| `e`                 | Edit document in `$EDITOR`                                                     |

The columns and default sort are configured under `[tui.graph]` in `.lazyspec.toml`:

```toml
[tui.graph]
# Columns rendered to the right of the DOC tree column. Each id is either a
# built-in (`status`, `related`) or a declared attribute name (`[[types.attributes]]`).
# An attribute not declared/present on a row's type renders as an empty cell.
columns = ["status", "related"]   # default
# Default sibling sort column: `path` (the stable topological order), `status`,
# or any declared attribute name. `o` cycles from here; missing attribute values
# always sort last.
sort = "path"                     # default
```

Both keys carry defaults, so a config without a `[tui.graph]` block still loads.

### Web view

A read-only web view of the project's documents is available behind the `web` cargo feature, so default builds carry no async/HTTP dependencies:

```sh
cargo run --features web -- serve            # binds 127.0.0.1:8787
cargo run --features web -- serve --port 9000
```

`serve` loads the store once and renders a server-side document list (grouped by type) with htmx status/tag filtering. It binds loopback only.

<details>
<summary><h3>CLI</h3></summary>

All document management is available as subcommands. Most accept `--json` for machine-readable output.

| Command                                                         | Description                                                                                     |
| --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `init`                                                          | Initialise lazyspec in the current project                                                      |
| `create <type> <title> [--author X] [--parent ID] [--body / --body-file]` | Create a document (rfc, adr, story, iteration); seed body inline, from a file, or `-` for stdin. `--parent <ID>` makes the new doc a child of an existing doc; the child must be the same store as its parent. For filesystem-store types the child is authored as a sibling `.md` inside the parent's subdir (promoting a flat parent to `TYPE-n-slug/index.md` on the first child). For `github-issues`-store types the child is created as a real GitHub issue and bound as a native sub-issue of the parent at create time; a later `fetch` mirrors them into the nested cache layout (`.lazyspec/cache/<type>/<PARENT>/index.md` + `NN-<child>.md`) |
| `list [type] [--status X]`                                      | List documents with optional filters                                                            |
| `show <id> [-e]`                                                | Display a document by path or shorthand ID (e.g. `RFC-001`)                                     |
| `update <path> [--status X] [--title X] [--body / --body-file] [--attr key=value]` | Update frontmatter and/or body content (`--body-file -` reads stdin); `--attr` (repeatable) sets a declared custom attribute, coerced and validated against its type; works for all stores |
| `delete <path>`                                                 | Delete a document                                                                               |
| `link <from> <rel> <to>`                                        | Add a typed relationship (canonical or inverse keyword)                                         |
| `unlink <from> <rel> <to>`                                      | Remove a relationship (canonical or inverse keyword)                                            |
| `search <query> [--doc-type X]`                                 | Full-text search across all documents                                                           |
| `context <id> [--depth N]`                                      | Show the full document chain (RFC -> Story -> Iteration)                                        |
| `context [--anchor TYPE]`                                       | Emit the context forest (omit `<id>`); `--anchor` re-roots on a type                            |
| `status`                                                        | Show full project status with all documents and validation                                      |
| `ignore <path>`                                                 | Mark a document to skip validation                                                              |
| `unignore <path>`                                               | Remove validation skip from a document                                                          |
| `validate [--warnings]`                                         | Check document integrity and link consistency                                                   |
| `fix [paths] [--dry-run]`                                       | Fix documents with broken or incomplete frontmatter                                             |
| `fix --config [--dry-run]`                                      | Repair `.lazyspec.toml` (inject missing standard relationships/rules)                           |
| `pin <id>`                                                      | Pin blob hashes onto `@ref` directives in a document                                            |
| `provenance add <id> <citation>`                                | Append a citation to a document's provenance list                                               |
| `provenance remove <id> <citation>`                             | Remove an exact-match citation from a document's provenance list                                |
| `provenance list [id]`                                          | List citations for a document, or for all documents grouped by id                               |
| `reservations list`                                             | Show all reservation refs on the remote                                                         |
| `reservations prune [--dry-run]`                                | Remove refs for documents that already exist locally                                            |

#### Relationship Keywords

`link` and `unlink` resolve relationship names against the `[[relationships]]` block in your `.lazyspec.toml` (see [Configuration](#configuration)). The starter config declares the canonical set (`implements`, `supersedes`, `blocks`, `related-to`) and, for each directional relationship, an inverse keyword (`implemented-by`, `superseded-by`, `blocked-by`) -- but the vocabulary is yours to change. An inverse keyword is a write-time alias: it flips the direction and stores the canonical relation on the target document. Nothing new is persisted; the reverse direction is still computed by the link graph.

```sh
lazyspec link STORY-9 blocked-by RFC-2
# writes `blocks: STORY-9` onto RFC-2, prints:
# Linked docs/rfcs/RFC-002-....md --blocks--> STORY-9
```

A relationship declared without an `inverse` is symmetric (like `related-to`) and has no separate inverse keyword. A keyword that matches no declared `name` or `inverse` is rejected before anything is written, and `validate` flags any document carrying a relationship name absent from `[[relationships]]`.

#### `show` Flags

| Flag                        | Description                                      |
| --------------------------- | ------------------------------------------------ |
| `-e`, `--expand-references` | Expand `@ref` directives into fenced code blocks |
| `--max-ref-lines N`         | Max lines per expanded ref (default: 25)         |

Each document entry in `show --json` and `status --json` (under `documents[]`) includes an `attributes` object holding the document's custom frontmatter attributes (declared via `[[types.attributes]]`). Declared attributes are emitted as their typed JSON value -- `int`/`float` as numbers, `string`/`enum` as strings, `bool` as a boolean, `date` as a `"YYYY-MM-DD"` string -- and undeclared keys pass through with their raw YAML value. The field is always present; a document with no attributes serializes it as `{}`, so consumers needn't null-check.

`show --json` and `status --json` also include a read-only `comments` array. For documents whose type uses the `github-issues` store, this fetches the issue's GitHub comment thread live (each entry `{ "author", "body", "timestamp" }`); for all other documents it is an empty array. Comments are never written back to GitHub, never merged into `body`, and never cached. The field is always present.

#### `context` Flags

| Flag            | Description                                                                                                                                               |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--depth N`     | Max hops to follow `related-to` links when collecting related records (default: 1)                                                                        |
| `--anchor TYPE` | Forest mode only (omit `<id>`): re-root the forest on documents of `TYPE`, emitting each anchor plus its chain descendants and pruning ancestors above it |

With an `<id>`, `context` shows that document's chain. Omit the id to emit the
whole-store context forest (every document, parents-first); add `--anchor TYPE`
(e.g. `--anchor story`) to re-root the forest on a document type, surfacing each
anchor-type doc with its descendant subtree only.

```sh
lazyspec context ITERATION-001            # chain for one document
lazyspec context --json                   # whole-store forest
lazyspec context --json --anchor story    # forest re-rooted on stories
```

#### `provenance` Subcommands

Cite the sources of truth that informed a document. Citations are free-form strings stored as a YAML list in frontmatter.

```sh
lazyspec provenance add RFC-001 "Workshop 2026-04-12"
lazyspec provenance add RFC-001 "Privacy Act 1988"
lazyspec provenance list RFC-001
# Workshop 2026-04-12
# Privacy Act 1988

lazyspec provenance remove RFC-001 "Privacy Act 1988"
lazyspec provenance list
# RFC-001	Workshop 2026-04-12
```

All three subcommands accept `--json`. Shapes:

- `add` / `remove`: `{ "doc": "...", "added"|"removed": "...", "provenance": [...] }`
- `list <id>`: `{ "doc": "...", "provenance": [...] }`
- `list` (no id): `{ "documents": [{ "id": "...", "path": "...", "provenance": [...] }, ...] }`

`add` rejects empty citations. `remove` is exact-match and errors when the citation is absent.

## Coordination

### Claude Code Hooks

Lazyspec ships hook snippets that claim, heartbeat, and release a lease on `$ASSIGNED_TASK` across a Claude Code session. The orchestrator (daemon, manual `export`, etc.) sets the env var; hooks no-op silently when it is unset, so the snippet is safe to install unconditionally.

Drop into `.claude/settings.json`:

```json
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec claim \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec heartbeat \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --min-interval 15m --json || true"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "[ -n \"$ASSIGNED_TASK\" ] && lazyspec release \"$ASSIGNED_TASK\" --agent-id \"$CLAUDE_SESSION_ID\" --json || true"
          }
        ]
      }
    ]
  }
}
```

The standalone file lives at [`hooks/claude-code-settings.json`](hooks/claude-code-settings.json).

**`$ASSIGNED_TASK` contract.** Orchestrator sets it to a doc id (e.g. `ITERATION-170`). If unset, the `[ -n "$ASSIGNED_TASK" ]` guard short-circuits and no `lazyspec` invocation happens.

**Throttle.** `--min-interval 15m` matches the default `lease_duration / 4` (lease defaults to 60m). If you tune `lease_duration` in `.lazyspec.toml`, tune this to roughly a quarter of it.

**Error tolerance.** `|| true` swallows non-zero exits from `lazyspec` (e.g. lease already released, network blip), so a session never fails to end because of a coordination error.

See [RFC-035](docs/rfcs/RFC-035-git-ref-document-storage-with-lease-based-claiming.md) for the design rationale.

</details>

<details>
<summary><h3><code>@ref</code> Syntax</h3></summary>

Documents can embed references to source code using `@ref` directives. By default, `lazyspec show` renders them as-is. Pass `-e` to expand them inline.

```
@ref <path>                    # entire file
@ref <path>#<symbol>           # specific type or struct
@ref <path>#<symbol>@<sha>     # symbol at a specific git commit
@ref <path>#123                # line 123
@ref <path>#123@<sha>          # line 123 at a specific git commit
```

Expansion resolves content via `git show` (committed state, not working tree). Supported languages for symbol extraction are TypeScript (`.ts`/`.tsx`) and Rust (`.rs`).

Each expanded ref includes a caption line showing the file path, short git SHA, and symbol or line info. Expanded blocks are truncated to 25 lines by default; when truncated, a trailing comment shows how many lines were omitted. Use `--max-ref-lines` to adjust the limit.

**Example**

A document containing:

```
@ref src/engine/store.rs#Store
```

Renders as:

````
```rust
pub struct Store { ... }
```
````

Unresolvable refs render as:

```
> [unresolved: src/engine/store.rs#Store]
```

</details>

<details>
<summary><h2>Configuration</h2></summary>

`lazyspec init` creates a `.lazyspec.toml` in your project root with a starter set
of document types, relationships, and validation rules. The engine carries no
built-in document types or relationship vocabulary: the `[[types]]`,
`[[relationships]]`, and `[[rules]]` declared in `.lazyspec.toml` are the sole
source of truth. A missing `.lazyspec.toml`, or a config with no `[[types]]`, is a
hard error that points you at `lazyspec init`; a config missing the
`[[relationships]]` block is a hard error that points you at `lazyspec fix --config`.

```toml
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

[[relationships]]
name = "implements"
inverse = "implemented-by"

[[relationships]]
name = "related-to"

[templates]
dir = ".lazyspec/templates"

[naming]
pattern = "{type}-{n:03}-{title}.md"
```

### Migrating an Existing Config

Projects created before relationships and rules became config-driven have a
`.lazyspec.toml` with no `[[relationships]]` or `[[rules]]` blocks. Strict load
now rejects such a config on every command, pointing you at the migration:

```sh
lazyspec fix --config            # inject missing standard relationships/rules and default lifecycles
lazyspec fix --config --dry-run  # preview the additions without writing
```

`fix --config` reads the config leniently (the one place strict load is
bypassed), then appends only the standard `[[relationships]]` / `[[rules]]` that
are missing -- comparing by name, so user-added relationships and rules are kept
and nothing is duplicated. It also injects the default `lifecycle` into any
`[[types]]` entry that lacks one (a type that already declares a lifecycle is
left untouched); migrated types are reported under `lifecycles_added`. Every
existing section (`[github]`, `[coordination]`, comments, ordering) is preserved,
and it is idempotent -- running it on an up-to-date config makes no change. The
flag is config-only: no documents are touched (use plain `lazyspec fix` for
frontmatter).

### Inspecting and Editing the Config

`lazyspec config` reads and edits `.lazyspec.toml` without you opening the file.
The read is plain JSON; the three mutators reconcile the TOML in place, preserving
comments, formatting, and block order exactly as `fix --config` and the TUI
settings screen do.

```sh
lazyspec config --json                      # print the resolved config as JSON

# Append a new document type (name, plural, dir, prefix are positional)
lazyspec config add-type spike spikes docs/spikes SPIKE \
  --icon "◆" --parent-type rfc --intent "throwaway exploration" \
  --authorship generated
# also accepts --singleton, --store <filesystem|github-issues|github-milestones|github-projects|git-ref>,
# --numbering <incremental|sqids|reserved>

# Replace a type's lifecycle (states + edges; `*` matches any source state)
lazyspec config set-lifecycle iteration \
  --state draft --state in-progress --state done \
  --edge draft:in-progress --edge in-progress:done --edge "*:rejected"

# Gate child creation on a parent status (parent-child rules only)
lazyspec config add-gate stories-need-rfcs --status accepted
```

`add-type` rejects a duplicate name; `set-lifecycle` replaces the whole lifecycle
(it is a set, not a merge) and rejects an unknown type; `add-gate` rejects an
unknown rule and refuses a `relation-existence` rule (the gate applies only to
`parent-child` rules). The mutators require an already-valid config; run
`lazyspec fix --config` first to migrate a legacy one.

#### `github-issues` store auth

Types stored as GitHub issues (`--store github-issues`) shell out to the `gh`
CLI, so run `gh auth login` first. Beyond plain issue access, lazyspec reads
native GitHub fields (issue types, Projects v2 fields) over the GraphQL API,
which needs the `project` scope on your token:

```sh
gh auth refresh -s project
```

Without it, schema-snapshot refreshes degrade gracefully -- they emit a warning
and keep serving the last cached snapshot, so offline validation still works.

#### `github-milestones` store

Types stored as `--store github-milestones` map each document to a GitHub
milestone over the REST API (title -> title, body -> description, `status` ->
open/closed state, `due_on` passed through verbatim). Progress
(`percent_complete`) is computed from the milestone's issue counts at read time
and is never writable. The write policy is last-write-wins: a push happens
unconditionally, then the milestone is re-read into the cache (no optimistic
lock). An issue -> milestone association is surfaced as a forward relation on
the issue document: declare a relationship with `github_native = "milestone"`,
and at fetch each issue's native milestone is read back as that relation (e.g.
`targets: MILESTONE-1`), resolving the milestone number to its document.
`link` an issue-backed document to a milestone sets the association on GitHub
(`unlink` clears it). The inverse is read-only and never stored: a milestone
document's `targeted-by` entries are derived virtually as the reverse of each
issue's forward relation; an issue whose milestone maps to no lazyspec document
is skipped.

The relation vocabulary is store-constrained for milestones: `link`/`unlink`
reject store-illegal edges before writing. A `github-milestones` document may be
the target only of the `targets` relation (the `github_native = "milestone"`
edge) and may never be the source of any relation, and `targets` requires its
source to be a `github-issues` document and its target to be a milestone
document. Violations exit non-zero with a clear message (e.g. "milestone docs
cannot be the source of a relation", "only github-issues docs can target a
milestone", "`targets` requires a milestone target", "milestone docs can only be
targeted by `targets`"). In the TUI link editor, milestone documents offer no
relation types, `targets` is offered only for issue-backed sources, and the
candidate search is scoped to match.

#### `github-projects` store

Types stored as `--store github-projects` bind each document to an existing
GitHub Projects v2 board, addressed by its board number (`PROJECT-7` -> board
#7 under `[github].repo`'s owner). The backend is **read/associate only**:
lazyspec never creates or deletes boards (they are authored on GitHub), so
`create` and `delete` are rejected. Resolving a board (`update`/binding) looks it
up over GraphQL under the organization root first, then the user root, and
errors if the number exists under neither -- no create mutation is ever issued.

Board membership is a many-to-many relation: declare a relationship with
`github_native = "membership"`, then `link` an issue-backed document to a board
document to add the issue to that board (`addProjectV2ItemById`); `unlink`
removes only that board's item (`deleteProjectV2Item`), leaving memberships of
other boards untouched. Each membership relation maps to exactly one board and is
synced independently. Membership mutations are self-contained -- no `--attr` is
involved (per-board field values are a separate concern).

Projects v2 mutations require the `project` scope on your `gh` token:

```sh
gh auth refresh -s project
```

On macOS, a slow keyring lookup can make `gh api` fall back to an unauthenticated
request (surfacing as a surprise 403 / rate-limit). If you hit that, pass the
token explicitly:

```sh
GH_TOKEN="$(gh auth token)" lazyspec fetch
```

### Custom Types

Each document type is declared with a `[[types]]` block. This lets you rename the
defaults, add new types, or set custom prefixes and icons used in the TUI.
Directories derive entirely from each type's own `dir`; there is no separate
`[directories]` table.

```toml
[[types]]
name = "rfc"
plural = "rfcs"
dir = "docs/rfcs"
prefix = "RFC"
icon = "●"

[[types]]
name = "spec"
plural = "specs"
dir = "docs/specs"
prefix = "SPEC"
icon = "◆"
```

### Lifecycle

Each type declares a `lifecycle`: the set of valid statuses (`states`) and,
optionally, the permitted status transitions (`edges`). `update --status` is
gated by this lifecycle -- a move is allowed only when an edge from the current
status to the target is declared. An edge with a `*` source matches any current
status (e.g. `* -> superseded` lets any document be superseded). Setting a status
to its current value is always a no-op (idempotent, never rejected). When a move
has no matching edge, `update` exits non-zero and the frontmatter is left
unchanged.

`edges` is optional. A lifecycle that declares `states` but omits `edges` (or
sets `edges = []`) is unconstrained: any move between declared states is allowed.
Declare `edges` only when you want to constrain the order of transitions.

```toml
[[types]]
name = "rfc"
prefix = "RFC"
lifecycle = { states = ["draft", "review", "accepted", "in-progress", "complete", "rejected", "superseded"], edges = [{ from = "draft", to = "review" }, { from = "review", to = "accepted" }, { from = "accepted", to = "in-progress" }, { from = "in-progress", to = "complete" }, { from = "*", to = "rejected" }, { from = "*", to = "superseded" }] }
```

Projects whose `[[types]]` predate the lifecycle axis can backfill the default
lifecycle with `lazyspec fix --config` (see _Migrating an Existing Config_).

### Relationships

The relationship vocabulary is config-driven, just like document types. Each
`[[relationships]]` block declares a relationship `name` and an optional
`inverse` keyword. A directional relationship declares its inverse (e.g.
`implements` / `implemented-by`); a relationship with no `inverse` is symmetric
(e.g. `related-to`). `link`/`unlink` resolve the keyword you type against this
registry -- a canonical `name` links in the stated direction, while a declared
`inverse` flips it and stores the canonical relation on the target. `validate`
flags any document carrying a relationship name not declared here.

A relationship may also declare `traversal`, which governs how it participates in
context traversal: `chain` relationships form the parent-child hierarchy that
`parent-child` validation rules and the context chain walk follow, while
`related` relationships form the symmetric related-context neighbourhood. A
relationship with no `traversal` participates in neither.

```toml
[[relationships]]
name = "implements"
inverse = "implemented-by"
traversal = "chain"

[[relationships]]
name = "tracks"
inverse = "tracked-by"

[[relationships]]
name = "related-to"
traversal = "related"
```

### Validation Rules

Validation rules define structural constraints between document types. Two shapes are supported:

- `parent-child` -- the child type must link to a parent type via any chain relationship (a relationship marked `traversal = "chain"` in `[[relationships]]`).
- `relation-existence` -- documents of a given type must have at least one relationship.

A `parent-child` rule may also carry `require_parent_status`: when set, `create`
of the child type is refused unless at least one parent document of the rule's
`parent` type has reached that status. The required status must be a valid state
of the parent type's lifecycle. Rules without `require_parent_status` impose no
creation gate.

```toml
[[rules]]
shape = "parent-child"
name = "stories-need-rfcs"
child = "story"
parent = "rfc"
severity = "warning"
require_parent_status = "accepted"  # optional: a story cannot be created until an rfc is accepted

[[rules]]
shape = "relation-existence"
name = "adrs-need-relations"
type = "adr"
require = "any-relation"
severity = "error"
```

### Numbering

Document numbers are assigned automatically during `create`. Three strategies are available per type:

| Strategy      | Behaviour                                                                                 |
| ------------- | ----------------------------------------------------------------------------------------- |
| `incremental` | Next sequential integer from existing files (default)                                     |
| `sqids`       | Short hash-like IDs derived from a timestamp, configured via `[numbering.sqids]`          |
| `reserved`    | Reserves numbers on a git remote before creating files, preventing distributed collisions |

Reserved numbering uses git custom refs (`refs/reservations/*`) to coordinate across branches. It wraps either incremental or sqids formatting with an atomic push-based lock, so two people never get the same number.

```toml
[[types]]
name = "rfc"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
remote = "origin"        # default
format = "incremental"   # or "sqids"
max_retries = 5          # push retry attempts before failing
```

If the remote is unreachable, `create` fails rather than silently falling back. Use `lazyspec reservations prune` to clean up refs for documents that have been created.

### Templates

Markdown templates live in the templates directory (`.lazyspec/templates/` by default). `init` materializes a single `template.md` carrying general authoring guidance; because the tool is config-driven, no per-type templates are shipped. `{title}`, `{author}`, `{date}`, and `{type}` are substituted when a document is created, so one template serves every type.

When creating a document, lazyspec resolves the template in this order: a per-type override `{type}.md` (e.g. `rfc.md`, `story.md`), then the shared `template.md`, then a built-in default. Add a `{type}.md` to override a single type while leaving the rest on the shared template.

### Agents

The global `[agents]` block configures interactive agent run mode. When an interactive-mode template is selected in the agent dialog, the configured shell command runs via `bash -lc` with the rendered template body exported as `$LAZYSPEC_PROMPT` and the document path as `$LAZYSPEC_DOC_PATH`.

```toml
[agents]
interactive = 'claude "$LAZYSPEC_PROMPT"'
# or 'opencode -p "$LAZYSPEC_PROMPT"', 'pi', 'tmux new-window claude "$LAZYSPEC_PROMPT"'
```

Zero-defaults: when `[agents] interactive` is unset, interactive-mode templates (`mode: interactive`) are not offered. Headless-mode templates (`mode: headless`) continue to work using the standard `claude -p` command.

</details>

## Development

### Nix (recommended)

The repo includes a Nix flake that provides the full toolchain. With [direnv](https://direnv.net/) installed:

```sh
direnv allow
```

Or enter the dev shell manually:

```sh
nix develop
```

This gives you cargo, clippy, rustfmt, and rust-analyzer at pinned versions.

To run all checks (clippy, tests, formatting):

```sh
nix flake check
```

### Without Nix

```sh
cargo build
cargo test
```
