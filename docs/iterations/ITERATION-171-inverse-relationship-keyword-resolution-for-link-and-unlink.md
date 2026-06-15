---
title: Inverse relationship keyword resolution for link and unlink
type: iteration
status: accepted
author: agent
date: 2026-06-12
tags: []
related:
- implements: STORY-121
---

## Changes

1. **Engine: keyword resolution fn.** ACs: inverse flips direction, symmetric no inverse, unknown rejected.
   File: `src/engine/document.rs`. Near `RelationType` (lines 116-180) add:
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq)]
   pub struct ResolvedRelKeyword {
       pub rel_type: RelationType,
       pub flipped: bool,
   }

   pub fn resolve_rel_keyword(s: &str) -> Result<ResolvedRelKeyword>
   ```
   Resolution: canonical keywords (`implements`, `supersedes`, `blocks`, `related-to`, plus existing `related to` alias via `RelationType::from_str`) → `flipped: false`. Inverse keywords `implemented-by` → `Implements`, `superseded-by` → `Supersedes`, `blocked-by` → `Blocks`, all `flipped: true`. Case-insensitive, match `FromStr` behaviour. `related-to` symmetric → own inverse, no separate keyword. Anything else → `Err(anyhow!("unknown relation type: {}", s))` naming keyword.
   Add `pub const INVERSE_STRS: [&str; 3] = ["implemented-by", "superseded-by", "blocked-by"];` on `RelationType` next to `ALL_STRS`.
   Unit tests in `#[cfg(test)]` mod, same file: each canonical → not flipped; each inverse → canonical + flipped; `related-to` resolves not-flipped + absent from `INVERSE_STRS`; unknown errors, message names keyword.
   Verify: `cargo test resolve_rel_keyword`.

2. **CLI: link/unlink resolve keyword, swap on flip, validate, return outcome.** ACs: forward unchanged, inverse stores canonical on target, unlink mirrors, unknown rejected before write, output names stored relation.
   Files: `src/cli/link.rs`, `src/main.rs`.
   - `src/cli/link.rs`: at top of `link_inner` (line ~48) and `unlink_with_config` (line ~84), call `resolve_rel_keyword(rel_type)?`. Error propagates before any `rewrite_frontmatter` → unknown keyword never written. If `flipped`, swap `from`/`to` before resolve_to_path/resolve_to_id. Write canonical via `resolved.rel_type.to_string()`, never raw input string. NB this also fixes existing hole: raw unvalidated string written today.
   - Change `link`/`link_with_config`/`link_inner` + `unlink`/`unlink_with_config` return type `Result<()>` → `Result<LinkOutcome>`:
     ```rust
     pub struct LinkOutcome {
         pub source: PathBuf,   // doc relation written to (post-flip)
         pub rel_type: RelationType,
         pub target: String,    // id stored in frontmatter
     }
     ```
   - `src/main.rs` lines 190-229: print from outcome, not raw args. Format: `Linked {source} --{rel_type}--> {target}` where rel_type = canonical. Inverse usage → user sees flip: `link A blocked-by B` prints `Linked <B-path> --blocks--> <A-id>`. Same for `Unlinked`.
   - Existing `?`/unwrap callers compile unchanged; fix any `let () =` bindings in tests (`src/cli/link.rs` embedded tests lines ~302-650, `tests/cli_link_test.rs`).
   Integration tests in `tests/cli_link_test.rs` (TempDir + Store pattern, see existing `link_adds_relationship_to_frontmatter` line 15):
   - `link A blocked-by B` → B frontmatter gains `blocks: A`, A file byte-unchanged, string `blocked-by` absent both files.
   - `implemented-by`, `superseded-by` → canonical on target, flipped.
   - seed B `blocks: A`; `unlink A blocked-by B` → entry removed from B.
   - `link A frobs B` → Err, message contains `frobs`, neither file modified. Same unlink.
   - outcome fields: `link A blocked-by B` returns source=B path, rel_type=Blocks, target=A id.
   - forward regression: existing tests still green.
   Verify: `cargo test --test cli_link_test && cargo test -p lazyspec link`.

3. **Completions: offer inverse keywords.** AC: discoverability (completions).
   File: `src/cli/completions.rs` lines 38-45. `complete_rel_type` chains `RelationType::ALL_STRS` + `RelationType::INVERSE_STRS`, same prefix filter. Unit test: empty prefix → 7 candidates; prefix `block` → `blocks`, `blocked-by`.
   Verify: `cargo test complete_rel_type`.

4. **README + help text.** AC: discoverability (README).
   Files: `README.md` lines 27, 126-127; `src/cli.rs` lines 149-171 doc comments.
   - README link/unlink table rows: mention inverse keywords. Add short paragraph after table: inverse keywords (`implemented-by`, `superseded-by`, `blocked-by`) = write-time aliases, flip direction, store canonical on target; `related-to` symmetric, no inverse. Example: `lazyspec link STORY-9 blocked-by RFC-2` writes `blocks: STORY-9` on RFC-2.
   - `src/cli.rs` rel_type arg help: list inverse keywords.
   Verify: README renders, `cargo run -- link --help` shows keywords.

Task order: 1 → 2 → 3 → 4. 3 and 4 independent after 1/2.

## Test Plan

| AC | Test | Kind |
|----|------|------|
| Forward keywords unchanged | existing `tests/cli_link_test.rs` suite green; `link_adds_relationship_to_frontmatter` asserts target untouched | integration (existing) |
| Inverse flips, stores canonical | `link A blocked-by B` → `blocks: A` on B, A unchanged, no `blocked-by` in frontmatter; same shape for `implemented-by`/`superseded-by` | integration |
| Symmetric no inverse | `related-to` resolves not-flipped; `INVERSE_STRS` lacks any related-to inverse; completion list has exactly 7 keywords | unit |
| Unlink mirrors | seed `blocks: A` on B, `unlink A blocked-by B` removes it | integration |
| Unknown rejected | `link`/`unlink A frobs B` → Err naming `frobs`, zero file writes | integration |
| Output names stored relation | `LinkOutcome` fields assert canonical rel + written doc | integration |
| Completions | `complete_rel_type` unit tests (prefix empty, `block`) | unit |
| README | manual check | manual |

Tradeoff: output AC tested via `LinkOutcome` struct, not stdout capture — `println!` lives in `main.rs`, untestable without spawning binary (DICTUM-004: no spawning processes). Behavioral coverage = outcome fields; format string itself unasserted. Accepted gap.

## Notes

- Discovery: today `rel_type` written to frontmatter raw, unvalidated (`src/cli/link.rs:52-66`). `link A frobs B` corrupts frontmatter. Task 2 validation fixes pre-existing hole, satisfies unknown-keyword AC.
- Direction decision: ADR-007 (write-time alias, canonical stored, ADR-003 invariant holds). No new stored relation types.
- TUI link editor untouched — out of scope per STORY-121.
- `link`/`unlink` have no `--json` flag today; adding one out of scope, outcome struct keeps door open.
