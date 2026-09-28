<h1 align="center">
  🤖
  <br>lazyspec
</h1>

<img alt="screenshot of a terminal interface displaying codebase documentation, categorised by type" src="https://github.com/user-attachments/assets/91f308d1-8d03-4815-b2ec-fa445159c563" />

Lazyspec manages project documents through a TUI and a command line interface.

```bash
lazyspec [command] [options]
lazyspec
```

## Description

Lazyspec stores documents as Markdown with YAML frontmatter. `.lazyspec.toml` defines document types, relationships, lifecycle states, templates, and storage backends. The CLI creates, links, searches, and validates documents. The terminal interface provides document navigation and Markdown preview.

The project is experimental. CLI and configuration interfaces may change between releases.

## Commands

Most commands accept `--json`. `lazyspec help <command>` prints the complete options for a command.

| Command                                  | Behaviour                                                                                                                                                                                                                     |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `init`                                   | Creates `.lazyspec.toml` and templates. On a terminal, it runs the configuration wizard. `--non-interactive`, `--json`, or a non-terminal writes the starter configuration. `--template <dir-or-url>` copies a workflow pack. |
| `create <type> <title>`                  | Creates a document from a template. `--parent <id>` creates a nested document when the parent and child use the same store.                                                                                                   |
| `list [type]`                            | Lists documents, optionally filtered by type or status.                                                                                                                                                                       |
| `show <id>`                              | Displays a document. `-e` expands `@ref` directives. `--parts` includes bundle parts. `--open` opens a browser or configured viewer.                                                                                          |
| `update <id>`                            | Changes title, status, assignee, attributes, or body. `--part <name>` writes a bundle part.                                                                                                                                   |
| `delete <id>`                            | Deletes a document.                                                                                                                                                                                                           |
| `link <from> <relation> <to>` / `unlink` | Adds or removes a configured relationship. Inverse relationship names are accepted.                                                                                                                                           |
| `tag add/remove <id> <tags>...`          | Changes document tags.                                                                                                                                                                                                        |
| `search <query>`                         | Searches titles, tags, paths, and bodies. A source path also matches documents whose `governs` globs cover it.                                                                                                                |
| `why <path>`                             | Lists the documents governing a source file.                                                                                                                                                                                  |
| `context [id]`                           | Shows one document's chain or the full context forest.                                                                                                                                                                        |
| `status`                                 | Reports documents, validation findings, and pending `git` store commits.                                                                                                                                                      |
| `validate`                               | Reports document, relationship, staleness, and source ownership findings. `--id <id>` limits document findings to one document.                                                                                               |
| `fix`                                    | Repairs supported document findings. `--config`, `--governs`, and `--renumber` select other repair modes. `--dry-run` previews changes.                                                                                       |
| `govern add/remove/list`                 | Manages source file globs on a document.                                                                                                                                                                                      |
| `pin <id>`                               | Records the current Git commit as the document's review anchor and pins `@ref` directives.                                                                                                                                    |
| `provenance add/remove/list`             | Manages document citations.                                                                                                                                                                                                   |
| `fetch` / `push`                         | Refreshes remote documents / publishes commits from `git` stores.                                                                                                                                                             |
| `config`                                 | Prints the resolved configuration. `config schema` prints its JSON Schema. Other subcommands edit types, lifecycles, and edges.                                                                                               |
| `convention`                             | Prints configured convention content.                                                                                                                                                                                         |
| `setup`                                  | Configures remote store authentication.                                                                                                                                                                                       |
| `completions <shell>`                    | Prints a shell completion script.                                                                                                                                                                                             |

JSON validation output has `errors`, `warnings`, and `parse_errors` arrays. Findings include a stable `rule` field. JSON mutation output includes `synced`. A value of `false` means the write is local and has not reached its remote. For a `git` store, mutations commit locally and `push` publishes them.

### Configuration

`[[types]]` declares document types. `[[relationships]]` declares link names and optional inverse names. `[[edges]]` constrains type relationships and selects links used for chain or related traversal. A type's `lifecycle` declares its states and transitions. `config --json` prints the resolved configuration. `config schema` prints the key reference.

