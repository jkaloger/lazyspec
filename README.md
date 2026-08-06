<h1 align="center">
  🤖
  <br>lazyspec
</h1>
<p align="center">
    Feature-rich documentation CLI & TUI for humans & LLMs.
    <br>A context engine that unifies git-tracked markdown, GitHub issues, and more.
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

Lazyspec manages project documentation as version-controlled markdown files with YAML frontmatter. Documents live in your repo, so agents and humans read from the same source of truth. You define the document types, their relationships, and their lifecycle in `.lazyspec.toml`; lazyspec creates, links, validates, and serves them, and every command supports `--json` output for automation.

## Install

```sh
cargo install lazyspec
```

<details>
<summary>Other install methods & shell completions</summary>

### Nix

```sh
nix profile install github:jkaloger/lazyspec
```

### From source

```sh
git clone https://github.com/jkaloger/lazyspec
cd lazyspec
cargo install --path .
```

### Shell completions

Generate and source a completion script for your shell:

```sh
# zsh
source <(lazyspec completions zsh)

# bash
source <(lazyspec completions bash)

# fish
lazyspec completions fish | source
```

Add the appropriate line to your shell profile (`~/.zshrc`, `~/.bashrc`, etc.) to load completions on startup. Completions cover subcommands, flags, document IDs, and relationship types.

</details>

## Quick start

Initialise a new project, then launch the TUI:

```sh
lazyspec init
lazyspec
```

`init` writes a `.lazyspec.toml` and templates. Running `lazyspec` with no subcommand opens the interactive dashboard. From there, or from the CLI:

```sh
lazyspec create rfc "Adopt event sourcing"   # create a document
lazyspec list                                # list documents
lazyspec validate                            # check links and frontmatter (non-zero on error)
```

> [!TIP]
> Check the `examples/` directory for a complete project setup including config, templates, and agent skill definitions you can use as a starting point.
> This repo dogfoods lazyspec, so you can also browse `docs/` or run `lazyspec` from this repo.

<details>
<summary><h2>Features</h2></summary>

- Create, update, link, and validate documents. Config-driven relationships (the starter set is `implements`, `supersedes`, `blocks`, `related-to`) keep the chain explicit.
- Catch broken links, orphaned documents, and incomplete frontmatter before they rot. `lazyspec validate` exits non-zero on errors, so it slots into CI.
- Embed `@ref` directives in your specs to point at source code. Lazyspec expands them inline using `git show`, with symbol-level extraction for Rust and TypeScript.
- Fuzzy search, markdown preview, live file watching, and document creation without leaving the terminal.
- Every command supports `--json` output for automation and agent integration.
- Define your own types, templates, and directory layout in `.lazyspec.toml`.

</details>

<details>
<summary><h2>Skills & agent integration</h2></summary>

Lazyspec ships a set of config-driven generic verb skills that enforce its workflow against whatever document types your `.lazyspec.toml` defines. The `lazy` router is the entry point: it reads the configured lifecycle DAG and the user's position, then dispatches the right verb.

