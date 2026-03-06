---
title: Test Infrastructure
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-028-engine-and-cli-quality.md
---





## Problem

17 test files repeat the same setup patterns: create tempdir, create doc directories, write YAML frontmatter fixtures, load Config + Store. There are no shared test helpers. Setup functions like `setup_with_chain`, `setup_app`, `write_doc`, `setup_dirs` exist as file-local copies with slight variations.

## Changes

### Task 1: Create `tests/common/mod.rs` with `TestFixture`

**ACs addressed:** AC-9 (shared helpers in `tests/common/mod.rs` are available)

**Files:**
- Create: `tests/common/mod.rs`

**What to implement:**

A `TestFixture` struct that encapsulates the three most duplicated test patterns: tempdir + directory creation, writing doc fixtures, and loading Config/Store.

```rust
use lazyspec::engine::config::Config;
use lazyspec::engine::store::Store;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct TestFixture {
    pub dir: TempDir,
}

impl TestFixture {
    pub fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs/rfcs")).unwrap();
        std::fs::create_dir_all(root.join("docs/adrs")).unwrap();
        std::fs::create_dir_all(root.join("docs/stories")).unwrap();
        std::fs::create_dir_all(root.join("docs/iterations")).unwrap();
        Self { dir }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    pub fn config(&self) -> Config {
        Config::default()
    }

    pub fn store(&self) -> Store {
        Store::load(self.root(), &self.config()).unwrap()
    }

    pub fn write_doc(&self, rel_path: &str, content: &str) -> PathBuf {
        let path = self.root().join(rel_path);
        std::fs::write(&path, content).unwrap();
        path
    }

    pub fn write_rfc(&self, filename: &str, title: &str, status: &str) -> PathBuf {
        let content = format!(
            "---\ntitle: \"{}\"\ntype: rfc\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n---\n",
            title, status
        );
        self.write_doc(&format!("docs/rfcs/{}", filename), &content)
    }

    pub fn write_story(&self, filename: &str, title: &str, status: &str, implements: Option<&str>) -> PathBuf {
        let related = match implements {
            Some(path) => format!("related:\n- implements: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: story\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/stories/{}", filename), &content)
    }

    pub fn write_iteration(&self, filename: &str, title: &str, status: &str, implements: Option<&str>) -> PathBuf {
        let related = match implements {
            Some(path) => format!("related:\n- implements: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: iteration\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/iterations/{}", filename), &content)
    }

    pub fn write_adr(&self, filename: &str, title: &str, status: &str, related_to: Option<&str>) -> PathBuf {
        let related = match related_to {
            Some(path) => format!("related:\n- related to: {}", path),
            None => String::new(),
        };
        let content = format!(
            "---\ntitle: \"{}\"\ntype: adr\nstatus: {}\nauthor: \"test\"\ndate: 2026-01-01\ntags: []\n{}\n---\n",
            title, status, related
        );
        self.write_doc(&format!("docs/adrs/{}", filename), &content)
    }
}
```

The exact API may need minor adjustments during migration (e.g. adding `tags` parameter to `write_rfc` if tests need it). Discover the real needs during Tasks 2-4 and extend minimally.

**How to verify:**
```
cargo test
```
No tests use it yet, but it should compile as part of the test crate.

---

### Task 2: Migrate CLI test files (batch 1)

**ACs addressed:** AC-9

**Files:**
- Modify: `tests/cli_validate_test.rs`
- Modify: `tests/cli_query_test.rs`
- Modify: `tests/cli_create_test.rs`
- Modify: `tests/cli_status_test.rs`
- Modify: `tests/cli_json_test.rs`

**What to implement:**

Each of these files has inline or file-local setup that creates tempdirs, doc directories, writes fixtures, and loads Config/Store. Replace with `TestFixture`:

- Remove file-local `setup()` functions and inline tempdir/directory creation
- Add `mod common;` at top of each file
- Use `common::TestFixture::new()` for setup
- Use `fixture.write_rfc()`, `fixture.store()`, etc. instead of inline equivalents
- Keep test assertions and logic unchanged

