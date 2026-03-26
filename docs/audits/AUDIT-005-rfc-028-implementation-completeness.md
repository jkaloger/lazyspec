---
title: RFC-028 Implementation Completeness
type: audit
status: draft
author: jkaloger
date: 2026-03-20
tags: []
related:
- related-to: RFC-030
- related-to: STORY-074
- related-to: STORY-075
- related-to: STORY-076
---










## Scope

Spec compliance audit of RFC-028 (Git-Based Document Number Reservation) against the acceptance criteria defined in its three stories (STORY-068, STORY-069, STORY-070). The audit also checks whether all document creation paths go through the reservation check when `numbering = "reserved"` is configured.

## Criteria

- STORY-068: Core reservation mechanism (5 ACs)
- STORY-069: Reserved numbering config and validation (7 ACs)
- STORY-070: Reservation management subcommands (6 ACs)
- Cross-cutting: all document creation paths must use reservation when configured

## AC Compliance

### STORY-068: Core Reservation Mechanism

| AC | Status | Location |
|----|--------|----------|
| Query refs, pick next, create local ref, atomic push before write | PASS | `src/engine/reservation.rs:177-215`, `src/cli/create.rs:32-56` |
| Push rejected: cleanup local ref, increment, retry up to max | PASS | `src/engine/reservation.rs:187-204` |
| Retries exhausted: error, no document written | PASS | `src/engine/reservation.rs:207-214` |
| Remote unreachable: immediate failure with hint | PARTIAL | `src/engine/reservation.rs:89-94` (see Finding 1) |
| Reserved number passed as-is to `resolve_filename` | PASS | `src/cli/create.rs:60-63`, `src/engine/template.rs:122-124` |

### STORY-069: Reserved Numbering Config and Validation

| AC | Status | Location |
|----|--------|----------|
| `numbering = "reserved"` produces `NumberingStrategy::Reserved` with `ReservedConfig` | PASS | `src/engine/config.rs:38, 59-66, 323-342` |
| Defaults: remote = "origin", max_retries = 5 | PASS | `src/engine/config.rs:68-74` |
| `format = "sqids"` requires `[numbering.sqids]` | PASS | `src/engine/config.rs:331-341` |
| `format = "incremental"` doesn't require sqids | PASS | Same validation block, skipped for incremental |
| `remote = ""` fails validation | PASS | `src/engine/config.rs:328-330` |
| Sqids format encodes raw integer through sqids | PASS | `src/cli/create.rs:43-54` (no integration test, see Finding 4) |
| Incremental format uses zero-padded integer | PASS | `src/cli/create.rs:42` |

### STORY-070: Reservation Management

| AC | Status | Location |
|----|--------|----------|
| `reservations list` displays type, number, ref path | PASS | `src/cli/reservations.rs:27-43` |
| `reservations list --json` structured output | PASS | `src/cli/reservations.rs:34-35` (see Finding 5) |
| `reservations prune` deletes matched refs | PASS | `src/cli/reservations.rs:107-171` |
| `reservations prune` flags orphans without deleting | PASS | `src/cli/reservations.rs:153-158` |
| `reservations prune --dry-run` previews without deleting | PASS | `src/cli/reservations.rs:127-131` |
| `reservations prune --json` structured output | PASS | `src/cli/reservations.rs:45-65, 161-168` |

## Findings

### Finding 1: `--numbering` override flag does not exist

**Severity:** medium
**Location:** `src/engine/reservation.rs:91`, `src/cli/mod.rs`
**Description:** When the remote is unreachable, the error message says `Hint: use --numbering incremental or --numbering sqids as an override`. This flag does not exist on the `create` command. A user following the hint gets a clap error.
**Recommendation:** Either implement the `--numbering` flag on the `create` command, or change the hint to suggest editing `.lazyspec.toml` as a workaround.

### Finding 2: `fix --renumber` bypasses reservation for Reserved types

**Severity:** medium
**Location:** `src/cli/fix.rs`, `collect_renumber_fixes` (lines 184-347)
**Description:** Running `lazyspec fix --renumber incremental` on a doc type configured with `numbering = "reserved"` will rename documents with locally-scanned incremental IDs that have no corresponding `refs/reservations/*` entry. The conflict-fix path (`renumber_doc`) correctly returns `None` for Reserved types, showing awareness of the constraint, but `--renumber` has no equivalent guard.
**Recommendation:** Add a guard in `collect_renumber_fixes` that skips or errors for types with `NumberingStrategy::Reserved`, consistent with `renumber_doc`'s approach.

### Finding 3: `fix` silently skips Reserved-type collisions

**Severity:** low
**Location:** `src/cli/fix.rs:757-769`
**Description:** When two Reserved-type documents have the same ID (e.g., manually created files), `renumber_doc` returns `None` and silently skips them. The collision remains unresolved with no user feedback.
**Recommendation:** Log a warning when skipping Reserved-type conflicts so users know `fix` cannot resolve them and must re-create via `lazyspec create`.

### Finding 4: No integration test for reserved+sqids create path

**Severity:** low
**Location:** `src/cli/create.rs:43-54`
**Description:** The `ReservedFormat::Sqids` branch in `create.rs` has no integration test exercising the full create command with a reserved+sqids config. Unit-level config parsing and sqids encoding are tested separately, but a regression in the dispatch path would go undetected.
**Recommendation:** Add an integration test (with a mock git remote) that runs `create` with `numbering = "reserved"` and `format = "sqids"`, asserting the filename uses sqids encoding.

### Finding 5: JSON field name diverges from spec language

**Severity:** info
**Location:** `src/engine/reservation.rs:7-11`
**Description:** The `Reservation` struct serializes the document type as `prefix` rather than `type`. The story ACs say "each reservation's type, number, and ref." Functionally equivalent but a naming mismatch if external tooling expects a `type` key.
**Recommendation:** Consider whether downstream consumers exist. If not, this is cosmetic and can be left as-is.

## Summary

RFC-028 is substantially implemented. All 18 acceptance criteria across the three stories pass, with one partial (STORY-068 AC-4: the error message references a non-existent CLI flag).

Two medium-severity findings need attention: the missing `--numbering` override flag (Finding 1) and the `fix --renumber` bypass of reservation (Finding 2). Both represent paths where a user could end up with unreserved document numbers despite configuring `reserved` numbering.

The low-severity findings (silent skip of Reserved collisions, missing integration test for sqids format) and the info-level naming divergence are worth addressing but not blocking.
