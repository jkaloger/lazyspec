---
title: "Architecture Overview"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, overview]
related:
  - related-to: "docs/rfcs/RFC-001-my-first-rfc.md"
  - related-to: "docs/rfcs/RFC-002-ai-driven-workflow.md"
---

# Lazyspec Architecture

Lazyspec is a CLI + TUI tool for managing project documentation as version-controlled
markdown files with YAML frontmatter. It keeps documentation organized, linked, and
validated alongside code.

**Version:** 0.4.1 | **Language:** Rust (Edition 2021) | **Binary:** single self-contained executable

> [!NOTE]
> This architecture grew out of [RFC-001: Core Document Management Tool](../../rfcs/RFC-001-my-first-rfc.md),
> with agent integration driven by [RFC-002: AI-Driven Development Workflow](../../rfcs/RFC-002-ai-driven-workflow.md).

## Architecture Documents

| Document | Covers |
|---|---|
| [ARCH-002: Data Model](../ARCH-002-data-model/index.md) | Documents, relationships, frontmatter schema, configuration |
| [ARCH-003: Engine](../ARCH-003-engine/index.md) | Store, validation, @ref expansion, symbol extraction, caching |
| [ARCH-004: CLI](../ARCH-004-cli/index.md) | Command routing, output modes, config schema |
| [ARCH-005: TUI](../ARCH-005-tui/index.md) | Event loop, threading, app state, rendering, diagram support |

## C4 Context

Lazyspec sits between developers/agents and the project filesystem. It reads and writes
markdown documents, shells out to `git` for ref expansion, and optionally spawns `claude`
for agent workflows.

```d2
direction: right

user: Developer {
  shape: person
}

agent: AI Agent {
  shape: person
  style.stroke: "#888"
}

lazyspec: lazyspec {
  shape: hexagon
  style.fill: "#e8f0fe"

  cli: CLI
  tui: TUI
  engine: Engine
}

filesystem: Project Filesystem {
  shape: cylinder
  docs: "docs/**/*.md"
  config: ".lazyspec.toml"
  templates: ".lazyspec/templates/"
}

git: Git {
  shape: cylinder
}

editor: $EDITOR {
  shape: rectangle
}

claude: Claude CLI {
  shape: rectangle
  style.stroke: "#888"
  style.stroke-dash: 3
}

d2_tool: d2 / mmdc {
  shape: rectangle
  style.stroke: "#888"
  style.stroke-dash: 3
}

user -> lazyspec.cli: "commands + flags"
user -> lazyspec.tui: "keyboard input"
agent -> lazyspec.cli: "--json output"
lazyspec.engine -> filesystem: "read/write docs"
lazyspec.engine -> git: "git show (ref expansion)"
lazyspec.tui -> editor: "open file for editing"
lazyspec.tui -> claude: "spawn agent sessions" {
  style.stroke-dash: 3
}
lazyspec.tui -> d2_tool: "render diagrams" {
  style.stroke-dash: 3
}
```

## C4 Container

The binary is structured into three domains: Engine (core logic), CLI (command-line
interface), and TUI (terminal user interface). The CLI and TUI both depend on Engine
but never on each other.

The entry point routes between CLI and TUI based on whether a subcommand was given:

@ref src/main.rs

```d2
direction: down

engine: Engine {
  style.fill: "#e8f0fe"

  config: Config
  store: Store
  document: Document
  validation: Validation
  refs: Refs
  symbols: Symbols
  cache: Cache
  template: Template

  config -> store: "type definitions"
  store -> document: "parse frontmatter"
  store -> validation: "validate_full()"
  refs -> symbols: "extract symbol"
  refs -> cache: "read/write expanded"
}

cli: CLI {
  style.fill: "#fce8e6"

  mod: "Command Router (Clap)"
  commands: "15 subcommands"
  style_mod: "Style + JSON output"

  mod -> commands
  commands -> style_mod
}

tui: TUI {
  style.fill: "#e6f4ea"

  event_loop: "Event Loop"
  app: "App State"
  ui: "Renderer (Ratatui)"
  diagram: "Diagram Renderer"
  agent_mod: "Agent Spawner" {
    style.stroke-dash: 3
  }

  event_loop -> app: "dispatch events"
  app -> ui: "draw()"
  app -> diagram: "render d2/mermaid"
  app -> agent_mod
}

main: "main.rs" {
  shape: rectangle
  style.fill: "#fff3e0"
}

main -> cli: "Some(command)"
main -> tui: "None (no args)"
cli -> engine.store: "Store::load()"
tui -> engine.store: "Store::load()"
tui -> engine.refs: "expand refs async"
```

## Key Dependencies

@ref Cargo.toml

## Feature Flags

| Flag | Effect |
|---|---|
| `agent` | Enables TUI agent integration (Claude CLI spawning, agent history) |