Example migration pattern:
```rust
// Before:
fn setup() -> (TempDir, Store) {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("docs/rfcs")).unwrap();
    // ... more dirs ...
    fs::write(root.join("docs/rfcs/RFC-001.md"), "---\n...").unwrap();
    let config = Config::default();
    let store = Store::load(root, &config).unwrap();
    (dir, store)
}

// After:
let fixture = common::TestFixture::new();
fixture.write_rfc("RFC-001.md", "Title", "draft");
let store = fixture.store();
```

If any test needs functionality not yet on `TestFixture` (e.g. tags, custom frontmatter), extend `TestFixture` minimally or use `fixture.write_doc()` with raw content.

**How to verify:**
```
cargo test cli_validate_test cli_query_test cli_create_test cli_status_test cli_json_test
```
All tests must pass with identical behavior.

---

### Task 3: Migrate CLI test files (batch 2)

**ACs addressed:** AC-9

**Files:**
- Modify: `tests/cli_expanded_validate_test.rs`
- Modify: `tests/cli_context_test.rs`
- Modify: `tests/cli_mutate_test.rs`
- Modify: `tests/cli_link_test.rs`

**What to implement:**

These files have more complex setups (document chains, multi-doc fixtures, specialized helpers like `setup_with_chain`, `setup_two_docs`, `write_doc`). Replace with `TestFixture`:

- `cli_expanded_validate_test.rs`: Has `setup_with_chain(rfc_status, story_status, iter_status)` and `setup_with_two_stories(...)`. Replace with `TestFixture::new()` + `write_rfc` + `write_story` + `write_iteration` calls. The chain-building logic moves from a file-local helper to inline use of fixture methods.
- `cli_context_test.rs`: Has `setup()` that creates an RFC->Story->Iteration chain. Same pattern.
- `cli_mutate_test.rs`: Has `write_doc(dir)`. Replace with `fixture.write_rfc(...)`.
- `cli_link_test.rs`: Has `setup_two_docs(dir)`. Replace with `fixture.write_rfc(...)` x2.

**How to verify:**
```
cargo test cli_expanded_validate_test cli_context_test cli_mutate_test cli_link_test
```

---

### Task 4: Migrate engine and TUI test files

**ACs addressed:** AC-9

**Files:**
- Modify: `tests/store_test.rs`
- Modify: `tests/tui_create_form_test.rs`
- Modify: `tests/tui_submit_form_test.rs`
- Modify: `tests/tui_delete_dialog_test.rs`
- Modify: `tests/tui_navigation_test.rs`

**What to implement:**

- `store_test.rs`: Has `setup_test_dir()`. Replace with `TestFixture::new()` + doc writes.
- `tui_create_form_test.rs`: Has `setup_app()` returning `(TempDir, App)`. Replace tempdir/dir setup with `TestFixture`, keep the `App::new(store)` construction.
- `tui_submit_form_test.rs`: Has `setup_app_with_rfc()`. Replace with fixture + write_rfc + App construction.
- `tui_delete_dialog_test.rs`: Has `setup_dirs`, `write_rfc`, `write_story_implementing`, `setup_app_with_rfc`. Replace all with `TestFixture` methods.
- `tui_navigation_test.rs`: Has `setup_app()` and `setup_app_with_docs()`. Replace with fixture.

Skip `document_test.rs` and `config_test.rs` -- they don't use Store fixtures.

**How to verify:**
```
cargo test store_test tui_create_form_test tui_submit_form_test tui_delete_dialog_test tui_navigation_test
```

## Test Plan

No new tests. The existing 102 tests are the verification. Every test must pass unchanged after migration.

| Test suite | What it verifies | Properties |
|------------|-----------------|------------|
| All 102 existing tests | Behavior preserved after fixture migration | Fast, Isolated, Deterministic |

## Notes

- `document_test.rs` and `config_test.rs` don't use Store fixtures, so they're excluded from migration.
- If `TestFixture` needs to grow during migration (e.g. adding a `tags` parameter), extend it in the task that discovers the need.
- `cli_init_test.rs` creates a tempdir but doesn't use doc directories or Store -- may not benefit from `TestFixture`. Migrate only if it simplifies the test.
