---
title: Frontmatter Rewrite Utility
type: iteration
status: accepted
author: agent
date: 2026-03-06
tags: []
related:
- implements: docs/stories/STORY-034-frontmatter-utility-extraction.md
---



## Changes

### Task 1: Add `rewrite_frontmatter` to the engine document module

**ACs addressed:** AC-1 (shared utility in engine module)

**Files:**
- Modify: `src/engine/document.rs`

**What to implement:**

Add a public function to `src/engine/document.rs`:

```rust
pub fn rewrite_frontmatter<F>(path: &Path, mutate: F) -> Result<()>
where
    F: FnOnce(&mut serde_yaml::Value) -> Result<()>,
```

The function should:
1. Read the file at `path`
2. Call `split_frontmatter` to get `(yaml, body)`
3. Parse the YAML into `serde_yaml::Value`
4. Call `mutate(&mut value)` to let the caller modify it
5. Serialize back with `serde_yaml::to_string`
6. Reconstruct the file with `format!("---\n{}---\n{}", new_yaml, body)`
7. Write the file back

Note: `serde_yaml::to_string` appends a trailing `\n`, so no extra newline
is needed before `---`. This matches the existing pattern in all call-sites
except `update.rs` (which is out of scope for this iteration since it does
line-level editing rather than serde round-tripping).

**How to verify:**
```
cargo test
```

### Task 2: Replace call-sites in `cli/ignore.rs`

**ACs addressed:** AC-2 (callers delegate to shared utility), AC-3 (no behaviour change)

**Files:**
- Modify: `src/cli/ignore.rs`

**What to implement:**

Replace both `ignore()` and `unignore()` to use `rewrite_frontmatter`.

`ignore` becomes:
```rust
pub fn ignore(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    rewrite_frontmatter(&full_path, |doc| {
        doc["validate-ignore"] = serde_yaml::Value::Bool(true);
        Ok(())
    })
}
```

`unignore` becomes:
```rust
pub fn unignore(root: &Path, doc_path: &str) -> Result<()> {
    let full_path = root.join(doc_path);
    rewrite_frontmatter(&full_path, |doc| {
        if let Some(mapping) = doc.as_mapping_mut() {
            mapping.remove(&serde_yaml::Value::String("validate-ignore".to_string()));
        }
        Ok(())
    })
}
```

Remove the `use crate::engine::document::split_frontmatter` import and add
`use crate::engine::document::rewrite_frontmatter`. Remove `use std::fs`.

**How to verify:**
```
cargo test --test cli_ignore_test
```

### Task 3: Replace call-sites in `cli/link.rs`

**ACs addressed:** AC-2, AC-3

**Files:**
- Modify: `src/cli/link.rs`

**What to implement:**

Replace both `link()` and `unlink()` to use `rewrite_frontmatter`.

`link` becomes:
```rust
pub fn link(root: &Path, from: &str, rel_type: &str, to: &str) -> Result<()> {
    let full_path = root.join(from);
    rewrite_frontmatter(&full_path, |doc| {
        if doc.get("related").is_none() {
            doc["related"] = serde_yaml::Value::Sequence(vec![]);
        }
        let mut entry = serde_yaml::Mapping::new();
        entry.insert(
            serde_yaml::Value::String(rel_type.to_string()),
            serde_yaml::Value::String(to.to_string()),
        );
        doc["related"]
            .as_sequence_mut()
            .unwrap()
            .push(serde_yaml::Value::Mapping(entry));
        Ok(())
    })
}
```

`unlink` follows the same pattern: move the `retain` logic into the closure.

Remove the `split_frontmatter` import, replace with `rewrite_frontmatter`.
Remove `use std::fs`.

**How to verify:**
```
cargo test --test cli_link_test
```

### Task 4: Replace call-site in `tui/app.rs`

**ACs addressed:** AC-2, AC-3

**Files:**
- Modify: `src/tui/app.rs`

**What to implement:**

Replace the `update_tags` function (lines 8-24) to use `rewrite_frontmatter`:

```rust
fn update_tags(root: &Path, relative: &Path, tags: &[String]) -> Result<()> {
    let full_path = root.join(relative);
    rewrite_frontmatter(&full_path, |doc| {
        let tag_values: Vec<serde_yaml::Value> = tags.iter()
            .map(|t| serde_yaml::Value::String(t.clone()))
            .collect();
        doc["tags"] = serde_yaml::Value::Sequence(tag_values);
        Ok(())
    })
}
```

Add `use crate::engine::document::rewrite_frontmatter` import. Remove the
`split_frontmatter` import if no longer used.

**How to verify:**
```
cargo test
```

## Test Plan

| Test | What it verifies | Properties |
|------|-----------------|------------|
| Existing `cli_ignore_test` | `ignore` / `unignore` produce correct frontmatter | Behavioral, Deterministic |
| Existing `cli_link_test` | `link` / `unlink` produce correct relationships | Behavioral, Deterministic |
| New unit test for `rewrite_frontmatter` | Utility reads, mutates, and writes correctly | Isolated, Fast, Specific |
| `cargo test` full suite | No regressions from the refactor | Predictive |

The new unit test for `rewrite_frontmatter` should:
1. Create a temp file with valid frontmatter
2. Call `rewrite_frontmatter` with a mutation closure
3. Read the file back and assert the YAML was modified and body preserved

Existing tests for ignore and link are the primary regression net. If they
pass unchanged, AC-3 (no behaviour change) is satisfied.

## Notes

`cli/update.rs` also reconstructs frontmatter but uses line-level editing
(not serde round-tripping) and a different format string
(`"---\n{}\n---\n{}"`). It is intentionally excluded from this iteration.
Converting it would require changing its approach from line editing to serde,
which is a separate concern.
