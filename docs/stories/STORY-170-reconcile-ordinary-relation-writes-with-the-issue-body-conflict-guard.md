---
title: Reconcile ordinary-relation writes with the issue-body conflict guard
type: story
status: complete
author: jkaloger
date: 2026-06-29
tags: []
related:
- implements: RFC-050
---

## Context

Relating an issue-backed doc to another issue-backed doc is broken three ways at once, and the dogfooding report says it plainly: relating an issue to an ADR freezes the TUI, spams duplicate `ADR-` refs into the frontmatter, and never reaches GitHub. STORY-167 fixed the native-relation twin of this (milestone/membership edges); ordinary issue-to-issue relations (`relates-to`, `implements`, `blocks`) still go through the broken path. This is that path's defect cluster — four bugs that compound into the reported symptom.

**A — ordinary relation writes never reach GitHub.** Native relations bypass the issue-body conflict guard via `resync_after_native_edge` (store_dispatch.rs:202); ordinary relations route through `push_cache` (store_dispatch.rs:166), which calls `check_lock` (store_dispatch.rs:270) and bails on ANY remote `updated_at` drift. A comment, a label change, or the milestone-fetch background poll all bump `updated_at`. So a milestone/membership link survives out-of-band drift, but an issue-to-issue `relates-to`/`implements`/`blocks` write gets rejected with "modified on GitHub since your last fetch" — a write that touches only `related` in the body, blocked by an unrelated remote bump. The cache mutates locally; the push never lands.

**B — TUI link editor swallows the error.** keys.rs:179 does `let _ = self.confirm_link(...)`, discarding the `Result`. `confirm_link` (app.rs:3072) `?`-propagates the push failure, so `close_link_editor()` (its last line) never runs and `LinkEditor` (forms.rs:282) has no `error` field to surface it. Net effect: the editor stays open, nothing changes on screen, the key "does nothing". The status picker has the same latent gap at keys.rs:153 (`let _ = self.confirm_status_change(...)`).

**C — duplicate relation refs accumulate.** link.rs:80 appends the new relation to `related` unconditionally — no dedup. Each swallowed-error retry (B) re-runs the append, stacking another identical entry. That is the reported "tonne of ADR- references": one link attempt, retried because nothing visibly happened, written N times.

**D — no atomicity.** The cache frontmatter is rewritten (link.rs:71) BEFORE the GitHub push (link.rs:104). When the push fails (A), the cache is already mutated and GitHub is not — the local view and the remote diverge, and the divergence persists.

The deeper problem is the same one STORY-167 named: `push_cache` re-serializes the WHOLE cache body and replaces the remote issue body wholesale. Even if the conflict guard let the write through, it would clobber any prose or relations a collaborator added on the GitHub side since the last fetch. Last-write-wins on the body is acceptable as a floor, but for relations we can do strictly better.

This slice reconciles ordinary-relation link/unlink with the conflict guard by replacing the wholesale body push with a **surgical remote merge**: fetch the remote issue, `issue_body::deserialize` it (store_dispatch.rs / issue_body.rs:83), apply the single relation delta to the remote's `related`, re-serialize KEEPING the remote prose verbatim (issue_body.rs:32), and `issue_edit`. No `check_lock` reject on this path. The merge is insert-if-absent, so it is idempotent (fixes C at the source). Ordering flips to **push-first**: apply the remote merge, and only on its success write the cache `related` = the merged result (fixes D — cache ends equal to remote). The TUI gets a `LinkEditor.error: Option<String>` and `confirm_link` catches into it — stay open and show on error, clear and close on success — mirroring `submit_create_form` (app.rs:2422-2443) and its overlay render (overlays.rs:137); the same fix lands on `StatusPicker`. Finally, `cli fix relations` (fix/relations.rs) gains a dedup pass to clean files already carrying stacked duplicates from before the fix.

The invariant to restore: after a successful ordinary link/unlink, the cache `related` matches the remote, the remote keeps any concurrently-added prose and relations, and the same operation retried is a no-op rather than a duplicate. On failure, nothing is written anywhere and the user sees why.

## Scope

### In Scope

- Route ordinary (non-native) link/unlink GitHub push through a surgical remote merge: fetch + `issue_body::deserialize` + apply single relation delta to `remote.related` + re-serialize preserving remote prose + `issue_edit`. No `check_lock` reject on this path.
- Make the merge insert-if-absent (idempotent) for link, and a single-entry removal for unlink.
- Push-first ordering: apply the remote merge first; write cache `related` = merged result only on push success. No half-applied divergence.
- `LinkEditor.error: Option<String>`; `confirm_link` catches push failure into it (stay open + render on error, clear + close on success), mirroring `submit_create_form`. Render the error line in the link-editor overlay.
- Fold in the symmetric `StatusPicker` error-surfacing gap (keys.rs:153).
- Extend `cli fix relations` with a dedup pass for `related` entries on files already carrying duplicates.
- Fakes at the gh seam reproducing the out-of-band `updated_at` advance and a concurrently-added remote relation, asserting the merge lands and preserves remote state.

### Out of Scope

- The native-relation path (milestone/membership) — already reconciled by STORY-167.
- The milestone relation-vocabulary constraint (which relation types a milestone-backed type may accept) — a SEPARATE sibling story.
- Three-way merge of concurrent edits to the issue PROSE; for body prose, last-write-wins is retained — the surgical merge only protects `related`, it does not reconcile prose conflicts.
- Removing or weakening the conflict guard on genuine issue-body writes (advance, edit) — body-write protection is retained for those paths.

