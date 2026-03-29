---
title: "Idiomatic Rust"
type: dictum
status: accepted
author: "jack"
date: 2026-03-29
tags: [rust, style, engine, cli, tui]
---

## Error Handling

- `anyhow::Result<T>` for all fallible functions — no custom error types unless a caller needs to match on variants
- Propagate errors with `?`, add context with `.context()` / `.with_context()` when the call site wouldn't be obvious from a stack trace
- No `unwrap()` outside of tests. `expect()` only when the invariant is genuinely guaranteed and the message explains why

## Ownership & Borrowing

- Prefer owned types in structs, borrow in function signatures where the lifetime is obvious
- Prefer `&str` over `&String` in function parameters
- Use `impl Into<T>` / `AsRef<T>` for flexible public APIs, concrete types for internal code
- Avoid `clone()` as a first resort — restructure ownership first, clone when the borrow checker fight isn't worth it

## Naming

- `snake_case` functions/variables, `PascalCase` types, `SCREAMING_SNAKE` constants

## Type Design

- Use tuple structs / newtypes for domain concepts (like `DocType(String)`) rather than bare primitives
- Use `Default` trait and derive it where sensible — prefer `Type::default()` over manual field-by-field construction
- Use `From`/`Into` implementations for type conversions rather than ad-hoc methods
- Derive `Debug` on all public types. Derive `Clone`, `PartialEq` etc. when there's a use for it, not speculatively

## Idioms

- Prefer iterators and combinators over manual loops where readability doesn't suffer
- Prefer `collect()` into concrete types over building collections manually
- Prefer `if let` / `let else` over `match` when only one variant matters
- Prefer exhaustive `match` over `_` wildcards when the enum is local — forces handling new variants