| Skill      | Purpose                                                                           |
| ---------- | --------------------------------------------------------------------------------- |
| `lazy`     | Entry-point router: reads the DAG and position, dispatches the right verb         |
| `scaffold` | Create a new document's file and frontmatter, hand the body back to the human     |
| `co-write` | Collaboratively draft a document body: AI proposes, human edits, iterate          |
| `generate` | Author a full document body from context (only when the type's ceiling allows it) |
| `advance`  | Move a document to its next status along the type's lifecycle DAG, checking gates |
| `execute`  | Carry out the work a delivery document describes against its tasks and ACs        |
| `review`   | Critique a document against its intent and acceptance criteria before advancing   |

### Installing skills

`skills install` places the embedded skill set into the project. It works with or without a `.lazyspec.toml`, and never creates one:

```sh
lazyspec skills install                      # both runtimes (default)
lazyspec skills install --runtime claude     # .claude/skills/ only
lazyspec skills install --runtime agents-md  # ./AGENTS.md only
```

For Claude, each skill is written under `.claude/skills/<verb>/SKILL.md`, and the router is installed under the configured `[skills] entry` name (default `lazy`). For other agents, the same prose is concatenated into `./AGENTS.md`. Re-running is idempotent. Configure the router name via `[skills] entry` in `.lazyspec.toml`.

### Install as a Claude Code plugin

Claude Code users can install the skills and the convention hook together through the plugin marketplace hosted in this repo, instead of running `skills install` and hand-editing settings. Two commands:

```
/plugin marketplace add jkaloger/lazyspec
/plugin install lazyspec@lazyspec
```

This loads every on-disk skill under `skills/` and registers a `UserPromptSubmit` hook that injects the project convention (`lazyspec convention --preamble`) into the agent's context on each prompt.

**Prerequisite:** the `lazyspec` binary must be on `PATH`. The hook shells out to it. Without the binary the hook is a silent noop, and in any directory lacking a `.lazyspec.toml` it injects nothing.

The plugin is an additional channel, not a replacement for `skills install`. Use `skills install` when you need the `AGENTS.md` target or a renamed router entry via `[skills] entry`: a static plugin ships the default `lazy` entry and the Claude runtime only.

</details>

<details>
<summary><h2>TUI</h2></summary>

Running `lazyspec` with no subcommand opens the interactive dashboard. It provides fuzzy search, markdown preview, document creation, and live file watching: documents update automatically when changed on disk. An external edit of `.lazyspec.toml` (for example a `git pull`) reloads the running session automatically; press `R` to reload it manually. Press `?` for the full keybindings overlay.

| Key                 | Action                                              |
| ------------------- | --------------------------------------------------- |
| `j` / `k`           | Navigate up/down                                    |
| `h` / `l`           | Switch document type                                |
| `g` / `G`           | Jump to top/bottom                                  |
| `Ctrl-d` / `Ctrl-u` | Half page down/up                                   |
| `Space`             | Expand/collapse                                     |
| `Tab`               | Cycle preview tab                                   |
| `Enter`             | Open document / follow relation                     |
| `n`                 | Create new document                                 |
| `e`                 | Edit document in `$EDITOR`                          |
| `o`                 | Open externally (browser or `[tui]` viewer)         |
| `d`                 | Delete document                                     |
| `s`                 | Change status                                       |
| `r`                 | Add relation                                        |
| `p`                 | Provenance                                          |
| `R`                 | Reload config from `.lazyspec.toml`                 |
| `a`                 | Agent (only with the `agent` cargo feature)         |
| `x`                 | Toggle wrap                                         |
| `/`                 | Fuzzy search                                        |
| `w`                 | Warnings / validation panel                         |
| `` ` ``             | Cycle view (documents / filters / graph / settings) |
| `5`                 | Open the Settings view                              |
| `?`                 | Toggle keybindings help                             |
| `q` / `Ctrl-c`      | Quit                                                |

### Settings view

Press `5` (or cycle to it with `` ` ``) to open the Settings view, which edits `.lazyspec.toml` in place. Categories are listed on the left; the right panel shows the fields (or entries) of the selected category. Saving rewrites `.lazyspec.toml`, preserving its comments and formatting, after validating the whole config. An invalid config is reported and not written.

| Key            | Action                                                                                                                                                     |
| -------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `h` / `l`      | Switch category (also `Left` / `Right`)                                                                                                                    |
| `j` / `k`      | Move between fields / entries (also `Down` / `Up`)                                                                                                         |
| `Enter`        | Drill into a collection entry, or start editing a field                                                                                                    |
| `n`            | Add a new entry to a collection (Document Types / Relationships / Validation Rules seed a default and drill in; Certification prompts for a spec-path key) |
| `d`            | Delete the selected collection entry, behind a confirm (refuses the last relationship)                                                                     |
| `Space`        | Toggle a boolean / cycle an enum field                                                                                                                     |
| `g`            | When a dependency section is auto-scaffolded (for example cycling numbering to `sqids`), jump to the required field it needs filled                        |
| type + `Enter` | Confirm a text / number / duration / list edit                                                                                                             |
| `Esc`          | Cancel an in-progress edit, or undrill from an entry                                                                                                       |
| `w` / `Ctrl-S` | Save changes to `.lazyspec.toml` (validates the whole config)                                                                                              |
| `q` / `Esc`    | Quit; with unsaved changes, prompts `(s)ave / (d)iscard / (Esc) cancel`                                                                                    |

### Graph view

Cycle to the Graph view with `` ` ``. The left panel is a pivot picker (`h` / `l` to re-root the forest on a document type or a tag, or `All` for the whole store). The right panel renders the dependency forest as a nested table sharing the documents table's styling (git-status gutter, slim `ID` column, selection bar, scrolling). The `DOC` column is the document tree, with indentation and connector art showing the `implements` lineage; each configured column follows. A document reachable from more than one parent (a diamond) is drawn once under each parent; cyclic edges are hidden. Pivoting on a type or tag also nests each anchor's chain ancestors below it as an inverted subtree, so a leaf-type pivot reads top-down instead of as a flat list; those rows carry `↑` where a forward child carries `▶`. Siblings under a shared parent can be sorted by any column while the parent grouping and topological order are preserved.

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

The documents table's columns are configured under `[tui.table]`:

```toml
[tui.table]
# Columns rendered to the right of the fixed ID and DOC columns. Each id is
# either a built-in (`status`, `tags`, `assignee`, `provenance`, `related`) or a
# declared attribute name (`[[types.attributes]]`). An attribute not
# declared/present on a row's type renders as an empty cell; the `assignee`
# column is blank for unassigned documents.
columns = ["status", "tags", "assignee", "provenance"]   # default
```

The key carries a default matching today's layout, so a config without a `[tui.table]` block renders the table unchanged.

Status colours (used in both the documents table and the Graph view's `status` column) are configured under `[tui.status_colors]`, mapping a status name to a colour:

```toml
[tui.status_colors]
draft = "yellow"
in-progress = "cyan"
blocked = "#cc4444"
```

A colour is either a named ANSI colour (case-insensitive: `black`, `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `gray`/`grey`, `darkgray`/`darkgrey`, `white`, or the light variants `lightred`, `lightgreen`, `lightyellow`, `lightblue`, `lightmagenta`, `lightcyan`) or a `#rrggbb` hex string. An invalid colour value is skipped, falling through to the next source below.

