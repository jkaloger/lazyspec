---
title: AgentRunner trait as subprocess seam with claude -p as sole v1 impl
type: adr
status: draft
author: jkaloger
date: 2026-06-18
tags: []
related:
- related-to: RFC-046
---

## Context

`AgentSpawner::spawn` in `src/tui/agent.rs` constructs the agent process inline with `Command::new("claude").args(["-p", prompt])`. Two problems follow. There is no seam to substitute the subprocess in tests, so spawning behaviour is exercised only by actually launching `claude`. And the runtime is fixed to one binary; admitting another (pi, opencode server) means editing the spawner.

## Decision

Define an `AgentRunner` trait at the subprocess-spawn boundary. The justification is dictum 4: spawning a child process is I/O, and I/O boundaries are defined by traits so production and test code share one interface. This is not a dictum 6 argument -- the trait is warranted by the test seam alone, independent of how many runtimes exist.

v1 ships exactly one implementation, `ClaudeP`, which runs `claude -p`. pi and opencode-server are the concrete future implementations that fix the trait's shape, but neither ships in v1. `AgentSpawner` keeps ownership of history records, polling, and status, and delegates process creation to an injected `AgentRunner`.

Rejected: leaving `Command::new("claude")` inline (no test seam, no pluggability). Rejected: a closed `enum Runtime` (a fixed set does not match the open, plugin-style growth toward pi/opencode and forces a match arm edit per backend).

## Consequences

- Tests inject a fake `AgentRunner` and assert on `AgentContext` without launching a process.
- A new runtime adds one trait impl and touches no spawner code.
- The TUI constructs the concrete `ClaudeP` and hands it in; the engine stays free of any assumption about which binary runs.