Each type selects a store. The default `filesystem` store writes Markdown under the type's `dir`. Other stores include `github-issues`, `github-milestones`, `github-projects`, `git-ref`, `git`, and `clickup-tasks`. Remote documents are refreshed with `fetch`. Configuration can also extend another directory or Git URL through `extends`.

Templates are read from `.lazyspec/templates/` by default. A type may use a shared `template.md`, a `<type>.md` file, or a `<type>/` directory. A directory template requires `index.md` and creates a document bundle. Markdown files beside `index.md` without frontmatter are parts of that document. Files with frontmatter are child documents.

The optional `[governs]` table defines a source root and an ownership check. Document `governs` globs identify source files covered by a document. `pin` sets the `reviewed` Git commit. Staleness can be based on age or changes under those globs. `show` reports the resulting `fresh`, `aging`, or `stale` band.

### Terminal interface

Running `lazyspec` without a command opens the terminal interface. It includes document and graph views, search, Markdown preview, validation findings, and a configuration editor. Changes to document files and `.lazyspec.toml` are detected while it runs. `?` displays the complete key list.

| Key             | Action                                  |
| --------------- | --------------------------------------- |
| `j` / `k`       | Move through rows.                      |
| `h` / `l`       | Change document type or graph pivot.    |
| `Enter`         | Open the selected document or relation. |
| `/`             | Search.                                 |
| `n` / `e` / `d` | Create, edit, or delete a document.     |
| `s` / `r`       | Change status or add a relation.        |
| `` ` ``         | Cycle views.                            |
| `5`             | Open settings.                          |
| `w`             | Open validation findings.               |
| `R`             | Reload configuration.                   |
| `?`             | Show keys.                              |
| `q`             | Quit.                                   |

### Source references

An `@ref` directive names committed source content. `show -e` expands it from Git. Rust and TypeScript symbols can be selected by name.

```text
@ref <path>
@ref <path>#<symbol>
@ref <path>#<symbol>@<sha>
@ref <path>#<line>
@ref <path>#<line>@<sha>
```

## Exit status

`validate` exits `0` when it finds no errors or parse errors, and `2` when either is present. Warnings alone do not change its exit status. Commands report other failures with a nonzero status.

## Files

| Path                   | Contents                            |
| ---------------------- | ----------------------------------- |
| `.lazyspec.toml`       | Project configuration.              |
| `.lazyspec/templates/` | Document templates.                 |
| `.lazyspec/cache/`     | Fetched documents and derived data. |
| `.lazyspec/git/`       | Shared clones for `git` stores.     |

## Examples

```sh
lazyspec init --non-interactive
lazyspec create rfc "Adopt event sourcing" --json
lazyspec create story "Record events" --json
lazyspec list --json
lazyspec link STORY-001 implements RFC-001 --json
lazyspec context STORY-001 --json
lazyspec validate --json
```

```sh
lazyspec init --template ./examples/openspec
lazyspec create change "Add caching" --json
lazyspec show CHANGE-001 --parts
```

```sh
lazyspec config schema > lazyspec.schema.json
lazyspec completions zsh > _lazyspec
```

## See also

- [Example configurations](examples/)
- [Bundled skills](skills/README.md)
- [Releases](https://github.com/jkaloger/lazyspec/releases)

## Install

The installer supports macOS and Linux and verifies a SHA-256 checksum. It installs to `~/.local/bin` by default. `LAZYSPEC_INSTALL_DIR` changes the destination. `LAZYSPEC_VERSION` selects a release.

```sh
curl -fsSL https://raw.githubusercontent.com/jkaloger/lazyspec/main/install.sh | sh
```

Cargo and Nix installations are also available:

```sh
cargo install lazyspec
nix profile install github:jkaloger/lazyspec
```

Release archives and checksums are available on the [releases page](https://github.com/jkaloger/lazyspec/releases).

## Development

The Nix flake supplies the Rust toolchain. `nix flake check` runs the project checks.

```sh
nix develop
nix flake check
```

The development binary runs through Cargo:

```sh
cargo run -- --help
cargo test
```
