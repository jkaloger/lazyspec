---
title: "Code Style"
type: dictum
status: accepted
author: "jkaloger"
date: 2026-03-27
tags: [build, iteration, style]
---


Use early returns over nested conditionals. `bail!()` for precondition failures, `let ... else { return ... }` for optional unwrapping, `continue` in loops for skip conditions. Nesting beyond two levels is a sign the function needs restructuring.

Use function names to convey meaning. A well-named function eliminates the need for a comment. Only add comments when explaining something not obvious from the code itself: a design constraint, a non-obvious invariant, or a workaround for an external limitation.

Error handling uses `anyhow::Result` throughout. Use `anyhow!("message")` for ad-hoc errors and `bail!("message")` for early-exit returns. Define typed error enums (implementing `Display` and `Error`) only when callers need to pattern-match on specific variants. Use `?` for propagation. Never `.unwrap()` in production code.

Do not add docstrings, type annotations, or comments to code you did not change. Do not refactor surrounding code while fixing a bug. Do not add error handling for scenarios that cannot occur. The right amount of complexity is the minimum needed for the current task.
