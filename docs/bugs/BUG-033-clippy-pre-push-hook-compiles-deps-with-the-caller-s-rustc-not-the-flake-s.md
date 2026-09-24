---
title: "clippy pre-push hook compiles deps with the caller's rustc, not the flake's"
type: bug
status: reported
author: "Jack Kaloger"
date: 2026-09-24
tags: []
related: []
---

## Expected

`git push` runs the clippy pre-push hook with the flake's pinned toolchain, whatever shell it's called from.

## Actual

From a shell without the devshell loaded, the hook fails with no lint involved:

```
error[E0514]: found crate `unicode_segmentation` compiled by an incompatible version of rustc
  = help: please recompile that crate using this compiler (rustc 1.83.0 ...)
error[E0599]: no method named `grapheme_indices` found for reference `&str`
error: could not compile `convert_case` (lib) due to 2 previous errors
```

## Cause

git-hooks.nix wraps `cargo-clippy` and puts only `packageOverrides.cargo` on PATH (`modules/hooks.nix`, `wrapProgram ... --prefix PATH : ${makeBinPath [ packageOverrides.cargo ]}`). There is no `rustc` in that prefix, so cargo builds dependencies with whatever `rustc` the calling shell finds first. `clippy-driver` only wraps workspace crates.

In a non-direnv shell that is the rustup proxy in `~/.cargo/bin`. It resolved to a stray `1.83.0` toolchain. `target/` already held deps built by the devshell's nix rustc 1.94. rustc 1.83 can't load those, hence E0514; E0599 follows from it.

The devshell passes because direnv puts nix's rustc 1.94 first on PATH.

## Fix

1. `flake.nix`: override the clippy hook's `packageOverrides.cargo` with a `symlinkJoin` of `pkgs.cargo` and `pkgs.rustc`. The wrapper then puts the pinned `rustc` on PATH next to `cargo`, so the hook's result stops depending on the caller's PATH.
2. Uninstall the stray rustup `1.83.0` toolchain (local machine, not repo).
