---
title: Nix flake setup
type: iteration
status: draft
author: agent
date: 2026-03-27
tags: []
related:
- implements: STORY-093
---


## Changes

### Task 1: Create flake.nix with package build

ACs addressed: AC-1 (nix build), AC-5 (cargoHash mismatch)

Files:
- Create: `flake.nix`

What to implement:

A Nix flake with three inputs: `nixpkgs` (nixpkgs-unstable), `rust-overlay` (oxalica/rust-overlay), and `flake-utils`. Use `flake-utils.lib.eachDefaultSystem` to support multiple platforms.

Inside the per-system output:

1. Apply the rust-overlay to nixpkgs
2. Select `pkgs.rust-bin.stable.latest.default` with extensions `["clippy" "rustfmt" "rust-src"]`
3. Build a custom `rustPlatform` using `pkgs.makeRustPlatform` with the overlay's cargo and rustc
4. Define `packages.default` using `rustPlatform.buildRustPackage`:
   - `pname = "lazyspec"`, version read from Cargo.toml (hardcode `"0.5.0"` for now)
   - `src = ./.` (use `lib.cleanSource` or a source filter to exclude `docs/`, `target/`, etc.)
   - `cargoHash = "sha256-..."` (get the real hash by running `nix build` and copying from the error)
   - `nativeBuildInputs = [ pkgs.pkg-config ]` (tree-sitter C compilation is handled by stdenv's default cc)
   - No special `buildInputs` needed on Linux; on Darwin, add `pkgs.darwin.apple_sdk.frameworks.Security` and `pkgs.darwin.apple_sdk.frameworks.SystemConfiguration` if needed (test and add conditionally with `lib.optionals pkgs.stdenv.isDarwin`)

The `cargoHash` will fail on first build. Run `nix build`, copy the expected hash from the error output, paste it in. This is the normal `buildRustPackage` workflow and validates AC-5 (hash mismatch detection).

How to verify:
```
nix build
./result/bin/lazyspec --version
```

### Task 2: Add dev shell

ACs addressed: AC-2 (nix develop)

Files:
- Modify: `flake.nix`

What to implement:

Add `devShells.default` to the flake outputs. Use `pkgs.mkShell`:

```nix
devShells.default = pkgs.mkShell {
  inputsFrom = [ self.packages.${system}.default ];
  packages = [
    rustToolchain
    pkgs.rust-analyzer
  ];
};
```

`inputsFrom` pulls in the build inputs from the package (so native deps are available). Adding `rustToolchain` explicitly ensures cargo, clippy, rustfmt are on PATH. `rust-analyzer` is added separately.

How to verify:
```
nix develop --command sh -c "cargo --version && clippy-driver --version && rustfmt --version && rust-analyzer --version"
```

### Task 3: Add flake checks

ACs addressed: AC-4 (nix flake check)

Files:
- Modify: `flake.nix`

What to implement:

Add three entries to the `checks` output:

1. `clippy` -- build the crate with clippy as the compiler, failing on warnings. Use `rustPlatform.buildRustPackage` with the same args as `packages.default` but override to run clippy:
   ```nix
   checks.clippy = self.packages.${system}.default.overrideAttrs (old: {
     buildPhase = ''
       cargo clippy -- -D warnings
     '';
     installPhase = "touch $out";
   });
   ```
   Alternatively, build a separate derivation that just runs clippy in the source with the right environment. The key constraint: must use `-D warnings` and must fail the check if any warnings exist.

2. `test` -- similar pattern, override to run `cargo test`:
   ```nix
   checks.test = self.packages.${system}.default.overrideAttrs (old: {
     buildPhase = ''
       cargo test
     '';
     installPhase = "touch $out";
   });
   ```

3. `fmt` -- run `cargo fmt --check`:
   ```nix
   checks.fmt = self.packages.${system}.default.overrideAttrs (old: {
     buildPhase = ''
       cargo fmt --check
     '';
     installPhase = "touch $out";
   });
   ```

> [!NOTE]
> The exact Nix patterns for checks may need adjustment. The `overrideAttrs` approach works but can be brittle. An alternative is to write standalone derivations that use `mkShell` + `runCommand`. Pick whichever builds cleanly; the AC just requires that `nix flake check` runs all three and fails if any fail.

How to verify:
```
nix flake check
```

### Task 4: Add .envrc for direnv

ACs addressed: AC-3 (direnv integration)

Files:
- Create: `.envrc`

What to implement:

A single-line `.envrc`:
```
use flake
```

Also add `.direnv/` to `.gitignore` if a `.gitignore` exists, or create one with `.direnv/` entry.

How to verify:
```
# With direnv installed and allowed:
direnv allow
# Shell should activate nix develop environment
cargo --version
```

## Test Plan

All tests are manual verification of Nix commands. There is no programmatic test suite for this iteration -- the Nix build system _is_ the test infrastructure.

| AC | Test | Tradeoffs |
|----|------|-----------|
| AC-1: nix build | Run `nix build` and verify `./result/bin/lazyspec --version` outputs the expected version | Predictive (if it builds, it works) but slow (~2-3min cold build) |
| AC-2: nix develop | Run `nix develop --command sh -c "cargo --version && clippy-driver --version && rustfmt --version && rust-analyzer --version"` and verify all four tools are present | Fast, specific |
| AC-3: .envrc | In a direnv-enabled shell, `cd` into the repo and verify the dev shell activates | Requires direnv installed; not automatable in CI |
| AC-4: nix flake check | Run `nix flake check` and verify it passes. Introduce a clippy warning, verify it fails. Remove the warning, introduce a fmt violation, verify it fails. | Predictive, covers all three checks |
| AC-5: cargoHash mismatch | Change a dependency in Cargo.toml, run `nix build` without updating cargoHash, verify it fails with hash mismatch | Deterministic, specific |

## Notes

The `cargoHash` value must be determined empirically by running `nix build` with a dummy hash and copying the correct one from the error. This is standard `buildRustPackage` workflow.

Darwin-specific build inputs (Security, SystemConfiguration frameworks) may be needed. Task 1 includes a conditional for this but it should be verified on macOS.
