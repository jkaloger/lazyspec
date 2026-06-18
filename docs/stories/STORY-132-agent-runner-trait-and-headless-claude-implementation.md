---
title: Agent runner trait and headless Claude implementation
type: story
status: accepted
author: jkaloger
date: 2026-06-18
tags: []
related:
- implements: RFC-046
---

## Context

`AgentSpawner::spawn` in `src/tui/agent.rs` builds the agent process inline with `Command::new("claude").args(["-p", prompt])` and a fixed `--allowedTools` string. There is no seam to substitute the subprocess, so spawning is exercised only by actually launching `claude`, and the runtime is locked to one binary. Per dictum 4, spawning a child process is I/O and an I/O boundary should be a trait so production and test code share one interface. This story introduces an `AgentRunner` trait at that boundary with a `ClaudeP` implementation, and refactors `AgentSpawner` to delegate process creation to an injected runner while keeping ownership of records, polling, and status. It is a seam only -- the existing agent actions behave exactly as before.

## Acceptance Criteria

### AC1: AgentRunner is the spawn seam

**Given** an `AgentRunner` is injected into the `AgentSpawner`
**When** a spawn is requested
**Then** the spawner constructs an `AgentContext` (prompt, optional allowed tools, doc path, session id) and obtains an `AgentHandle` from the runner, rather than constructing a process itself

### AC2: A fake runner can be asserted on without launching a process

**Given** a test injects a fake `AgentRunner` that records the `AgentContext` it receives
**When** the spawner is asked to spawn an agent
**Then** the fake captures the constructed `AgentContext` and returns a handle, and no real subprocess is launched

### AC3: ClaudeP runs headless claude -p

**Given** the `ClaudeP` implementation of `AgentRunner`
**When** it spawns an agent for a given `AgentContext`
**Then** it runs `claude -p <prompt> --session-id <id>` with stdin, stdout, and stderr discarded, returning an `AgentHandle` carrying the session id and child process

### AC4: ClaudeP passes allowedTools only when present

**Given** a `ClaudeP` runner
**When** it spawns an agent whose `AgentContext.allowed_tools` is `Some`
**Then** the process is invoked with `--allowedTools` set to that value
**And** when `allowed_tools` is `None`, no `--allowedTools` argument is passed

### AC5: AgentSpawner retains record lifecycle ownership

**Given** an `AgentSpawner` wired to a runner
**When** an agent is spawned
**Then** the spawner still creates and persists the `AgentRecord`, tracks the running handle, and exposes it via the active count -- the runner is not involved in record keeping

### AC6: Polling and status are unchanged

**Given** agents that have been spawned through the runner
**When** the spawner polls for finished agents
**Then** completed agents are marked `Complete`, failed agents `Failed`, and their records updated, exactly as before the refactor

### AC7: No behaviour change to existing actions

**Given** the existing TUI agent actions
**When** they are invoked after the refactor with the production `ClaudeP` runner
**Then** the spawned `claude -p` command, allowed-tools value, and record persistence are identical to the pre-refactor behaviour

## Scope

### In Scope

- An `AgentRunner` trait in the engine with `spawn(&self, ctx: AgentContext) -> Result<AgentHandle>`
- `AgentContext { prompt, allowed_tools: Option<String>, doc_path, session_id }` and `AgentHandle { session_id, child }`
- A `ClaudeP` implementation running `claude -p <prompt> --session-id <id>` with stdio discarded, passing `--allowedTools` only when `allowed_tools` is `Some`
- Refactoring `src/tui/agent.rs` `AgentSpawner` to own records, polling, and status while delegating process creation to an injected `AgentRunner` (the hardcoded `Command::new("claude")` moves into `ClaudeP`)
- A fake `AgentRunner` as the test seam, letting tests assert on the constructed `AgentContext` without launching a process

### Out of Scope

- Prompt templates, minijinja rendering, and `.lazyspec/agents/` discovery (Story 2)
- Per-type `agents` config gating and resolution to an allowed template set (Story 3)
- TUI action dialog changes (Story 4)
- Interactive run mode, the `[agents] interactive` config, and terminal handover (Story 5)
- pi / opencode-server runner implementations; the trait is shaped to admit them but none ship here
- Relocating the agent history directory
