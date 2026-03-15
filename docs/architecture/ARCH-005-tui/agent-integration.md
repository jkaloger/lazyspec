---
title: "Agent Integration"
type: arch
status: accepted
author: jkaloger
date: 2026-03-15
tags: [architecture, tui, agent]
related:
  - related-to: "docs/rfcs/RFC-016-init-agents-from-tui.md"
  - related-to: "docs/stories/STORY-051-agent-invocation-from-tui.md"
  - related-to: "docs/stories/STORY-052-agent-management-screen.md"
---

# Agent Integration (Feature-Gated)

Behind the `agent` feature flag, the TUI can spawn Claude CLI sessions for
document-related tasks. See [RFC-016: Init agents from TUI](../../rfcs/RFC-016-init-agents-from-tui.md),
[STORY-051: Agent invocation from TUI](../../stories/STORY-051-agent-invocation-from-tui.md),
and [STORY-052: Agent management screen](../../stories/STORY-052-agent-management-screen.md).

## Agent Spawner

@ref src/tui/agent.rs#AgentSpawner

`AgentSpawner` manages background Claude processes:
- Spawns `claude -p <prompt> --session-id <uuid>` with restricted tool access
- Polls for completion via `try_wait()`
- Persists agent records to `~/.lazyspec/agents/` as JSON

@ref src/tui/agent.rs#AgentRecord

## Agent Actions

Two built-in prompts:

@ref src/tui/agent.rs#build_create_children_prompt

@ref src/tui/agent.rs#build_expand_prompt

## Session Resume

Completed agent sessions can be resumed interactively. The TUI leaves alternate
screen, runs `claude --resume <session_id>`, and reloads the store on return.
