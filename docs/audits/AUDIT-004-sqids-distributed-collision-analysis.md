---
title: Sqids distributed collision analysis
type: audit
status: complete
author: jkaloger
date: 2026-03-18
tags:
- sqids
- distributed
- collision
related:
- related to: RFC-027
---





## Scope

Audit of whether sqids-based document numbering (RFC-027) achieves its stated goal of collision-free ID generation in distributed (git-branched) workflows. Audit type: spec compliance.

RFC-027's trade-off table claims sqids has "near zero" conflict risk compared to incremental numbering's "high" risk. This audit tests that claim against the actual implementation.

## Criteria

1. Two users on separate branches creating the same document type should not produce the same ID
2. The collision-retry mechanism should handle conflicts that span branches (not just local filesystem)
3. The design should be meaningfully better than incremental numbering for distributed use

## Findings

### Finding 1: Count-based input guarantees collisions across branches

**Severity:** high
**Location:** `src/engine/template.rs:95-96` (`next_sqids_id`)
**Description:** The sqids input integer is `count_prefixed_files(dir, prefix) + 1`. Sqids is deterministic: same input + same salt = same output. When two users on separate branches have the same number of existing documents, they produce identical IDs.

Scenario:
- Main has 5 RFCs. Alice branches, Bob branches.
- Alice creates an RFC. `count = 5`, `input = 6`, ID = `k3f`.
- Bob creates an RFC. `count = 5`, `input = 6`, ID = `k3f`.
- Both branches now have `RFC-k3f-<different-slug>.md`.

This is the same failure mode as incremental numbering (`RFC-006` on both branches), just encoded differently. The sqids layer adds no entropy because the input is derived from shared state (document count) that diverges identically on both branches.

**Recommendation:** Replace the count-based input with a source of local entropy. Options detailed in Finding 4.

### Finding 2: Collision-retry loop only checks local filesystem

**Severity:** medium
**Location:** `src/engine/template.rs:98-106` (the `loop` block in `next_sqids_id`)
**Description:** The retry loop calls `file_exists_with_prefix` which scans the local directory. It catches collisions with files already on the current branch, but cannot detect files on other branches. This is expected (git doesn't expose unmerged branch state), but the RFC doesn't acknowledge this limitation.

The retry loop is useful for local edge cases (e.g., manually created files, format conversion residue) but provides no distributed collision protection.

**Recommendation:** The retry loop is fine for what it does. Document its scope explicitly: it handles local collisions only.

### Finding 3: Salt does not help with same-project collisions

**Severity:** info
**Location:** `src/engine/template.rs:87`, RFC-027 design section
**Description:** RFC-027 mentions that "a project-specific salt changes the output alphabet, so `RFC-k3f` in one project maps to a different number than `RFC-k3f` in another." This is true but irrelevant to the distributed collision problem, which is about collisions within the same project (same salt). The salt differentiates between projects, not between branches of the same project.

**Recommendation:** Clarify in RFC-027 that salt prevents cross-project collisions, not cross-branch collisions within a project.

### Finding 4: Possible remediation approaches

**Severity:** info
**Location:** n/a (design analysis)
**Description:** To make sqids genuinely collision-resistant in distributed workflows, the input integer needs local entropy. Some options:

**A. Timestamp-based input.** Use millisecond-precision Unix timestamp as the sqids input. Two users would need to run `create` in the same millisecond to collide. Produces longer IDs (timestamps are large integers) but sqids handles this fine. Trade-off: IDs grow from 3-4 chars to 6-8 chars.

**B. Random input.** Generate a random u64 and encode it. Collision probability follows the birthday paradox but is negligible at document-scale volumes (thousands, not billions). Trade-off: IDs are not reversible to a meaningful integer; loses sqids' decode capability.

**C. Hybrid: timestamp + count.** Encode `[timestamp, count]` as a multi-value sqids ID. Preserves some ordering information while adding entropy. Sqids natively supports multi-value encoding.

**D. Accept the limitation.** Keep count-based input and lean on `lazyspec fix` (RFC-020) to resolve collisions post-merge. This is honest about what sqids provides: shorter, non-sequential IDs with the same collision characteristics as incremental numbering, resolved by the same mechanism.

**Recommendation:** Option D is the simplest and most honest. Update RFC-027's trade-off table to reflect that sqids and incremental have equivalent collision risk, with different trade-offs in readability and information leakage. If genuine collision prevention is desired, Option A (timestamp) is the most practical path.

## Summary

RFC-027 claims sqids numbering has "near zero" distributed conflict risk. The implementation does not deliver on this claim. The sqids input is derived from document count, which is identical across branches that diverge from the same base. Two users creating the same document type from the same starting state will always produce the same ID.

The existing `fix` command (RFC-020) handles post-merge collision repair for both incremental and sqids IDs, so this isn't a data-loss scenario. But the RFC's marketing of sqids as a distributed-safe alternative is inaccurate and should be corrected.

Priority: update the RFC-027 trade-off table to accurately reflect collision behaviour, then decide whether to pursue timestamp-based input (genuine fix) or accept the limitation with honest documentation.
