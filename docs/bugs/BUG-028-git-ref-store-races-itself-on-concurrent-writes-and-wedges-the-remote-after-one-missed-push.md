---
title: git-ref store races itself on concurrent writes and wedges the remote after one missed push
type: bug
status: reported
author: Jack Kaloger
date: 2026-09-14
tags: []
related:
- related-to: RFC-035
---

## Expected

Four `lazyspec link` processes running at once against `git-ref` docs in one checkout all land. Each edge ends up in the local ref, the cache file, and the remote. A push that misses once catches up on the next write.

## Actual

Three of four links fail or lose their edge. The surviving state disagrees with itself: the cache file holds edges the ref does not, `.lazyspec/cache.lock` holds SHAs the refs have moved past, and one doc's remote ref falls behind and never recovers. Every later push on that doc is rejected with `stale info` while the local ref keeps advancing. `lazyspec fetch` then resets the local ref to the remote and drops the unpushed edges.

## Repro

Two variants, both deterministic on 0.12.1. `$BIN` is `target/debug/lazyspec`, `origin` is a local bare repo, `feature` and `epic` are `--store git-ref` types.

Same source doc, four targets:

```bash
for i in 1 2 3 4; do $BIN link EPIC-001 related-to FEAT-00$i --json & done; wait
# 3x: Error: conflict updating EPIC-001: git update-ref CAS failed ... is at ee42d22 but expected 6f680b4
rg related-to .lazyspec/cache/epic/EPIC-001.md          # four edges
git show refs/lazyspec/epic/EPIC-001:doc.md | rg related-to  # one edge
```

Four distinct source docs, one target:

```bash
for i in 1 2 3 4; do $BIN link FEAT-00$i implements STORY-001 --json & done; wait
# all four report synced: true, but for three of them:
git rev-parse refs/lazyspec/feature/FEAT-001            # c45d674
jq -r '."feature/FEAT-001"' .lazyspec/cache.lock        # e039107  (stale)
$BIN link FEAT-001 related-to FEAT-002                  # Error: conflict updating FEAT-001 ... is at c45d674 but expected e039107
```

Remote wedge, from either variant once one push has been missed:

```bash
$BIN link FEAT-004 related-to FEAT-002   # ! [rejected] ... (stale info)  -- local ref advances anyway
$BIN link FEAT-004 related-to FEAT-003   # rejected again; remote never catches up
$BIN fetch --type feature                # local ref reset to remote, both edges gone
```

## Root cause

Two defects, both in the `git-ref` write path.

**No cross-process exclusion.** One `link` on a git-ref doc is four read-modify-write steps against shared state, and only one is atomic:

1. `rewrite_frontmatter` edits the cache markdown file in place (`src/engine/ops/link.rs:142`).
2. `recommit_cache` reads the doc's expected SHA from `.lazyspec/cache.lock`, a whole-file JSON map shared by every doc (`src/engine/git_ref_store.rs:220`).
3. `git update-ref <ref> <new> <old>` compare-and-swaps the ref. Atomic and correct.
4. `cache.lock` is loaded, mutated, and rewritten whole (`git_ref_store.rs:246`), then the ref is pushed.

Interleaving on steps 1, 2 and 4 loses updates. Same-doc writers all read the same parent, one CAS wins, the other three bail after their edge is already in the cache file. Distinct-doc writers each save their own copy of `cache.lock`, and the last save wins, leaving the other docs' SHAs stale so their next write fails a CAS against a parent that no longer exists. `update` and `set_provenance` share the same shape.

**Push lease anchored to the local parent.** `push_ref_with_lease` (`src/engine/git_ref.rs:391`) pushes with `--force-with-lease=<ref>:<local parent SHA>`. That asserts the remote is *exactly* at the parent. After one missed push the remote sits at the grandparent, so every later push is rejected while `update_ref` has already advanced the local ref. The doc is wedged until someone pushes by hand. A plain fast-forward push accepts a remote that is behind and still rejects one that diverged; verified both in the repro.

## Fix

1. **Plain fast-forward push** in `push_ref_with_lease`. Drop the lease argument and the `expected_old` parameter. Git's default non-fast-forward rejection is the conflict check wanted, and a behind remote heals on the next write. `push_new_ref` keeps its expect-absent lease for number collision. One semantic change: a doc deleted on the remote by another clone is resurrected by a concurrent update rather than conflicting. Accepted; note it in [[RFC-035]]. README line 686 and RFC-035 describe the lease and need amending.

2. **One process-wide file lock** held for the whole mutation in `GitRefStore` (`recommit_cache`, `update`, `set_provenance`, `delete`), via `std::fs::File::lock` on a file under `.lazyspec/`. Stable std since 1.89; toolchain is 1.94, so no new dependency. Serialises the cache edit, the `cache.lock` rewrite, the CAS and the push. Mark it `ponytail:` with per-doc locks as the upgrade path.

Alternatives not taken: reading the expected SHA from the local ref instead of `cache.lock` fixes only the distinct-doc case; a retry loop on CAS failure fixes only the same-doc case.

Regression test: two writers against a temp bare remote, assert every edge lands in ref, cache and remote, and that a write after a forced-behind remote fast-forwards it.

## Also flagged, no action here

`lazyspec fetch` force-resets local git-ref refs to the remote and drops unpushed commits. Fix 1 makes unpushed commits short-lived so the exposure shrinks, but the clobber deserves its own bug.

## Manual repair for a wedged doc

```bash
git push origin refs/lazyspec/<type>/<ID>:refs/lazyspec/<type>/<ID>
```

Fast-forwards a behind remote, rejects a diverged one. No fetch needed.
