---
title: CLI and TUI Cleanup
type: iteration
status: accepted
author: agent
date: 2026-03-05
tags: []
related:
- implements: docs/stories/STORY-028-engine-and-cli-quality.md
---





## Problem

Code review identified three structural issues in the CLI entry point and TUI event handling:

1. `main.rs` repeats `current_dir()` 13 times, `Config::load()` 8 times, and `Store::load()` 7 times across command arms
2. The TUI event loop in `tui/mod.rs` is a deeply nested if/else chain (5 levels deep) with duplicated search navigation logic
3. `update.rs`, `link.rs`, and `app.rs` round-trip frontmatter through `serde_yaml`, which silently reformats key ordering, quoting, and drops comments

## Changes

### Task 1: Extract shared setup in `main.rs`

**Files:**
- Modify: `src/main.rs`

**What to implement:**

The 12 command arms in `main.rs` follow three patterns:
- `Init`: needs only `cwd`
- `Create`, `Update`, `Delete`, `Link`, `Unlink`: need `cwd` + `config` (no Store)
- `List`, `Show`, `Search`, `Status`, `Context`, `Validate`, `None` (TUI): need `cwd` + `config` + `store`

Restructure to compute shared state once before the match:

```rust
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    if let Some(Commands::Init) = cli.command {
        lazyspec::cli::init::run(&cwd)?;
        return Ok(());
    }

    let config = Config::load(&cwd)?;

    match cli.command {
        Some(Commands::Init) => unreachable!(),
        Some(Commands::Create { .. }) => { ... }
        // commands that need store:
        Some(Commands::List { .. }) | Some(Commands::Show { .. }) | ... => {
            let store = Store::load(&cwd, &config)?;
            match cli.command { ... }
        }
        ...
    }
}
```

The exact restructuring is flexible. The key constraint: `cwd` computed once, `config` computed once, `store` computed only when needed. The `Init` command must be handled before `Config::load` since it creates the config file.

A simpler approach that avoids the double-match: extract `cwd` and `config` at the top, handle `Init` as an early return, then match the rest. Commands that don't need Store just don't call `Store::load`. Commands that do can call a `load_store` helper.

```rust
fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let cwd = std::env::current_dir()?;

    if matches!(cli.command, Some(Commands::Init)) {
        lazyspec::cli::init::run(&cwd)?;
        return Ok(());
    }

    let config = Config::load(&cwd)?;

    match cli.command {
        Some(Commands::Create { doc_type, title, author, json }) => { ... }
        Some(Commands::Update { path, status, title }) => { ... }
        Some(Commands::Delete { path }) => { ... }
        Some(Commands::Link { from, rel_type, to }) => { ... }
        Some(Commands::Unlink { from, rel_type, to }) => { ... }
        Some(Commands::List { doc_type, status, json }) => {
            let store = Store::load(&cwd, &config)?;
            ...
        }
        // etc.
    }
    Ok(())
}
```

This eliminates 12 `current_dir()` calls and 7 `Config::load()` calls.

**How to verify:**
```
cargo build && cargo test
```

---

### Task 2: Extract TUI key handlers into `App` methods

**Files:**
- Modify: `src/tui/mod.rs` (simplify event loop)
- Modify: `src/tui/app.rs` (add handler methods)

**What to implement:**

The event loop in `tui/mod.rs:72-181` is a deeply nested conditional chain. Each mode (help, create_form, delete_confirm, search, fullscreen, normal) has its own match block. Extract each into a method on `App`:

Add these methods to `App` in `app.rs`:

```rust
pub fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers, root: &Path, config: &Config) {
    if self.show_help {
        self.show_help = false;
        return;
    }
    if self.create_form.active {
        return self.handle_create_form_key(code, root, config);
    }
    if self.delete_confirm.active {
        return self.handle_delete_confirm_key(code, root);
    }
    if self.search_mode {
        return self.handle_search_key(code, modifiers);
    }
    if self.fullscreen_doc {
        return self.handle_fullscreen_key(code);
    }
    self.handle_normal_key(code, modifiers);
}
```

Each sub-handler moves the corresponding match block from `mod.rs`:

`handle_search_key`: consolidate the duplicated navigation. `Up` and `Ctrl+k` both call `self.search_move_up()`. `Down` and `Ctrl+j` both call `self.search_move_down()`. Add two small methods:

```rust
pub fn search_move_up(&mut self) {
    if self.search_selected > 0 {
        self.search_selected -= 1;
    }
}

pub fn search_move_down(&mut self) {
    if !self.search_results.is_empty() && self.search_selected < self.search_results.len() - 1 {
        self.search_selected += 1;
    }
}
```

