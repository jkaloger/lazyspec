---
title: Nested document bundles and workflow packs
type: rfc
status: in-progress
author: Jack Kaloger
date: 2026-09-28
tags: []
related:
- related-to: RFC-062
- related-to: RFC-070
- related-to: BUG-034
reviewed: a6983d9cb6cbf9c5c2a647cfca0703ff0b38da8b
---

## Summary

Let a document be a **folder of parts**. A type whose template is a directory (`.lazyspec/templates/change/` holding `index.md`, `design.md`, `arch.md`, `index.yaml`) scaffolds that whole directory on `create`. The `.md` parts carry no frontmatter of their own: they are sections of the parent document, sharing its id, status, relations and lifecycle. Non-markdown files are sidecars, copied and otherwise ignored. `show` lists parts and can concatenate them; `validate` warns when a part the template declares is missing. Nothing is added to `.lazyspec.toml`: the template directory is the declaration. On top of that, `init --template <path-or-url>` copies a pack's config and templates into a project, so a bundle-shaped workflow like OpenSpec's or the Autobots' is one command to adopt.

## Motivation

1. **Nested groups exist but are hand-assembled.** `subdirectory = true` plus `create --parent` already places children beside an `index.md`. A workflow that always wants the same four files under a change runs four `create` calls per change and relies on a skill to remember them. OpenSpec's whole model is such a group: `proposal.md`, `design.md`, `tasks.md` per change. Kiro's is `requirements.md`, `design.md`, `tasks.md`. Both are one document in several files.
2. **Every child today is a separate document.** It has its own frontmatter, status, relations and id. That is right for an ad hoc child (a task under a change) and wrong for a fixed part (the design section of a change). A part should not have a lifecycle of its own or need linking to its parent; it is the parent.
3. **A frontmatter-less `.md` beside `index.md` is a parse error** (reported three times over). The loader has no concept between "document" and "broken".
4. **Nothing checks the group is complete.** OpenSpec derives per-artifact state from file existence. Lazyspec has no rule saying "a change needs its design part".
5. **A workflow has no install vehicle.** A pack is a `.lazyspec.toml`, a templates dir and skills. Skills ship as a plugin. Config and templates have nothing: `init --template` accepts only `starter`, and `extends` moves document storage into the extended clone, which is doc-set sharing, not config sharing.
6. **Child addressing is inconsistent** (BUG-034). Parts are addressed through the parent, so they sidestep it, but ad hoc children still need it fixed.

## Goals

- A template directory `{type}/` scaffolds every file it contains on `create <type>`, with `{title}`, `{author}`, `{date}`, `{type}` substituted in `.md` files. `--json` reports the parent record plus a `parts` array.
- A `.md` in a document's folder with no frontmatter is a **part** of that document: not a document, not an error. It shares the parent's frontmatter entirely.
- Non-markdown files in the folder are sidecars: copied from the template on create, listed by `show`, never parsed.
- `show <id>` lists parts beside children; `show <id> --parts` concatenates `index.md` and every part in template order. `show --json` carries `parts: [{name, path}]` and, under `--parts`, each part's body.
- `update <id> --part <name> --body-file` writes one part, so the CLI stays the only writer.
- `search` indexes part bodies under the parent's id.
- `validate` reports a part or sidecar the template directory declares but the document lacks, as a warning.
- `init --template <path-or-url>` writes a pack's `.lazyspec.toml` and templates into the current project without touching document storage.
- The TUI's create path scaffolds the same directory; its preview shows the concatenated form.
- README documents template directories, parts and sidecars alongside `subdirectory` and `create --parent`.

## Non-goals

- Per-part status, relations or lifecycle. A part that needs those is a child document; make it one with `create --parent`.
- Per-part `required` severity. Every file in the template directory is expected; the finding is a warning. A project that wants an optional part leaves it out of the template.
- Multi-level nesting. Parts and children sit one level under the parent, as today.
- Packed context (`context --pack`); RFC-070 owns it. The `full` tier body should be the concatenated form once both land.
- Merging delta documents into canonical specs. That is a skill over `show --json` bodies.
- Skill distribution. The plugin marketplace already does it.
- Migrating existing documents. A type gains parts when its template becomes a directory; older documents without them get the warning.

## Design

### Template directory as bundle declaration

Today template resolution is `{type}.md`, then `template.md`, then built-in. Add a first step: if `{type}/` exists in the templates dir, it is the template, and the type is implicitly `subdirectory = true` for scaffolding. `{type}/index.md` is the parent's template and must exist. Every other file is copied: `.md` files with substitution, anything else verbatim. No config key; `config --json` reports `template: "directory"` for the type so agents and the TUI know.

Declaring `subdirectory = false` on a type whose template is a directory is a config error. A directory template with no `index.md` is a config error.

### Parts

The loader's rule for a `.md` inside a document's folder becomes:

