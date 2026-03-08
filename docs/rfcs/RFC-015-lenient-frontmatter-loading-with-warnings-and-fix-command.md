---
title: "Lenient frontmatter loading with warnings and fix command"
type: rfc
status: draft
author: "jkaloger"
date: 2026-03-08
tags:
  - frontmatter
  - validation
  - dx
related:
  - related to: docs/rfcs/RFC-008-project-health-awareness.md
---

## Problem

Documents with incomplete or malformed frontmatter are silently dropped by `Store::load()`. The current `RawFrontmatter` struct requires six fields (title, type, status, author, date, tags) with no defaults except `related` and `validate-ignore`. When any required field is missing or has the wrong type, `serde_yaml::from_str()` fails and the `if let Ok()` pattern in Store silently discards the document.

Users get no feedback. A document missing `tags: []` simply doesn't appear in the TUI or CLI output. The only way to diagnose is to manually inspect the file and compare against the expected schema.

This matters because:

- External documents (imported from other tools, written by hand) often have partial frontmatter
- Agent-generated documents occasionally miss fields
- The failure mode (silent disappearance) is the hardest kind to debug

## Design Intent

Keep the strict parsing model (a document either fully loads or it doesn't) but replace the silent failure with observable, actionable feedback. Add a `fix` command that programmatically repairs broken frontmatter.

The approach has three parts: error collection in the Store, a CLI fix command, and a TUI warnings panel.

## Interface Sketch

### 1. Store error collection

```
@ref src/engine/store.rs#Store

pub struct Store {
    // ... existing fields ...
    parse_errors: Vec<ParseError>,    // @draft
}

@draft ParseError {
    path: PathBuf,
    error: String,
}
```

`Store::load()` changes from:

```rust
if let Ok(mut meta) = DocMeta::parse(&content) {
    // use meta
}
// else: silently dropped
```

to:

```rust
match DocMeta::parse(&content) {
    Ok(mut meta) => { /* use meta */ }
    Err(e) => {
        parse_errors.push(ParseError {
            path: relative_path,
            error: e.to_string(),
        });
    }
}
```

Store exposes `pub fn parse_errors(&self) -> &[ParseError]` for consumers.

### 2. `lazyspec fix` command

```
lazyspec fix [--dry-run] [--json] [PATH...]
```

Behaviour:
- With no PATH args, fixes all documents with parse errors
- With PATH args, fixes only the specified files
- `--dry-run` prints what would change without writing
- `--json` outputs structured results

For each broken file, the fix command:
1. Reads the file and calls `split_frontmatter()` to separate YAML from body
2. Parses the YAML as `serde_yaml::Value` (loose parse)
3. For each missing required field, inserts a default:

| Field    | Default                               |
|----------|---------------------------------------|
| `title`  | Derived from filename (slug to title) |
| `type`   | Inferred from parent directory name   |
| `status` | `draft`                               |
| `author` | `git config user.name` or `"unknown"` |
| `date`   | Today's date                          |
| `tags`   | `[]`                                  |

4. Rewrites the frontmatter using `@ref src/engine/document.rs#rewrite_frontmatter` pattern
5. Reports what was changed

> [!NOTE]
> If the file has no frontmatter delimiters at all, the fix command wraps the
> existing content with a generated frontmatter block.

### 3. TUI warnings panel

A toggleable panel activated by pressing `w`. Displays:
- Count of documents that failed to load
- Each failure with its file path and error message
- Scrollable if the list is long

The panel overlays or splits from the main view, similar to existing modal patterns in the TUI.

### 4. Enhanced validate output

`lazyspec validate` already reports validation issues for loaded documents. This extends it to also report parse failures:

```json
{
  "errors": [ /* existing validation errors */ ],
  "warnings": [ /* existing warnings */ ],
  "parse_errors": [
    {
      "path": "docs/rfcs/some-broken-doc.md",
      "error": "missing field `status`"
    }
  ]
}
```

## Stories

1. **Store error collection** -- Add `ParseError` tracking to `Store::load()`, expose via accessor, surface in `lazyspec validate` and `lazyspec status` JSON output
2. **Fix command** -- `lazyspec fix` with `--dry-run` and `--json` support, default inference logic, frontmatter rewriting
3. **TUI warnings panel** -- Toggleable panel showing parse failures, keybinding on `w`