A status's colour resolves in this order: this `[tui.status_colors]` config, then a synced ClickUp status-colour cache, then built-in defaults for the standard statuses (`draft`, `review`, `accepted`, `in-progress`, `complete`, `rejected`, `superseded`), then a deterministic hashed-palette fallback. Even unknown or custom statuses always render with a stable, visible colour. The block is optional; omitting it means statuses resolve via the remaining sources.

The external viewer used to open documents without a web URL (via `show --open`) is configured with a `viewer` key under `[tui]`:

```toml
[tui]
viewer = "glow"
```

This command is spawned with the document's file path as its argument when the document has no browser URL (a `git-ref`/`clickup-tasks` doc, or a filesystem doc whose repo coordinates don't resolve). The key is optional; without it, `show --open` on such a document reports an error instead of guessing a viewer.

</details>

<details>
<summary><h2>CLI</h2></summary>

All document management is available as subcommands. Most accept `--json` for machine-readable output, including every mutating command: `create`, `update`, and `tag` emit the resulting document, while `delete`, `link`, `unlink`, `ignore`, and `unignore` emit a structured outcome (`action` plus the doc id/path or relation edge) instead of the human confirmation line.

Every mutation's `--json` output also reports whether the change reached the document's remote: a `"synced"` boolean (`true` when the backend push landed, `false` when only the local write succeeded), and, only when `"synced": false`, a `"warnings"` array carrying the push-failure message (naming the remote and doc). This matters for the `git-ref` store, whose push is deferred and can fall back to a local-only write when the remote is unreachable; synchronous backends (filesystem, GitHub, ClickUp) always report `"synced": true`. In non-`--json` mode the same warning is written to stderr, so the machine-readable channel and the human channel stay in sync.