| File | Has frontmatter | Loads as |
|---|---|---|
| `index.md` | yes | the document |
| `*.md` | yes | a child document (today's behaviour) |
| `*.md` | no | a **part** of the document |
| anything else | n/a | a sidecar |

A part has a `name` (its stem) and a `path`. It has no id, no status, no relations. `DocMeta` gains `parts: Vec<Part>` and `sidecars: Vec<PathBuf>`. Staleness, `governs`, `reviewed` and every frontmatter-derived fact belong to the parent and apply to its parts.

Parts order: template directory order (sorted by filename) for parts the template names, then any extra parts alphabetically. Extra parts are allowed; they are not a finding.

### Show and update

- Human `show` prints a `Parts:` block after `Children:`, one line per part and sidecar.
- `show --parts` renders `index.md` body, then each part body under a `## <name>` heading in parts order. `-e` expands `@ref` in parts too.
- `show --json` always carries `parts` and `sidecars`; `--parts` adds `body` to each part entry.
- `update <id> --part <name> --body-file <f>` writes that part. `--part` with no such file creates it. `update --body` without `--part` writes `index.md` as today.
- `search` reads part bodies into the corpus under the parent.

### Validation

New rule `missing-part`: for each document whose type template is a directory, every file the directory declares (other than `index.md`) that is absent from the document's folder yields a warning `{path, part}`. `validate --id` scopes it. The parse error for a frontmatter-less `.md` is removed for files inside a document folder; it stays for a top-level file in a type's `dir`. The triple-report on parse errors is fixed alongside.

### Workflow packs

`init --template <path-or-url>` resolves a directory (or clones a URL into `.lazyspec/cache/config/`, the clone path `extends` already uses) and copies its `.lazyspec.toml` and `.lazyspec/templates/` into the project. It refuses to overwrite an existing `.lazyspec.toml` without `--force`. Storage stays local. `extends` is untouched.

A pack repo is: `.lazyspec.toml`, `.lazyspec/templates/` (flat templates and directory templates), `README.md`, and optionally a plugin manifest for skills. Because the bundle lives in templates, a pack carries its whole document shape with no config beyond types and edges. The OpenSpec-shaped pack is a `change` type with `change/{index,proposal,design,tasks}.md` and a `delta` type created per affected spec with `create --parent`.

## Interfaces

- Template resolution: `{type}/` directory before `{type}.md` @draft.
- `DocMeta.parts: Vec<Part { name, path }>`, `DocMeta.sidecars: Vec<PathBuf>` @draft. Serialised on `show --json`, `list --json`, `context --json`.
- `show <id> --parts` @draft; `--json` shape gains `parts[].body` under it.
- `update <id> --part <name> --body|--body-file` @draft.
- `create <type> --json` gains `parts` and `sidecars` @draft when the template is a directory.
- `validate` rule `missing-part` @draft.
- `config --json` per-type `template: "file" | "directory"` @draft.
- `init --template <path-or-url> [--force]` @draft; `starter` keeps working as a built-in name.
- Loader: frontmatter-less `.md` in a document folder classified as a part, not a parse error.

## Decisions (ADRs to emit)

- A bundle is declared by a template directory, not by config. Templates already own document shape; config owns graph shape.
- A part shares its parent's frontmatter completely. Anything needing its own status or relations is a child document, not a part.
- Packs install by copy (`init --template`), not by reference (`extends`). Config sharing and doc-set sharing stay separate mechanisms.

## Stories

1. Loader classifies parts and sidecars; `DocMeta` carries them; frontmatter-less files in a document folder stop being parse errors; triple-report fixed.
2. Directory templates: resolution, `create` scaffolding, `--json` shape, TUI create path.
3. `show` parts block and `--parts` concat; `show --json` shape; `update --part`; `search` corpus.
4. `missing-part` validate rule, `--id` scoping, `status --json`, TUI warnings panel.
5. `init --template <path-or-url>`; README; an `openspec` pack repo as the first consumer.

BUG-034 is independent: parts never need a child id. It still gates a usable `delta`-style ad hoc child in the same folder.

## Risks and tradeoffs

- **Frontmatter presence as the discriminator.** A child document whose author forgot the frontmatter silently becomes a part. Accepted: today it is a parse error either way, and `create --parent` always writes frontmatter, so only a hand-written file hits this. `validate` could flag a part whose first line is a `# Title` matching no template part; deferred.
- **No per-part severity.** Every template part is a warning if missing. A project that wants a hard error can wrap `validate --json` in CI. Revisit if two projects ask.
- **Templates now carry semantics.** A templates dir was inert content; a directory in it now changes scaffolding and validation. Documented in README and reported in `config --json` so nothing is hidden.
- **`init --template` overlaps with the wizard.** RFC-062's wizard designs a config interactively; a template skips it. They compose: `--template` seeds, the wizard can still run on a TTY to tweak.
