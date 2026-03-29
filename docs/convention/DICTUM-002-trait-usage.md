---
title: "Trait Usage"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [traits, rust, engine, cli, tui]
---

## When to Use Traits

- Use traits for **testability boundaries** — the `FileSystem` trait pattern is the canonical example: real I/O in prod, injectable in tests
- Use traits for **polymorphism you actually need** — e.g., multiple store backends (filesystem, GitHub Issues)
- Don't introduce a trait for a single implementation. If there's only one impl and no testing seam, use a concrete type

## Design

- Trait methods should be minimal — prefer several small traits over one fat trait. A consumer should never need to implement methods it doesn't use
- Default implementations are fine when there's an obvious sensible default, not as a way to make a big trait look smaller

## Dispatch

- Prefer static dispatch (`impl Trait` / generics) for internal code. Use `dyn Trait` when you need heterogeneous collections or the type must be erased

## Location

- Keep trait definitions in the module that owns the concept, not in the consumer. `FileSystem` lives in `engine/fs.rs`, not in the test module
- When a trait exists for testability, the mock/fake belongs in `#[cfg(test)]` of the consuming module, not next to the trait definition
