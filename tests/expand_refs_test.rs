mod common;

use lazyspec::cli::show;
use lazyspec::engine::refs::RefExpander;
use std::process::Command;

fn setup() -> common::TestFixture {
    let fixture = common::TestFixture::new();

    let _output = Command::new("git")
        .args(["init"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    let _output = Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    let _output = Command::new("git")
        .args(["config", "user.name", "Test"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    fixture
}

fn commit_file(fixture: &common::TestFixture, path: &str, content: &str) -> String {
    let full_path = fixture.root().join(path);
    std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
    std::fs::write(&full_path, content).unwrap();

    let _ = Command::new("git")
        .args(["add", "-A"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    let output = Command::new("git")
        .args(["commit", "-m", "add file"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    if !output.status.success() {
        eprintln!(
            "git commit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(fixture.root())
        .output()
        .unwrap();

    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

#[test]
fn test_cli_show_expands_ref_to_code_block() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n  name: string;\n}",
    );

    commit_file(&fixture, "test.txt", "test content");

    fixture.write_doc(
        "docs/rfcs/RFC-001-test.md",
        r#"---
title: "Test Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref test.txt
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-001",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("```"),
        "Output should contain fenced code block"
    );
    assert!(
        output.contains("test content") || output.contains("> [unresolved:"),
        "Output should contain either test content or unresolved blockquote"
    );
}

#[test]
fn test_ref_with_git_sha() {
    let fixture = setup();

    let sha = commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    let short_sha = &sha[..7];

    fixture.write_doc(
        "docs/rfcs/RFC-002-test.md",
        &format!(
            r#"---
title: "Test SHA Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref src/user.ts@{}
"#,
            short_sha
        ),
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-002",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("```"),
        "Output should contain fenced code block"
    );
}

#[test]
fn test_ref_nonexistent_file_warning() {
    let fixture = setup();

    fixture.write_doc(
        "docs/rfcs/RFC-003-test.md",
        r#"---
title: "Test Nonexistent File"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref nonexistent.ts
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-003",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("> [unresolved:"),
        "Output should contain unresolved blockquote for nonexistent file"
    );
}

#[test]
fn test_ref_nonexistent_symbol_warning() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-004-test.md",
        r#"---
title: "Test Nonexistent Symbol"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref src/user.ts#NonExistentSymbol
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-004",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("> [unresolved: src/user.ts#NonExistentSymbol]"),
        "Output should contain unresolved marker for nonexistent symbol, got: {}",
        output
    );
}

#[test]
fn test_ref_invalid_sha_warning() {
    let fixture = setup();

    fixture.write_doc(
        "docs/rfcs/RFC-005-test.md",
        r#"---
title: "Test Invalid SHA"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref Cargo.toml@invalid_sha_12345
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-005",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("> [unresolved:"),
        "Output should contain unresolved blockquote for invalid SHA"
    );
}

#[test]
fn test_ref_typescript_language_tag() {
    let fixture = setup();

    let _sha = commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-006-test.md",
        r#"---
title: "Test TS Language"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

@ref src/user.ts
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-006",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    if output.contains("```") {
        assert!(
            output.contains("```ts") || output.contains("```typescript"),
            "Output should use ts or typescript language tag for .ts files"
        );
    }
}

#[test]
fn test_ref_rust_language_tag() {
    let fixture = setup();

    commit_file(&fixture, "src/lib.rs", "pub struct User {\n  id: u32,\n}");

    fixture.write_doc(
        "docs/rfcs/RFC-007-test.md",
        r#"---
title: "Test Rust Language"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

@ref src/lib.rs
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-007",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    if output.contains("```") {
        assert!(
            output.contains("```rust"),
            "Output should use rust language tag for .rs files"
        );
    }
}

#[test]
fn test_mixed_resolved_and_unresolved_refs() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n  name: string;\n}",
    );

    commit_file(&fixture, "test.txt", "test content");

    fixture.write_doc(
        "docs/rfcs/RFC-001-test.md",
        r#"---
title: "Test Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref test.txt
"#,
    );

    let store = fixture.store();

    let result = show::run_json(
        &store,
        "RFC-001",
        true,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("```"),
        "Output should contain fenced code block"
    );
    assert!(
        output.contains("test content") || output.contains("> [unresolved:"),
        "Output should contain either test content or unresolved blockquote"
    );
}

#[test]
fn test_show_without_expand_flag_shows_raw_refs() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-010-test.md",
        r#"---
title: "Test Raw Ref"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

See the code:

@ref src/user.ts
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-010",
        false,
        25,
        &lazyspec::engine::fs::RealFileSystem,
    );

    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        output.contains("@ref src/user.ts"),
        "Output should contain raw @ref directive when expand=false, got: {}",
        output
    );
    assert!(
        !output.contains("```ts") && !output.contains("```typescript"),
        "Output should NOT contain expanded code block when expand=false"
    );
}

#[test]
fn test_search_does_not_expand_refs() {
    let fixture = setup();

    commit_file(
        &fixture,
        "src/user.ts",
        "export interface User {\n  id: number;\n}",
    );

    fixture.write_doc(
        "docs/rfcs/RFC-011-test.md",
        r#"---
title: "Test Search Raw"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

Some context here.

@ref src/user.ts
"#,
    );

    let store = fixture.store();
    let results = store.search("@ref", &lazyspec::engine::fs::RealFileSystem);

    assert!(
        !results.is_empty(),
        "Search for '@ref' should find the document"
    );
    let snippet = &results[0].snippet;
    assert!(
        snippet.contains("@ref"),
        "Search snippet should contain raw @ref text, got: {}",
        snippet
    );
}

// --- Line number refs ---

#[test]
fn test_line_number_ref_extracts_from_line() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 9999);
    let content = "@ref Cargo.toml#1";
    let result = expander.expand(content).unwrap();
    assert!(
        result.contains("```"),
        "Should contain a code fence, got: {}",
        result
    );
    assert!(
        result.contains("[package]"),
        "Line 1 of Cargo.toml should contain [package], got: {}",
        result
    );
}

