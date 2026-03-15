---
title: "CLI"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, cli]
related:
  - related-to: "docs/rfcs/RFC-001-my-first-rfc.md"
  - related-to: "docs/rfcs/RFC-007-agent-native-cli.md"
  - related-to: "docs/stories/STORY-002-cli-commands.md"
  - related-to: "docs/stories/STORY-021-json-everywhere.md"
---

# CLI

The CLI module (`src/cli/`) provides the command-line interface via Clap derive macros.
Every command supports dual output: human-readable (styled) and machine-readable (`--json`).

The CLI was established in [RFC-001](../../rfcs/RFC-001-my-first-rfc.md)
and shaped by:

- [RFC-007: Agent-Native CLI](../../rfcs/RFC-007-agent-native-cli.md) drove the `--json` everywhere pattern
- [RFC-015: Lenient frontmatter loading](../../rfcs/RFC-015-lenient-frontmatter-loading-with-warnings-and-fix-command.md) added the `fix` command
- [RFC-020: Fix command numbering](../../rfcs/RFC-020-fix-command-numbering-conflict-resolution.md) extended fix capabilities

## Command Routing

```d2
direction: down

main: "main()" {
  shape: hexagon
}

parse: "Cli::parse()" {
  shape: parallelogram
}

init_check: "Init?" {
  shape: diamond
}

config: "Config::load()"
store_load: "Store::load()"

commands: "Command dispatch" {
  init: init
  create: create
  list: list
  show: show
  update: update
  delete: delete
  link: "link / unlink"
  search: search
  status: status
  context: context
  validate: validate
  fix: fix
  ignore: "ignore / unignore"
}

tui: "TUI::run()" {
  style.fill: "#e6f4ea"
}

main -> parse -> init_check
init_check -> commands.init: "yes"
init_check -> config: "no"
config -> store_load: "most commands"
config -> commands.create: "no store needed"
store_load -> commands
parse -> tui: "no subcommand"
```

All commands are defined via Clap derive:

@ref src/cli/mod.rs#Commands

Children cover:
- **commands**: All 15 subcommands in detail
- **output-modes**: Human and JSON output patterns
