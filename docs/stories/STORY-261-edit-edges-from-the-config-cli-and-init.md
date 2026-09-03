---
title: Edit edges from the config CLI and init
type: story
status: complete
author: Jack Kaloger
date: 2026-08-29
tags: []
related:
- implements: RFC-067
---
As an agent, I want to declare and modify edges through the config CLI, so that I can shape a project's DAG without writing TOML by hand.

Parity with STORY-260: per CLAUDE.md, a capability in the TUI exists in the CLI and the web view, and the reverse.

## Acceptance criteria

- Given a project, when the agent adds an edge via the config CLI, then it is written and `config --json` reflects it.
- Given an existing edge, when the agent modifies any field including the target set, then the change is written without disturbing unrelated config.
- Given an edge, when the agent removes it, then it is gone and nothing else changes.
- Given any of these commands, when `--json` is passed, then the result is machine-readable, per dictum 2.
- Given an invalid mutation, when attempted, then it is refused with the loader's own error message and the config is left untouched.
- Given `lazyspec init`, when a project is scaffolded, then it writes a starter `[[edges]]` set that loads cleanly and reproduces today's default behaviour.
- Given the README, when this story lands, then the new commands and the `[[edges]]` schema are documented.

## Notes

Touches `src/cli/config.rs`, `src/cli/init.rs`, and `src/engine/config_write.rs`.

The starter set is where wildcards earn themselves: `init` should emit a handful of readable rows, not one row per type pair.