#[test]
fn test_line_number_ref_out_of_bounds() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 9999);
    let content = "@ref Cargo.toml#99999";
    let result = expander.expand(content).unwrap();
    assert!(
        result.contains("> [unresolved: Cargo.toml#99999]"),
        "Out-of-bounds line ref should produce unresolved warning, got: {}",
        result
    );
}

#[test]
fn test_line_number_vs_symbol_disambiguation() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 9999);
    let content = "@ref src/engine/refs.rs#RefExpander";
    let result = expander.expand(content).unwrap();
    assert!(
        result.contains("```rust"),
        "Symbol ref should produce a rust code block, got: {}",
        result
    );
    assert!(
        result.contains("RefExpander"),
        "Should contain the RefExpander symbol, got: {}",
        result
    );
}

// --- Captions ---

#[test]
fn test_expanded_ref_has_caption() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 9999);
    let content = "@ref Cargo.toml";
    let result = expander.expand(content).unwrap();
    assert!(
        result.contains("**Cargo.toml**"),
        "Caption should contain bold path, got: {}",
        result
    );
    assert!(
        result.contains("@ `"),
        "Caption should contain backtick-wrapped SHA, got: {}",
        result
    );
}

#[test]
fn test_caption_includes_symbol_name() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 9999);
    let content = "@ref src/engine/store.rs#Store";
    let result = expander.expand(content).unwrap();
    assert!(
        result.contains("(Store)"),
        "Symbol ref caption should contain (Store), got: {}",
        result
    );
}

#[test]
fn test_caption_includes_line_number() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 9999);
    let content = "@ref Cargo.toml#1";
    let result = expander.expand(content).unwrap();
    assert!(
        result.contains("(L1)"),
        "Line number ref caption should contain (L1), got: {}",
        result
    );
}

#[test]
fn test_unresolved_ref_no_caption() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 9999);
    let content = "@ref nonexistent/file.rs";
    let result = expander.expand(content).unwrap();
    assert!(
        !result.contains("**"),
        "Unresolved ref should have no bold caption, got: {}",
        result
    );
}

// --- Truncation ---

#[test]
fn test_max_lines_truncates_long_content() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 5);
    // Cargo.toml is well over 5 lines
    let content = "@ref Cargo.toml";
    let result = expander.expand(content).unwrap();

    // Extract content between code fences
    let fence_start = result.find("```toml\n").expect("should have toml fence");
    let code_start = fence_start + "```toml\n".len();
    let fence_end = result[code_start..]
        .find("\n```")
        .expect("should have closing fence");
    let code_body = &result[code_start..code_start + fence_end];
    let code_lines: Vec<&str> = code_body.lines().collect();

    // 5 content lines + 1 truncation comment = 6
    assert_eq!(
        code_lines.len(),
        6,
        "Should have 5 content lines + 1 truncation comment, got {} lines: {:?}",
        code_lines.len(),
        code_lines
    );
    assert!(
        code_lines.last().unwrap().contains("more lines"),
        "Last line should be truncation comment, got: {}",
        code_lines.last().unwrap()
    );
}

#[test]
fn test_max_lines_no_truncation_when_short() {
    let fixture = setup();
    commit_file(&fixture, "tiny.txt", "line1\nline2\nline3");

    fixture.write_doc(
        "docs/rfcs/RFC-020-test.md",
        r#"---
title: "Test No Truncation"
type: rfc
status: draft
author: test
date: 2026-03-11
tags: []
---

@ref tiny.txt
"#,
    );

    let store = fixture.store();
    let result = show::run_json(
        &store,
        "RFC-020",
        true,
        9999,
        &lazyspec::engine::fs::RealFileSystem,
    );
    assert!(result.is_ok());
    let output = result.unwrap();
    assert!(
        !output.contains("more lines"),
        "Short content should not have truncation comment, got: {}",
        output
    );
}

#[test]
fn test_truncation_comment_style_rust() {
    let root = std::env::current_dir().unwrap();
    let expander = RefExpander::with_max_lines(root, 5);
    let content = "@ref src/engine/refs.rs";
    let result = expander.expand(content).unwrap();
    assert!(
        result.contains("// ... ("),
        "Rust file truncation should use // comment style, got: {}",
        result
    );
    assert!(
        result.contains("more lines)"),
        "Truncation comment should mention remaining lines, got: {}",
        result
    );
}
