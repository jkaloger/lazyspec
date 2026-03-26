---
title: Git-Based Document Number Reservation
type: rfc
status: accepted
author: jkaloger
date: 2026-03-20
tags:
- numbering
- distributed
- git
related:
- related-to: RFC-027
---




## Problem

Document numbering in lazyspec is local-only. Both numbering strategies scan the filesystem to pick the next ID:

- **Incremental** (`next_number`): finds the highest existing number and adds one. Two people branching from the same state and creating the same document type always collide. This is the original problem that motivated RFC-020 and RFC-027.
- **Sqids** (`next_sqids_id`): uses a timestamp as input, reducing collisions to a same-second window (ITERATION-077). Better, but still probabilistic.

In both cases, the failure is silent until merge time. The `fix` command (RFC-020) can repair collisions after the fact, but it's reactive. For teams that want collisions prevented rather than repaired, we need a coordination mechanism that works across branches.

Git's remote already provides this. Its ref-push semantics give us atomic compare-and-swap for free.

## Intent

Add a `reserved` numbering strategy that uses git custom refs (`refs/reservations/*`) to reserve document numbers on the remote before creating files locally. This eliminates distributed collisions for any number format -- incremental or sqids.

This is an optional layer that wraps the existing format strategies with a coordination mechanism. Teams without a shared remote, or teams comfortable with post-merge repair, don't need it. Teams that want deterministic uniqueness opt in via config.

## Design

### Why Custom Refs

Git refs are just pointer files under `.git/refs/`. The well-known namespaces (`refs/heads/`, `refs/tags/`) are convention, not constraint. You can create arbitrary ref namespaces, and they're invisible to normal git operations unless explicitly queried.

`refs/reservations/*` gives us:

- **Atomic push**: `git push origin refs/reservations/RFC/042` fails if the ref already exists. This is the same CAS guarantee that prevents two people from force-pushing the same branch.
- **No fetch required**: `git ls-remote --refs origin 'refs/reservations/RFC/*'` queries the remote without downloading objects.
- **Invisible**: `git fetch`, `git tag -l`, IDE integrations, and CI pipelines never see these refs unless configured to.
- **No working tree impact**: `git hash-object` and `git update-ref` operate on the object database directly. No checkout, no index changes, no dirty state.

Other git plumbing approaches were considered:

| Approach | Why not |
|----------|---------|
| `git notes` | Append-only to a single object; merge conflicts when two users append simultaneously |
| `git mktree` + `git commit-tree` | Audit trail is nice, but the complexity isn't justified for a reservation ledger |
| `git replace` | Alters history traversal globally; dangerous side effects |
| Tags | Pollutes `git tag -l` output; visible everywhere |

### Reservation Flow

When `lazyspec create` runs with `numbering = "reserved"`:

1. Query the remote for existing reservations:
   ```
   git ls-remote --refs origin "refs/reservations/{PREFIX}/*"
   ```
2. Parse the highest existing number and increment.
3. Create a local ref pointing at an empty blob:
   ```
   git hash-object -t blob --stdin < /dev/null  → {sha}
   git update-ref "refs/reservations/{PREFIX}/{NUM}" {sha}
   ```
4. Attempt atomic push:
   ```
   git push origin "refs/reservations/{PREFIX}/{NUM}"
   ```
5. If push succeeds, use `{NUM}` for the document filename. If it fails (ref already exists on remote), clean up the local ref, increment, and retry from step 3.

The retry loop is bounded. After a configurable number of attempts (default 5), `create` fails with an error rather than spinning.

### Number Format

Reservation is a coordination layer, not a format. It wraps whichever format the team already uses:

- `format = "incremental"`: reserves sequential integers. `refs/reservations/RFC/042` produces `RFC-042-some-title.md`. This is the primary use case -- incremental numbering has no collision protection without reservation.
- `format = "sqids"`: reserves a raw integer, encodes it through sqids for the filename. `refs/reservations/RFC/42` produces `RFC-k3f-some-title.md`. Adds a hard guarantee on top of timestamp entropy's probabilistic one.

The ref path always uses the raw integer regardless of format. This keeps the reservation namespace simple and decodable.

### Configuration

```toml
[[types]]
name = "rfc"
prefix = "RFC"
numbering = "reserved"

[numbering.reserved]
remote = "origin"        # git remote to coordinate against
format = "incremental"   # "incremental" or "sqids"
max_retries = 5          # push retry attempts before failing
```

When `format = "sqids"`, the `[numbering.sqids]` section must also be present (salt, min_length). The reserved strategy reuses the sqids config for encoding.

@ref src/engine/config.rs#NumberingStrategy

The enum gains a third variant:

@draft NumberingStrategy {
    Incremental,
    Sqids { config: SqidsConfig },
    Reserved { config: ReservedConfig },
}

@draft ReservedConfig {
    remote: String,           // default "origin"
    format: ReservedFormat,   // Incremental or Sqids
    max_retries: u8,          // default 5
}

@draft ReservedFormat {
    Incremental,
    Sqids,
}

### Graceful Degradation

If the remote is unreachable (offline, no remote configured, SSH auth failure), `create` should not silently fall back to a different strategy. It fails with a clear error explaining that reserved numbering requires remote access, and suggests using `--numbering incremental` or `--numbering sqids` as a one-off override.

This is deliberate. Silent fallback defeats the purpose: the team chose reserved numbering to guarantee uniqueness, and a silent fallback reintroduces the collision risk they opted out of.

### Cleanup

Reservation refs accumulate over time. They're lightweight (each is a 41-byte file pointing at an empty blob), but projects with hundreds of documents will have hundreds of refs.

`lazyspec` should provide a way to list and prune reservations:

```
lazyspec reservations list              # show all reservations from remote
lazyspec reservations prune             # remove refs for documents that exist
lazyspec reservations prune --dry-run   # preview what would be removed
```

Pruning checks each reservation against the local filesystem: if `RFC-042-some-title.md` exists, the ref `refs/reservations/RFC/042` can be safely deleted. Orphaned reservations (reserved but never created) are flagged but not auto-deleted, since someone may still be working on that document.

### Integration with `resolve_filename`

@ref src/engine/template.rs#resolve_filename

The `numbering` parameter currently accepts `Option<(&NumberingStrategy, &SqidsConfig)>`. With the reserved strategy, it needs access to git operations. Rather than passing git plumbing functions into the template layer, the reservation should happen in the command layer (where git access is natural) and the resolved number passed down to `resolve_filename` as a pre-computed value.

This keeps the template layer pure (no I/O beyond filesystem) and contains the git dependency to the CLI command implementation.

## Stories

1. **Core reservation mechanism** -- Git plumbing integration (`ls-remote`, `hash-object`, `update-ref`, `push`), `NumberingStrategy::Reserved` variant, atomic push with retry loop, integration with `create` command.

2. **Reserved numbering config and validation** -- `[numbering.reserved]` config section, `format` field dispatch to incremental or sqids encoding, validation that remote exists, validation that sqids config is present when `format = "sqids"`.

3. **Reservation management** -- `lazyspec reservations list` and `lazyspec reservations prune` subcommands, `--dry-run` support, orphan detection.