| Command                                                                                           | Description                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `init [--non-interactive] [--json] [--template starter]`                                          | Initialise lazyspec in the current project. On a TTY runs an interactive wizard that defaults to designing a blank DAG; `--template starter` (or picking `starter` on the first screen) tweaks the built-in starter DAG instead. `--non-interactive`/`--json`/non-TTY write the starter config unchanged (`--template` is ignored)                                                                                                                                                                                                                                                                                                                       |
| `create <type> <title> [--author X] [--parent ID] [--body / --body-file]`                         | Create a document (rfc, adr, story, iteration); seed body inline, from a file, or `-` for stdin. `--parent <ID>` makes the new doc a child of an existing doc; the child must be the same store as its parent. For filesystem-store types the child is authored as a sibling `.md` inside the parent's subdir (promoting a flat parent to `TYPE-n-slug/index.md` on the first child). For `github-issues`-store types the child is created as a real GitHub issue and bound as a native sub-issue of the parent at create time; a later `fetch` mirrors them into the nested cache layout (`.lazyspec/cache/<type>/<PARENT>/index.md` + `NN-<child>.md`) |
| `list [type] [--status X]`                                                                        | List documents with optional filters; each list card shows the assignee (as `@name`) when one is set, and nothing when unset                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `show <id> [-e] [--open]`                                                                         | Display a document by path or shorthand ID (for example `RFC-001`); `--open` opens it in a browser or viewer instead. The detail header adds an `Assignee:` line when the document has one (omitted when unset), mirroring the `Tags:` line. The `--json` output includes an `assignee` field (a string, or `null` when unset) alongside `status`/`tags`; `status --json` reports it for every document too                                                                                                                                                                                                                                              |
| `update <path> [--status X] [--title X] [--assignee X] [--body / --body-file] [--attr key=value]` | Update frontmatter and/or body content (`--body-file -` reads stdin); `--assignee` sets the first-class assignee field (pass `--assignee ""` to clear it); `--attr` (repeatable) sets a declared custom attribute, coerced and validated against its type; works for all stores                                                                                                                                                                                                                                                                                                                                                                          |
| `delete <path>`                                                                                   | Delete a document                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `link <from> <rel> <to>`                                                                          | Add a typed relationship (canonical or inverse keyword)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `unlink <from> <rel> <to>`                                                                        | Remove a relationship (canonical or inverse keyword)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `tag add <id> <tags>...`                                                                          | Add tags to a document (auto-creates GitHub labels if needed)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `tag remove <id> <tags>...`                                                                       | Remove tags from a document                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `search <query> [--doc-type X]`                                                                   | Full-text search across all documents                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `context <id> [--depth N]`                                                                        | Show the full document chain (RFC -> Story -> Iteration)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `context [--anchor TYPE]`                                                                         | Emit the context forest (omit `<id>`); `--anchor` re-roots on a type, nesting each anchor's chain descendants and its inverted chain ancestors below it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `status`                                                                                          | Show full project status with all documents and validation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `ignore <path>`                                                                                   | Mark a document to skip validation                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `unignore <path>`                                                                                 | Remove validation skip from a document                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `validate [--warnings]`                                                                           | Check document integrity and link consistency                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `fix [paths] [--dry-run] [--type X]`                                                              | Fix documents with broken or incomplete frontmatter; `--type` filters to a single document type                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `fix --renumber <sqids\|incremental> [--type X] [--dry-run]`                                      | Renumber all documents to the given format; `--type` filters to a single document type                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `fix --config [--dry-run]`                                                                        | Repair `.lazyspec.toml` (inject missing standard relationships/rules)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `completions <shell>`                                                                             | Generate a shell completion script (bash, elvish, fish, powershell, zsh)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| `pin <id>`                                                                                        | Pin blob hashes onto `@ref` directives in a document                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `fetch [--type X]`                                                                                | Fetch remote documents into the cache (`github-issues`, `github-milestones`, `git-ref`, `clickup-tasks` types)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `convention [--preamble] [--tags X]`                                                              | Show convention and dictum content; `--preamble` omits the dictum, `--tags` filters it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `skills install [--runtime <claude\|agents-md>]`                                                  | Install the embedded agent skill set into the project (both runtimes by default)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `config [--json]`                                                                                 | Print the resolved `.lazyspec.toml` as JSON                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `config schema`                                                                                   | Print a JSON Schema for `.lazyspec.toml` (runs from any directory)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `config add-type <name> <plural> <dir> <prefix>`                                                  | Append a new document type to `.lazyspec.toml` (bare, on a TTY, runs the interactive wizard)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `config set-lifecycle <type> [--state X] [--edge from:to]`                                        | Replace a type's lifecycle states and edges                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `config add-gate <rule> --status X`                                                               | Set the `require_parent_status` gate on a parent-child rule                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `provenance add <id> <citation>`                                                                  | Append a citation to a document's provenance list                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `provenance remove <id> <citation>`                                                               | Remove an exact-match citation from a document's provenance list                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `provenance list [id]`                                                                            | List citations for a document, or for all documents grouped by id                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `reservations list`                                                                               | Show all reservation refs on the remote                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `reservations prune [--dry-run]`                                                                  | Remove refs for documents that already exist locally                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `setup`                                                                                           | Validate GitHub auth and fetch issues for `github-issues` types                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `setup clickup [--token pk_...]`                                                                  | Validate a ClickUp personal API token and store it globally (see [ClickUp store auth](#clickup-store-auth))                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |

### Relationship keywords

`link` and `unlink` resolve relationship names against the `[[relationships]]` block in your `.lazyspec.toml` (see [Configuration](#configuration)). The starter config declares the canonical set (`implements`, `supersedes`, `blocks`, `related-to`) and, for each directional relationship, an inverse keyword (`implemented-by`, `superseded-by`, `blocked-by`), but the vocabulary is yours to change. An inverse keyword is a write-time alias: it flips the direction and stores the canonical relation on the target document. Nothing new is persisted; the reverse direction is still computed by the link graph.

```sh
lazyspec link STORY-9 blocked-by RFC-2
# writes `blocks: STORY-9` onto RFC-2, prints:
# Linked docs/rfcs/RFC-002-....md --blocks--> STORY-9
```

A relationship declared without an `inverse` is symmetric (like `related-to`) and has no separate inverse keyword. A keyword that matches no declared `name` or `inverse` is rejected before anything is written, and `validate` flags any document carrying a relationship name absent from `[[relationships]]`.

### `show` flags

| Flag                        | Description                                      |
| --------------------------- | ------------------------------------------------ |
| `-e`, `--expand-references` | Expand `@ref` directives into fenced code blocks |
| `--max-ref-lines N`         | Max lines per expanded ref (default: 25)         |
| `--open`                    | Open the document externally (see below)         |

`show <id> --open` opens the document in an external viewer. For a document whose backend has a web URL (a `github-issues` doc opens its issue page, a `github-milestones` doc its milestone page, a `filesystem` doc its blob on the default branch), it launches your browser (`open` on macOS, `xdg-open` on Linux). For any other document (a `git-ref` or `clickup-tasks` doc, or a filesystem doc whose repo coordinates don't resolve) it launches the command configured as `viewer` under `[tui]` in `.lazyspec.toml` (for example `viewer = "glow"`) on the document's file. If no web URL resolves and no viewer is configured, `--open` reports a clear error rather than doing nothing. With `--json`, `--open` prints the resolved target (`{ "target": "url", "url": ... }` or `{ "target": "file", "path": ... }`) and spawns nothing.

Each document entry in `show --json` and `status --json` (under `documents[]`) includes an `attributes` object holding the document's custom frontmatter attributes (declared via `[[types.attributes]]`). Declared attributes are emitted as their typed JSON value: `int`/`float` as numbers, `string`/`enum` as strings, `bool` as a boolean, `date` as a `"YYYY-MM-DD"` string. Undeclared keys pass through with their raw YAML value. The field is always present; a document with no attributes serializes it as `{}`, so consumers needn't null-check.

`show --json` and `status --json` also include a read-only `comments` array. For documents whose type uses the `github-issues` store, this fetches the issue's GitHub comment thread live (each entry `{ "author", "body", "timestamp" }`); for all other documents it is an empty array. Comments are never written back to GitHub, never merged into `body`, and never cached. The field is always present.

### `context` flags

| Flag            | Description                                                                                                                                                                                                                                                             |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--depth N`     | Max hops to follow `related-to` links when collecting related records (default: 1)                                                                                                                                                                                      |
| `--anchor TYPE` | Forest mode only (omit `<id>`): re-root the forest on documents of `TYPE`, emitting each anchor with its chain descendants nested below it and its chain ancestors below it too as an inverted subtree, marked `↑` in the human tree and `reverse_in_context` in `--json` |

With an `<id>`, `context` shows that document's chain. Omit the id to emit the whole-store context forest (every document, parents-first); add `--anchor TYPE` (for example `--anchor story`) to re-root the forest on a document type.

An anchored forest is bidirectional: each anchor-type doc is a root, with its chain descendants nested below it and its chain ancestors nested below it too as an inverted subtree. Pivoting on a leaf type therefore reads top-down (`ITERATION-246` → `STORY-184` → `RFC-058`) instead of emitting a flat list. A row reached by an inverted (ancestor) edge is marked `↑` in the human tree, in the same column its forward siblings put their connector. In `--json` it carries `reverse_in_context: true` and lists the anchor-side paths it hangs under in `inverted_parents_in_context`, so `implements_in_context` never asserts an edge pointing the other way; a node's parents in the rendered tree are the union of the two lists, and only one of them is ever populated. `inverted_parents_in_context` holds inverted parent EDGES, not "the docs that implement this one": on `--anchor story` a story's implementing iterations are forward children, so the story's list is empty. The whole-store forest has no inverted edges, so it carries neither key and is unchanged.

```sh
lazyspec context ITERATION-001            # chain for one document
lazyspec context --json                   # whole-store forest
lazyspec context --json --anchor story    # stories as roots: iterations below, RFCs inverted below
lazyspec context --anchor iteration       # leaf pivot: each iteration with its story and RFC above it
```

### `provenance` subcommands

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

- `add` / `remove`: `{ "doc": "...", "added"|"removed": "...", "provenance": [...], "synced": true|false }`. As with every mutation, `add`/`remove` also carry the push outcome: `"synced"` reflects whether the backend push landed, and a `"warnings"` array is present only when `"synced": false` (a `git-ref` local-only write against an unreachable remote). The same warning goes to stderr in non-`--json` mode.
- `list <id>`: `{ "doc": "...", "provenance": [...] }`
- `list` (no id): `{ "documents": [{ "id": "...", "path": "...", "provenance": [...] }, ...] }`

`add` rejects empty citations. `remove` is exact-match and errors when the citation is absent.

</details>

<details>
<summary><h2><code>@ref</code> syntax</h2></summary>

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

`lazyspec init` creates a `.lazyspec.toml` in your project root. On a TTY it runs an interactive wizard: by default it designs a **blank** type DAG from scratch (prompting for each type, its lifecycle, and parent-child rules), or pass `--template starter` to tweak the built-in starter set instead. `--non-interactive`, `--json`, or a non-TTY writes the starter config unchanged.

The engine ships no built-in types or vocabulary: the `[[types]]`, `[[relationships]]`, and `[[rules]]` in `.lazyspec.toml` are the sole source of truth. A missing `.lazyspec.toml` (or one with no `[[types]]`) errors and points you at `lazyspec init`; a config with no `[[relationships]]` points you at `lazyspec fix --config`.

> [!NOTE]
> `lazyspec config schema` prints a JSON Schema for `.lazyspec.toml`, derived from the actual parser so it never drifts from the binary. It is the authoritative key reference: point [taplo](https://taplo.tamasfe.dev/) or Even Better TOML at it for editor autocomplete, or read it instead of inferring keys from this README. The sections below cover the main blocks with examples.

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

### Migrating an existing config

Projects created before relationships and rules became config-driven have a `.lazyspec.toml` with no `[[relationships]]` or `[[rules]]` blocks. Strict load now rejects such a config on every command, pointing you at the migration:

```sh
lazyspec fix --config            # inject missing standard relationships/rules and default lifecycles
lazyspec fix --config --dry-run  # preview the additions without writing
```

`fix --config` reads the config leniently (the one place strict load is bypassed), then appends only the standard `[[relationships]]` / `[[rules]]` that are missing, comparing by name, so user-added relationships and rules are kept and nothing is duplicated. It also injects the default `lifecycle` into any `[[types]]` entry that lacks one (a type that already declares a lifecycle is left untouched); migrated types are reported under `lifecycles_added`. Every existing section (`[github]`, comments, ordering) is preserved, and it is idempotent: running it on an up-to-date config makes no change. The flag is config-only: no documents are touched (use plain `lazyspec fix` for frontmatter).

### Inspecting and editing the config

`lazyspec config` reads and edits `.lazyspec.toml` without you opening the file. The read is plain JSON; the three mutators reconcile the TOML in place, preserving comments, formatting, and block order exactly as `fix --config` and the TUI settings screen do; and `schema` emits a JSON Schema describing the file's shape.

```sh
lazyspec config --json                      # print the resolved config as JSON
lazyspec config schema                      # print a JSON Schema for .lazyspec.toml

# Append a new document type (name, plural, dir, prefix are positional)
lazyspec config add-type spike spikes docs/spikes SPIKE \
  --icon "◆" --parent-type rfc --intent "throwaway exploration" \
  --authorship generated

# With no positionals on a TTY, add-type prompts interactively.
lazyspec config add-type

# Declare custom frontmatter attributes with the type
lazyspec config add-type bug bugs docs/bugs BUG \
  --attribute "severity:enum:required:low,medium,high" \
  --attribute "reported:date" --attribute "estimate:int"

# Replace a type's lifecycle (states + edges; `*` matches any source state)
lazyspec config set-lifecycle iteration \
  --state draft --state in-progress --state done \
  --edge draft:in-progress --edge in-progress:done --edge "*:rejected"

# Gate child creation on a parent status (parent-child rules only)
lazyspec config add-gate stories-need-rfcs --status accepted
```

`add-type` rejects a duplicate name; `set-lifecycle` replaces the whole lifecycle (it is a set, not a merge) and rejects an unknown type; `add-gate` rejects an unknown rule and refuses a `relation-existence` rule (the gate applies only to `parent-child` rules). The mutators require an already-valid config; run `lazyspec fix --config` first to migrate a legacy one.

`config schema` needs no active project and runs from any directory (every other `config` subcommand requires a `.lazyspec.toml`). Save it as a machine-readable reference for a human or agent editing the config:

```sh
lazyspec config schema > lazyspec.schema.json
```

### Store backends

Every `[[types]]` block has a `store` (default `filesystem`) that decides where its documents live and how mutations sync. Set it with `--store` on `config add-type`. Per-store config keys are documented in `lazyspec config schema`; per-command behaviour in `lazyspec <cmd> --help`.

| Store                  | Documents are                                         | Auth                          |
| ---------------------- | ----------------------------------------------------- | ----------------------------- |
| `filesystem` (default) | Markdown files under the type's `dir`                 | none                          |
| `github-issues`        | GitHub issues (labelled `lazyspec:{type}` by default) | `gh auth login`               |
| `github-milestones`    | GitHub milestones                                     | `gh auth login`               |
| `github-projects`      | Existing Projects v2 boards (associate only)          | `gh auth login`, `-s project` |
| `git-ref`              | Docs in git custom refs, pushed live to the remote    | a writable git remote         |
| `clickup-tasks`        | Tasks in one bound ClickUp List (read/write)          | `lazyspec setup clickup`      |

Remote-backed types cache into `.lazyspec/cache/` and refresh with `lazyspec fetch [--type <name>]`. `fetch` refreshes every remote type in one pass; a per-type failure still refreshes the rest, reports the error, and exits non-zero. `git-ref` mutations push live with `--force-with-lease`; if the remote is unreachable the change stays local and prints a `warning:`.

`fetch` prints every warning to stderr as `warning: <message>` in both modes. `fetch --json` also prints one entry per type on stdout: `{ "type", "fetched", "new", "removed" }`, plus a `"warnings"` array repeating that type's warnings (a stale-cache fallback, a subtree the composed read could not refresh, a document whose `Status` the authority board does not set) when it produced any, and an `"error"` string when its fetch failed.

GitHub native fields (issue types, Projects v2 boards, milestone associations) need the `project` scope on your token:

```sh
gh auth refresh -s project
```

**<a id="clickup-store-auth"></a>ClickUp auth.** ClickUp has no `gh`-style CLI, so lazyspec owns its own credential store. `lazyspec setup clickup` validates a `pk_` personal API token (from ClickUp's _Settings -> Apps_) against the `/user` endpoint, then stores it keychain-first: the OS keychain by default, falling back to a `0600` `~/.lazyspec/credentials.toml` (with a loud warning) on headless boxes. The token is global, never per-repo, never committed, and redacted in all output.

```sh
lazyspec setup clickup                       # prompt (no echo)
lazyspec setup clickup --token pk_XXXXXXXX    # non-interactive
```

Deeper per-store behaviour (write-through, optimistic locking, label/tag matching, relation and custom-field mapping via keys like `github_issue_tag`, `github_label`, `status_authority`, `clickup_list_id`, `clickup_custom_field_map`, and `github_native`) is described by `lazyspec config schema` and the relevant command's `--help`.

### Custom types

Each document type is declared with a `[[types]]` block. This lets you rename the defaults, add new types, or set custom prefixes and icons used in the TUI. Directories derive entirely from each type's own `dir`; there is no separate `[directories]` table.

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

Each type declares a `lifecycle`: the set of valid statuses (`states`) and, optionally, the permitted status transitions (`edges`). `update --status` is gated by this lifecycle: a move is allowed only when an edge from the current status to the target is declared. An edge with a `*` source matches any current status (for example `* -> superseded` lets any document be superseded). Setting a status to its current value is always a no-op (idempotent, never rejected). When a move has no matching edge, `update` exits non-zero and the frontmatter is left unchanged.

`edges` is optional. A lifecycle that declares `states` but omits `edges` (or sets `edges = []`) is unconstrained: any move between declared states is allowed. Declare `edges` only when you want to constrain the order of transitions.

```toml
[[types]]
name = "rfc"
prefix = "RFC"
lifecycle = { states = ["draft", "review", "accepted", "in-progress", "complete", "rejected", "superseded"], edges = [{ from = "draft", to = "review" }, { from = "review", to = "accepted" }, { from = "accepted", to = "in-progress" }, { from = "in-progress", to = "complete" }, { from = "*", to = "rejected" }, { from = "*", to = "superseded" }] }
```

**Board-derived states.** A `github-issues` type may hand its lifecycle to one Projects v2 board by naming a `github-projects` document in `status_authority`. That board's `Status` single-select options become the type's `lifecycle` states — lowercased, in board order, with no transition edges — written into `.lazyspec.toml` by `lazyspec fetch`. No `config` subcommand sets the key; edit it by hand.

```toml
[[types]]
name = "ticket"
prefix = "TICKET"
store = "github-issues"
status_authority = "PROJECT-7"
```

Only the nominated board is authoritative. A document can belong to several boards; every other board's fields, including its own `Status`, stay plain `PROJECT-n.<field>` attributes and do not affect the lifecycle. A type that sets no `status_authority` is unaffected: it keeps its declared lifecycle, or, for `github-issues` and `github-milestones` types that declare none, the canonical `open`/`closed` pair.

Each document of a board-bound type also takes its own status from that board: `fetch` reads the document's `Status` cell on the authority board, and nothing on the issue itself — neither its open/closed state nor the status stored in its body — contributes one. A closed issue sitting in the board's `In Progress` column is therefore `in progress`, not `closed`. `fetch` makes exactly one write to the board: a document of the type whose issue is not yet an item is added to it, so the type cannot report one lifecycle for its board members and another for everyone else. No `Status` value is ever written — the added item's cell starts empty, which leaves the document with no status and a warning, as does an existing item whose cell is empty. The two warnings are worded differently because the fixes differ, and a failed add warns as well without failing the fetch. A `fetch` that cannot read the board (a token without the `project` scope) warns and leaves each document at the status it last read off the board.

`update --status` on a board-bound type moves the card: it writes the document's `Status` cell on the authority board, matching the value you pass against the board's column names case-insensitively, so `--status "In Progress"` and `--status "in progress"` are the same move. The column names and their ids come from the cached schema snapshot, so a value naming no column on the board is rejected offline — naming the valid columns — before anything is written. The GitHub issue's `open`/`closed` state is deliberately left untouched, in both directions: reaching the board's last column does not close the issue and leaving it does not reopen it. Express that coupling as Projects automation on the board itself, where the rest of your team's board rules live. The TUI status picker offers the same columns and writes through the same path.

An `update` that changes anything else on a board-bound document — its title, body or an attribute — leaves the status alone: it stays whatever the board last reported, and the issue's open/closed state cannot move it. A document whose issue is not an item of the authority board has no cell to write, so `update --status` rejects it (run `fetch`, which adds it) before touching the issue. A board write GitHub rejects — most often a token without the `project` scope, which GitHub answers with an error and HTTP 200 — fails the update rather than reporting a move that did not happen.

Because `fetch` overwrites `lifecycle`, `validate` reports a declared lifecycle the nominated board could not have produced: one carrying transition `edges`, or (once the board's columns are cached) states that are not the board's. It also reports a `status_authority` that cannot work at all: one set on a type whose `store` is not `github-issues` (only a GitHub issue can be a board item), and one whose value names no board number.

Projects whose `[[types]]` predate the lifecycle axis can backfill the default lifecycle with `lazyspec fix --config` (see _Migrating an existing config_).

### Relationships

The relationship vocabulary is config-driven, just like document types. Each `[[relationships]]` block declares a relationship `name` and an optional `inverse` keyword. A directional relationship declares its inverse (for example `implements` / `implemented-by`); a relationship with no `inverse` is symmetric (for example `related-to`). `link`/`unlink` resolve the keyword you type against this registry: a canonical `name` links in the stated direction, while a declared `inverse` flips it and stores the canonical relation on the target. `validate` flags any document carrying a relationship name not declared here.

A relationship may also declare `traversal`, which governs how it participates in context traversal: `chain` relationships form the parent-child hierarchy that `parent-child` validation rules and the context chain walk follow, while `related` relationships form the symmetric related-context neighbourhood. A relationship with no `traversal` participates in neither walk, but the target document's own declared relations still surface in the related section of `context` (and the TUI Relations tab) at one hop.

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

### Validation rules

Validation rules define structural constraints between document types. Two shapes are supported:

- `parent-child`: the child type must link to a parent type via any chain relationship (a relationship marked `traversal = "chain"` in `[[relationships]]`).
- `relation-existence`: documents of a given type must have at least one relationship.

A `parent-child` rule may also carry `require_parent_status`: when set, `create` of the child type is refused unless at least one parent document of the rule's `parent` type has reached that status. The required status must be a valid state of the parent type's lifecycle. Rules without `require_parent_status` impose no creation gate.

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

When creating a document, lazyspec resolves the template in this order: a per-type override `{type}.md` (for example `rfc.md`, `story.md`), then the shared `template.md`, then a built-in default. Add a `{type}.md` to override a single type while leaving the rest on the shared template.

### Agents

The global `[agents]` block configures interactive agent run mode. When an interactive-mode template is selected in the agent dialog, the configured shell command runs via `bash -lc` with the rendered template body exported as `$LAZYSPEC_PROMPT` and the document path as `$LAZYSPEC_DOC_PATH`.

```toml
[agents]
interactive = 'claude "$LAZYSPEC_PROMPT"'
# or 'opencode -p "$LAZYSPEC_PROMPT"', 'pi', 'tmux new-window claude "$LAZYSPEC_PROMPT"'
```

Zero-defaults: when `[agents] interactive` is unset, interactive-mode templates (`mode: interactive`) are not offered. Headless-mode templates (`mode: headless`) continue to work using the standard `claude -p` command.

</details>

<details>
<summary><h2>Web view</h2></summary>

A read-only web view of the project's documents is available behind the `web` cargo feature, so default builds carry no async/HTTP dependencies:

```sh
cargo run --features web -- serve            # binds 127.0.0.1:8787
cargo run --features web -- serve --port 9000
```

`serve` loads the store once and renders a server-side document list (grouped by type) with htmx status/tag filtering. It binds loopback only.

Each document page carries an outbound "edit on GitHub" deep-link, derived from the document's store backend: filesystem docs link to the blob (`/blob/{branch}/{path}`), `github-issues` docs to the issue, `github-milestones` docs to the milestone. The repo coordinates resolve from the `origin` remote (owner/repo) and current branch, overridable per field with an optional `[web]` table:

```toml
[web]
owner = "acme"      # optional; defaults to the origin remote's owner
repo = "widgets"    # optional; defaults to the origin remote's repo
branch = "main"     # optional; defaults to the current branch
```

When owner/repo can't be resolved (no `origin` remote and no override), deep-links are omitted and `serve` logs a single startup warning rather than rendering broken links.

### Native macOS app (deprecated)

The native macOS app (the Tauri build behind the `app` cargo feature) is deprecated and is no longer built or shipped by CI. The `app` cargo feature and its source remain in-tree but are unsupported and unbuilt in releases. crates.io (`cargo install`) is the supported install and artifact channel.

</details>

<details>
<summary><h2>Development</h2></summary>

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

</details>