Then `handle_search_key` becomes:
```rust
fn handle_search_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
    match code {
        KeyCode::Esc => self.exit_search(),
        KeyCode::Enter => self.select_search_result(),
        KeyCode::Backspace => { self.search_query.pop(); self.update_search(); }
        KeyCode::Up => self.search_move_up(),
        KeyCode::Down => self.search_move_down(),
        KeyCode::Char(c) => {
            if modifiers.contains(KeyModifiers::CONTROL) && c == 'k' {
                self.search_move_up();
            } else if modifiers.contains(KeyModifiers::CONTROL) && c == 'j' {
                self.search_move_down();
            } else {
                self.search_query.push(c);
                self.update_search();
            }
        }
        _ => {}
    }
}
```

After extraction, the event loop in `mod.rs` reduces to:
```rust
if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
    let root = app.store.root().to_path_buf();
    app.handle_key(code, modifiers, &root, config);
}
```

**How to verify:**
```
cargo build
```
Manual TUI verification: all keybindings should work identically. The existing TUI tests (tui_navigation_test, tui_create_form_test, tui_delete_dialog_test) test `App` methods directly, not the event loop, so they remain valid.

---

### Task 3: Replace YAML round-trip with targeted string replacement for simple field updates

**Files:**
- Modify: `src/cli/update.rs`

**What to implement:**

Currently `update.rs:run()` does:
1. Parse YAML into `serde_yaml::Value`
2. Modify the value
3. Serialize back with `serde_yaml::to_string`

This silently reformats the frontmatter. For simple key-value updates (status, title), use regex replacement instead.

Replace the current implementation with a function that does line-by-line replacement within the frontmatter section:

```rust
pub fn run(root: &Path, doc_path: &str, updates: &[(&str, &str)]) -> Result<()> {
    let full_path = root.join(doc_path);
    let content = fs::read_to_string(&full_path)?;

    let (yaml, body) = crate::engine::document::split_frontmatter(&content)?;

    let mut lines: Vec<String> = yaml.lines().map(|l| l.to_string()).collect();
    for (key, value) in updates {
        let prefix = format!("{}:", key);
        if let Some(line) = lines.iter_mut().find(|l| l.trim_start().starts_with(&prefix)) {
            *line = format!("{}: {}", key, value);
        }
    }

    let new_yaml = lines.join("\n");
    let new_content = format!("---\n{}\n---\n{}", new_yaml, body);
    fs::write(&full_path, new_content)?;
    Ok(())
}
```

This preserves the original YAML formatting for all fields except the one being updated. It handles the `status` and `title` fields which are simple scalar values.

> [!NOTE]
> This approach only works for simple scalar fields. If `update` ever needs to modify arrays or nested structures (like `related` or `tags`), those operations should continue using `serde_yaml`. But today, `update.rs` only handles `status` and `title` via CLI flags, so string replacement is sufficient.

The same YAML round-trip exists in `link.rs` and `app.rs:update_tags`, but those modify the `related` array and `tags` array respectively, which genuinely need structured YAML manipulation. Leave those as-is.

**How to verify:**
```
cargo test cli_mutate_test
```
Additionally, manually verify that running `cargo run -- update <path> --status accepted` does not reformat the rest of the frontmatter. Create a test doc, note its frontmatter formatting, update status, diff.

## Test Plan

| Test | What it verifies | Properties traded |
|------|-----------------|-------------------|
| `cargo build` | main.rs restructuring compiles | Fast, Predictive |
| All existing `cli_mutate_test` tests | update command still works with string replacement | Fast, Isolated |
| All existing `tui_navigation_test` tests | TUI app methods unchanged | Fast, Isolated |
| All existing `tui_create_form_test` tests | Create form handling unchanged | Fast, Isolated |
| All existing `tui_delete_dialog_test` tests | Delete flow unchanged | Fast, Isolated |
| All existing `tui_submit_form_test` tests | Form submission unchanged | Fast, Isolated |
| Manual: TUI keybindings | All modes respond to keys correctly after event loop refactor | Predictive, but not automated |

No new tests. The refactoring preserves existing behavior. One exception: a new test for the string-replacement update approach would be valuable to verify formatting preservation, but the existing `cli_mutate_test` already covers the functional correctness.

## Notes

- Task 1 and Task 2 are independent and can be done in either order.
- Task 3 is independent of the other two.
- Task 2 makes the key handling testable in the future (unit tests against `App::handle_key` with mock key events), but adding those tests is out of scope.
- The YAML round-trip fix in Task 3 is scoped to `update.rs` only. `link.rs` and `app.rs:update_tags` do structural modifications to arrays, so they legitimately need serde_yaml serialization. A future iteration could explore preserving formatting for those too (e.g., using a YAML-aware editor like `yaml-rust2`), but that's a bigger change.
