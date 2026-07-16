---
title: Tag add and remove CLI commands
type: iteration
status: accepted
author: agent
date: 2026-04-13
tags: []
related:
- implements: STORY-206
---



## Changes

### Task 1: CLI command definitions and main.rs dispatch

ACs addressed: all (scaffolding)

**Files:**
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Create: `src/cli/tag.rs`

Add a `Tag` command to the `Commands` enum in `src/cli.rs` (after `Unlink`, ~line 169). Use a nested subcommand enum rather than two top-level commands:

```
Tag {
    #[command(subcommand)]
    action: TagAction,
},
```

```
enum TagAction {
    Add {
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: String,
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        #[arg(long)]
        json: bool,
    },
    Remove {
        #[arg(add = ArgValueCompleter::new(completions::complete_doc_id))]
        id: String,
        #[arg(required = true, num_args = 1..)]
        tags: Vec<String>,
        #[arg(long)]
        json: bool,
    },
}
```

In `src/main.rs`, add a `Commands::Tag { action }` match arm (~line 228, after the Unlink arm). The dispatch pattern follows `Update` (lines 150-181): check lease gate, load store, call handler, reload doc and print JSON if `--json`, otherwise print human confirmation.

Add `pub mod tag;` to `src/cli/mod.rs`.

Create `src/cli/tag.rs` with stub functions `tag_add_with_config()` and `tag_remove_with_config()` that return `Ok(())`. Wire them into main.rs dispatch. Verify the binary compiles and `lazyspec tag add --help` / `lazyspec tag remove --help` produce correct usage text.

### Task 2: Filesystem tag add and remove

ACs addressed: add-tag-to-filesystem, add-multiple-tags, add-tag-idempotent, remove-tag-from-filesystem, remove-nonexistent-tag-noop, json-output

**Files:**
- Modify: `src/cli/tag.rs`

Implement `tag_add_inner<G>()` and `tag_remove_inner<G>()` following the `link_inner` pattern from `src/cli/link.rs:38-71`. The function is generic over `G: GhIssueReader + GhIssueWriter` to allow mock injection in tests.

**tag_add_inner flow:**
1. `resolve_to_path(store, id)` to get the document path
2. `rewrite_frontmatter(&full_path, fs, |doc| { ... })` to mutate the YAML:
   - Get or create `doc["tags"]` as a sequence
   - For each tag in the input list, check if it already exists in the sequence (idempotent); if not, push it
3. Return `Ok(())`

**tag_remove_inner flow:**
1. Same resolution
2. `rewrite_frontmatter(&full_path, fs, |doc| { ... })`:
   - If `doc["tags"]` is a sequence, `retain` only entries whose string value is not in the removal set
3. Return `Ok(())`

No github/git-ref push logic in this task. That's Task 3.

### Task 3: GitHub-issues tag add and remove with label auto-creation

ACs addressed: add-tag-creates-label-if-needed, remove-tag-from-github-issues, tag-command-updates-local-cache

**Files:**
- Modify: `src/cli/tag.rs`

After the `rewrite_frontmatter` call in both `tag_add_inner` and `tag_remove_inner`, add github-issues push logic. Do NOT reuse `push_if_github_backed` from `link.rs` because that calls `push_cache()` which only pushes body changes with empty label arrays. Tags need `labels_add`/`labels_remove` on `issue_edit`.

Instead, write a `push_tags_if_github_backed<G>()` function in `src/cli/tag.rs` that:

1. Checks if `doc_path` starts with `.lazyspec/cache/` (same guard as `link.rs:127`)
2. Extracts type name from cache path
3. Looks up `TypeDef`, confirms `store == GithubIssues`
4. Gets `repo` from github config
5. Builds `GithubIssuesStore`, calls `check_lock(doc_id)` to get issue number and verify no remote conflict
6. For **tag add**: calls `client.label_ensure(repo, tag, "", &deterministic_color(tag))` for each tag, then `client.issue_edit(repo, number, None, None, &tags_to_add, &[])`
7. For **tag remove**: calls `client.issue_edit(repo, number, None, None, &[], &tags_to_remove)`
8. Calls `issue_cache.touch_lock(doc_id)` to update cache timestamp

`label_ensure` (gh.rs:152, impl at 331-345) handles the "create if not exists" requirement. `deterministic_color` (gh.rs:106-111) generates a consistent hex color.

Pass a `mode: TagOp` enum (`Add(Vec<String>)` / `Remove(Vec<String>)`) to keep the push function unified.

### Task 4: Update README

ACs addressed: (project convention)

**Files:**
- Modify: `README.md`

Add `tag add` and `tag remove` to the CLI command reference in the README, following the format of existing entries.

## Test Plan

All tests go in `src/cli/tag.rs` under `#[cfg(test)] mod tests`, following the pattern in `src/cli/link.rs:234-646`.

### Unit: tag_add on filesystem document
Set up a `TempDir` with a filesystem document containing `tags: [existing]`. Call `tag_add_inner` with `tags = ["new"]`. Read the file back, parse frontmatter, assert `tags == [existing, new]`.

### Unit: tag_add idempotent
Document with `tags: [auth]`. Call `tag_add_inner` with `tags = ["auth"]`. Assert tags remain `[auth]` with no duplicate.

### Unit: tag_add multiple tags at once
Document with `tags: []`. Call `tag_add_inner` with `tags = ["a", "b", "c"]`. Assert tags contain all three.

### Unit: tag_remove on filesystem document
Document with `tags: [auth, refactor]`. Call `tag_remove_inner` with `tags = ["auth"]`. Assert tags become `[refactor]`.

### Unit: tag_remove nonexistent tag
Document with `tags: [auth]`. Call `tag_remove_inner` with `tags = ["missing"]`. Assert tags remain `[auth]`, no error.

### Unit: tag_add on github-issues document triggers label_ensure and issue_edit
Use `MockGhClient` (from `gh.rs:469-646`). Set up a github-issues config, create a cache file and issue map entry. Call `tag_add_inner` with `tags = ["security"]`. Assert:
- `MockGhClient.label_ensure` was called with `"security"`
- `MockGhClient.issue_edit` was called with `labels_add = ["security"]`
- Cache file tags field includes `"security"`

### Unit: tag_remove on github-issues document triggers issue_edit with labels_remove
Same setup. Call `tag_remove_inner` with `tags = ["security"]`. Assert:
- `MockGhClient.issue_edit` was called with `labels_remove = ["security"]`
- Cache file tags field no longer includes `"security"`

### Tradeoff note
These tests use `MockGhClient` at the trait seam per project convention (principle 4). Integration tests against a real GitHub repo are out of scope. The mock verifies the correct `gh` CLI args would be constructed; the `GhCli` implementation itself is already tested elsewhere.

## Notes

- `push_if_github_backed` in `link.rs:116-179` is not reusable for tags because `push_cache()` (store_dispatch.rs:117-136) passes empty label arrays to `issue_edit`. A tag-specific push function is needed.
- The `rewrite_frontmatter` approach (document.rs:218-230) works for both filesystem docs and cache files since both use YAML frontmatter.
- Git-ref backend tag support is out of scope per STORY-206. The push function should only handle github-issues.
