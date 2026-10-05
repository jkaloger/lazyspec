pub trait SymbolExtractor {
    fn extract(&self, source: &str, symbol: &str) -> Option<String>;
}

use tree_sitter::{Node, Parser, TreeCursor};
use tree_sitter_rust::LANGUAGE as LANGUAGE_RUST;
use tree_sitter_typescript::LANGUAGE_TYPESCRIPT;

const COMMENT_NODE_KINDS: &[&str] = &["line_comment", "block_comment", "comment"];

/// Parse source text with the given tree-sitter language, strip comment nodes,
/// collect leaf-node text, and collapse whitespace runs into single spaces.
pub fn normalize_ast(source: &str, language: tree_sitter::Language) -> String {
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .expect("failed to set language");
    let tree = parser.parse(source, None).expect("failed to parse source");
    let root = tree.root_node();

    let mut leaves = Vec::new();
    collect_leaves(&root, source, &mut leaves);

    let joined = leaves.join(" ");
    // Collapse runs of whitespace into single spaces and trim
    let mut result = String::with_capacity(joined.len());
    let mut prev_was_space = true; // treat start as space to trim leading
    for ch in joined.chars() {
        if ch.is_whitespace() {
            if !prev_was_space {
                result.push(' ');
                prev_was_space = true;
            }
        } else {
            result.push(ch);
            prev_was_space = false;
        }
    }
    // Trim trailing space
    if result.ends_with(' ') {
        result.pop();
    }
    result
}

fn collect_leaves<'a>(node: &Node<'a>, source: &str, out: &mut Vec<String>) {
    if COMMENT_NODE_KINDS.contains(&node.kind()) {
        return;
    }
    if node.child_count() == 0 {
        let text = &source[node.start_byte()..node.end_byte()];
        out.push(text.to_string());
    } else {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                collect_leaves(&child, source, out);
            }
        }
    }
}

struct Matcher {
    declaration_kinds: &'static [&'static str],
    leading_attributes_and_docs: bool,
    test_blocks: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Pass {
    Declarations,
    TestBlocks,
}

const TEST_CALLEES: &[&str] = &["it", "test", "describe"];
const TEST_MODIFIERS: &[&str] = &["skip", "only"];

fn node_text<'a>(node: Node, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}

fn is_test_callee(callee: Node, source: &str) -> bool {
    match callee.kind() {
        "identifier" => TEST_CALLEES.contains(&node_text(callee, source)),
        "member_expression" => {
            let (Some(object), Some(property)) = (
                callee.child_by_field_name("object"),
                callee.child_by_field_name("property"),
            ) else {
                return false;
            };
            object.kind() == "identifier"
                && TEST_CALLEES.contains(&node_text(object, source))
                && TEST_MODIFIERS.contains(&node_text(property, source))
        }
        _ => false,
    }
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(e @ ('"' | '\'' | '\\' | '`')) => out.push(e),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn literal_title(arg: Node, source: &str) -> Option<String> {
    if arg.kind() == "template_string" {
        let mut cursor = arg.walk();
        let has_substitution = arg
            .children(&mut cursor)
            .any(|c| c.kind() == "template_substitution");
        if has_substitution {
            return None;
        }
    } else if arg.kind() != "string" {
        return None;
    }
    let text = node_text(arg, source);
    text.get(1..text.len().checked_sub(1)?).map(unescape)
}

fn call_title(call: Node, source: &str) -> Option<String> {
    if !is_test_callee(call.child_by_field_name("function")?, source) {
        return None;
    }
    let first_arg = call.child_by_field_name("arguments")?.named_child(0)?;
    literal_title(first_arg, source)
}

fn is_comment(node: Node) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

fn is_outer_doc_comment(node: Node) -> bool {
    is_comment(node) && node.child_by_field_name("outer").is_some()
}

fn is_inner_doc_comment(node: Node) -> bool {
    is_comment(node) && node.child_by_field_name("inner").is_some()
}

// Attributes and outer docs attach to the item across blank lines and plain
// comments; the range starts at the earliest of them. Inner docs belong to the
// enclosing module, so the walk stops there.
fn start_including_leading_decorations(node: Node) -> usize {
    let mut earliest = node;
    let mut current = node;
    while let Some(prev) = current.prev_sibling() {
        if is_inner_doc_comment(prev) {
            break;
        }
        if prev.kind() == "attribute_item" || is_outer_doc_comment(prev) {
            earliest = prev;
        } else if !is_comment(prev) {
            break;
        }
        current = prev;
    }
    earliest.start_byte()
}

fn declaration_name_matches(node: Node, source: &str, symbol: &str) -> bool {
    node.child_by_field_name("name")
        .or_else(|| node.child_by_field_name("type"))
        .is_some_and(|n| node_text(n, source) == symbol)
}

fn matched_range(
    node: Node,
    source: &str,
    symbol: &str,
    matcher: &Matcher,
    pass: Pass,
) -> Option<(usize, usize)> {
    if pass == Pass::Declarations
        && matcher.declaration_kinds.contains(&node.kind())
        && declaration_name_matches(node, source, symbol)
    {
        let start = if matcher.leading_attributes_and_docs {
            start_including_leading_decorations(node)
        } else {
            node.start_byte()
        };
        return Some((start, node.end_byte()));
    }

    if pass == Pass::TestBlocks
        && node.kind() == "call_expression"
        && call_title(node, source).as_deref() == Some(symbol)
    {
        let statement = node
            .parent()
            .filter(|p| p.kind() == "expression_statement")
            .unwrap_or(node);
        return Some((statement.start_byte(), statement.end_byte()));
    }

    None
}

// Declarations win over test blocks, wherever each sits in the tree.
fn find_symbol(root: Node, source: &str, symbol: &str, matcher: &Matcher) -> Option<String> {
    let found = find_symbol_node(
        &mut root.walk(),
        source,
        symbol,
        matcher,
        Pass::Declarations,
    );
    if found.is_some() || !matcher.test_blocks {
        return found;
    }
    find_symbol_node(&mut root.walk(), source, symbol, matcher, Pass::TestBlocks)
}

fn find_symbol_node(
    cursor: &mut TreeCursor,
    source: &str,
    symbol: &str,
    matcher: &Matcher,
    pass: Pass,
) -> Option<String> {
    if let Some((start, end)) = matched_range(cursor.node(), source, symbol, matcher, pass) {
        return Some(source[start..end].to_string());
    }

    if cursor.goto_first_child() {
        loop {
            if let Some(result) = find_symbol_node(cursor, source, symbol, matcher, pass) {
                return Some(result);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }

    None
}

pub struct TypeScriptSymbolExtractor;

impl TypeScriptSymbolExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl SymbolExtractor for TypeScriptSymbolExtractor {
    fn extract(&self, source: &str, symbol: &str) -> Option<String> {
        let mut parser = Parser::new();
        parser.set_language(&LANGUAGE_TYPESCRIPT.into()).ok()?;
        let tree = parser.parse(source, None)?;
        let root = tree.root_node();

        find_symbol(
            root,
            source,
            symbol,
            &Matcher {
                declaration_kinds: &[
                    "type_alias",
                    "type_alias_declaration",
                    "interface_declaration",
                    "class_declaration",
                    "function_declaration",
                    "enum_declaration",
                ],
                leading_attributes_and_docs: false,
                test_blocks: true,
            },
        )
    }
}

impl Default for TypeScriptSymbolExtractor {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RustSymbolExtractor;

impl RustSymbolExtractor {
    pub fn new() -> Self {
        Self
    }
}

impl SymbolExtractor for RustSymbolExtractor {
    fn extract(&self, source: &str, symbol: &str) -> Option<String> {
        let mut parser = Parser::new();
        parser.set_language(&LANGUAGE_RUST.into()).ok()?;
        let tree = parser.parse(source, None)?;
        let root = tree.root_node();

        find_symbol(
            root,
            source,
            symbol,
            &Matcher {
                declaration_kinds: &[
                    "struct_item",
                    "enum_item",
                    "function_item",
                    "trait_item",
                    "impl_item",
                    "type_item",
                    "const_item",
                    "static_item",
                    "macro_definition",
                ],
                leading_attributes_and_docs: true,
                test_blocks: false,
            },
        )
    }
}

impl Default for RustSymbolExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // BUG-038: leading attributes and doc comments
    fn rust_extract(source: &str, symbol: &str) -> String {
        RustSymbolExtractor::new().extract(source, symbol).unwrap()
    }

    #[test]
    fn rust_extraction_includes_leading_attributes() {
        let source = "#[test]\n#[should_panic]\nfn my_test() {}\n";
        assert_eq!(
            rust_extract(source, "my_test"),
            "#[test]\n#[should_panic]\nfn my_test() {}"
        );
    }

    #[test]
    fn rust_extraction_changes_when_attribute_added() {
        let before = rust_extract("#[test]\nfn t() {}", "t");
        let after = rust_extract("#[test]\n#[ignore]\nfn t() {}", "t");
        assert_ne!(before, after);
    }

    #[test]
    fn rust_extraction_includes_line_doc_comments() {
        let source = "/// Adds.\n/// More.\n#[inline]\npub fn add() {}";
        assert_eq!(rust_extract(source, "add"), source);
    }

    #[test]
    fn rust_extraction_stops_at_inner_doc_comments() {
        assert_eq!(
            rust_extract("/// Before.\n//! Inner.\n/// After.\nfn f() {}", "f"),
            "/// After.\nfn f() {}"
        );
        assert_eq!(
            rust_extract("/// Before.\n/*! Inner. */\n/// After.\nfn f() {}", "f"),
            "/// After.\nfn f() {}"
        );
    }

    #[test]
    fn rust_extraction_excludes_inner_doc_comments() {
        assert_eq!(rust_extract("//! Inner.\nfn f() {}", "f"), "fn f() {}");
        assert_eq!(rust_extract("/*! Inner. */\nfn f() {}", "f"), "fn f() {}");
        assert_eq!(
            rust_extract("#![allow(unused)]\nfn f() {}", "f"),
            "fn f() {}"
        );
    }

    #[test]
    fn rust_extraction_includes_block_doc_comments() {
        let source = "/** Docs. */\nstruct S;";
        assert_eq!(rust_extract(source, "S"), source);
    }

    #[test]
    fn rust_extraction_excludes_plain_comments() {
        let source = "// plain\n#[test]\nfn f() {}";
        assert_eq!(rust_extract(source, "f"), "#[test]\nfn f() {}");
        assert_eq!(rust_extract("// plain\nfn f() {}", "f"), "fn f() {}");
    }

    #[test]
    fn rust_extraction_includes_doc_across_plain_comment() {
        let source = "/// doc\n// plain\nfn f() {}";
        assert_eq!(rust_extract(source, "f"), source);
    }

    #[test]
    fn rust_extraction_includes_attribute_across_plain_comment() {
        let source = "#[test]\n// note\nfn t() {}";
        assert_eq!(rust_extract(source, "t"), source);
    }

    #[test]
    fn rust_extraction_includes_attribute_across_blank_line() {
        let source = "#[should_panic]\n\nfn t() {}";
        assert_eq!(rust_extract(source, "t"), source);
    }

    #[test]
    fn rust_extraction_trims_leading_plain_comments() {
        let source = "// plain\n\n/// doc\n#[test]\nfn t() {}";
        assert_eq!(rust_extract(source, "t"), "/// doc\n#[test]\nfn t() {}");
    }

    #[test]
    fn rust_extraction_stops_at_preceding_item() {
        let source = "fn a() {}\n#[test]\nfn b() {}";
        assert_eq!(rust_extract(source, "b"), "#[test]\nfn b() {}");
    }

    #[test]
    fn rust_extraction_includes_attributes_on_impl_methods() {
        let source = "impl X {\n    /// Doc.\n    #[must_use]\n    fn m(&self) {}\n}";
        assert_eq!(
            rust_extract(source, "m"),
            "/// Doc.\n    #[must_use]\n    fn m(&self) {}"
        );
    }

    // STORY-298: Jest/Vitest test blocks
    fn ts_extract(source: &str, symbol: &str) -> Option<String> {
        TypeScriptSymbolExtractor::new().extract(source, symbol)
    }

    #[test]
    fn ts_it_and_test_calls_resolve_to_whole_statement() {
        let it = "it(\"does x\", () => {\n  expect(1).toBe(1);\n});";
        assert_eq!(ts_extract(it, "does x").as_deref(), Some(it));
        let test = "test('does y', async () => { await run(); });";
        assert_eq!(ts_extract(test, "does y").as_deref(), Some(test));
    }

    #[test]
    fn ts_describe_resolves_to_whole_block() {
        let source = "describe(\"suite\", () => {\n  it(\"a\", () => {});\n});";
        assert_eq!(ts_extract(source, "suite").as_deref(), Some(source));
    }

    #[test]
    fn ts_nested_it_inside_describe_is_found() {
        let source = "describe(\"suite\", () => {\n  it(\"inner\", () => {});\n});";
        assert_eq!(
            ts_extract(source, "inner").as_deref(),
            Some("it(\"inner\", () => {});")
        );
    }

    #[test]
    fn ts_skip_and_only_modifiers_resolve_like_plain_form() {
        for callee in [
            "it.skip",
            "it.only",
            "test.skip",
            "test.only",
            "describe.skip",
            "describe.only",
        ] {
            let source = format!("{callee}(\"title\", () => {{}});");
            assert_eq!(
                ts_extract(&source, "title").as_deref(),
                Some(source.as_str()),
                "{callee}"
            );
        }
    }

    #[test]
    fn ts_substitution_free_template_literal_title_matches() {
        let source = "it(`a`, () => {});";
        assert_eq!(ts_extract(source, "a").as_deref(), Some(source));
    }

    #[test]
    fn ts_template_literal_with_substitution_does_not_match() {
        let source = "it(`a${x}`, () => {});";
        assert_eq!(ts_extract(source, "a${x}"), None);
        assert_eq!(ts_extract(source, "a"), None);
    }

    #[test]
    fn ts_declarations_still_resolve_alongside_test_blocks() {
        let source = "function helper() { return 1; }\nclass Widget {}\nit(\"x\", () => {});";
        assert_eq!(
            ts_extract(source, "helper").as_deref(),
            Some("function helper() { return 1; }")
        );
        assert_eq!(
            ts_extract(source, "Widget").as_deref(),
            Some("class Widget {}")
        );
    }

    #[test]
    fn ts_duplicate_titles_return_first_in_document_order() {
        let source = "it(\"dup\", () => { first(); });\nit(\"dup\", () => { second(); });";
        assert_eq!(
            ts_extract(source, "dup").as_deref(),
            Some("it(\"dup\", () => { first(); });")
        );
    }

    #[test]
    fn ts_declaration_wins_over_an_earlier_test_block() {
        let source = "it(\"setup\", () => {});\nfunction setup() {}";
        assert_eq!(
            ts_extract(source, "setup").as_deref(),
            Some("function setup() {}")
        );
    }

    #[test]
    fn ts_escaped_title_matches_its_unescaped_value() {
        let source = "it(\"says \\\"hi\\\"\\n\", () => {});";
        assert_eq!(ts_extract(source, "says \"hi\"\n").as_deref(), Some(source));
    }

    #[test]
    fn ts_non_test_callee_does_not_match() {
        assert_eq!(ts_extract("foo(\"title\", () => {});", "title"), None);
        assert_eq!(
            ts_extract("it.each([1])(\"title\", () => {});", "title"),
            None
        );
        assert_eq!(ts_extract("it.todo(\"title\");", "title"), None);
    }

    #[test]
    fn rust_extractor_ignores_call_like_test_blocks() {
        assert_eq!(
            RustSymbolExtractor::new().extract("fn f() { it(\"x\"); }", "x"),
            None
        );
    }

    // AC-1: TypeScript type alias extraction
    #[test]
    fn test_extract_type_alias_basic() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "type MyType = string | number;";
        let result = extractor.extract(source, "MyType");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("MyType"));
        assert!(extracted.contains("string | number"));
    }

    #[test]
    fn test_extract_type_alias_with_generics() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "type StringMap<T> = Record<string, T>;";
        let result = extractor.extract(source, "StringMap");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("StringMap"));
        assert!(extracted.contains("<T>"));
    }

    #[test]
    fn test_extract_type_alias_object_type() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "type Config = { key: string; value: number; };";
        let result = extractor.extract(source, "Config");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Config"));
        assert!(extracted.contains("key"));
        assert!(extracted.contains("value"));
    }

    // AC-2: TypeScript interface extraction
    #[test]
    fn test_extract_interface_basic() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "interface Person { name: string; age: number; }";
        let result = extractor.extract(source, "Person");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Person"));
        assert!(extracted.contains("name"));
        assert!(extracted.contains("age"));
    }

    #[test]
    fn test_extract_interface_with_generics() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "interface Repository<T> { find(id: string): T; save(item: T): void; }";
        let result = extractor.extract(source, "Repository");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Repository"));
        assert!(extracted.contains("<T>"));
        assert!(extracted.contains("find"));
    }

    #[test]
    fn test_extract_interface_extends() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "interface Employee extends Person { department: string; }";
        let result = extractor.extract(source, "Employee");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Employee"));
        assert!(extracted.contains("extends"));
    }

    // AC-3: Rust struct extraction
    #[test]
    fn test_extract_rust_struct_basic() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub struct Person {
    name: String,
    age: u32,
}"#;
        let result = extractor.extract(source, "Person");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Person"));
        assert!(extracted.contains("name"));
        assert!(extracted.contains("age"));
    }

    #[test]
    fn test_extract_rust_struct_tuple() {
        let extractor = RustSymbolExtractor::new();
        let source = "pub struct Point(i32, i32);";
        let result = extractor.extract(source, "Point");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Point"));
        assert!(extracted.contains("i32"));
    }

    #[test]
    fn test_extract_rust_struct_unit() {
        let extractor = RustSymbolExtractor::new();
        let source = "pub struct Marker;";
        let result = extractor.extract(source, "Marker");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Marker"));
    }

    #[test]
    fn test_extract_rust_struct_with_impl() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub struct Counter {
    count: u64,
}

impl Counter {
    pub fn new() -> Self {
        Counter { count: 0 }
    }
}"#;
        let result = extractor.extract(source, "Counter");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Counter"));
        assert!(extracted.contains("count"));
    }

    // AC-4: Rust enum extraction
    #[test]
    fn test_extract_rust_enum_basic() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub enum Status {
    Pending,
    Active,
    Completed,
}"#;
        let result = extractor.extract(source, "Status");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Status"));
        assert!(extracted.contains("Pending"));
        assert!(extracted.contains("Active"));
    }

    #[test]
    fn test_extract_rust_enum_with_data() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub enum Result {
    Ok(T),
    Err(E),
}"#;
        let result = extractor.extract(source, "Result");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Result"));
        assert!(extracted.contains("Ok"));
        assert!(extracted.contains("Err"));
    }

    #[test]
    fn test_extract_rust_enum_with_fields() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
    ChangeColor(i32, i32, i32),
}"#;
        let result = extractor.extract(source, "Message");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Message"));
        assert!(extracted.contains("Move"));
        assert!(extracted.contains("Write"));
    }

    // AC-5: Non-existent symbol returns None
    #[test]
    fn test_nonexistent_type_script_symbol() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "type MyType = string;";
        let result = extractor.extract(source, "NonExistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_nonexistent_rust_symbol() {
        let extractor = RustSymbolExtractor::new();
        let source = "pub struct MyStruct { field: i32 }";
        let result = extractor.extract(source, "NonExistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_nonexistent_in_empty_source() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "";
        let result = extractor.extract(source, "Anything");
        assert!(result.is_none());
    }

    #[test]
    fn test_nonexistent_rust_in_empty_source() {
        let extractor = RustSymbolExtractor::new();
        let source = "";
        let result = extractor.extract(source, "Anything");
        assert!(result.is_none());
    }

    // AC-6: Trait is extensible - verify trait is public and has correct signature
    #[test]
    fn test_trait_is_public() {
        assert!(SymbolExtractor::extract(&TypeScriptSymbolExtractor::new(), "", "").is_none());
    }

    #[test]
    fn test_trait_has_correct_signature() {
        let extractor = TypeScriptSymbolExtractor::new();
        fn check_trait_signature(_ext: &dyn SymbolExtractor) {}
        check_trait_signature(&extractor);

        let rust_extractor = RustSymbolExtractor::new();
        check_trait_signature(&rust_extractor);
    }

    #[test]
    fn test_trait_implementations_have_extract_method() {
        let ts_extractor = TypeScriptSymbolExtractor::new();
        let result = ts_extractor.extract("type Foo = string;", "Foo");
        assert!(result.is_some());

        let rust_extractor = RustSymbolExtractor::new();
        let result = rust_extractor.extract("pub struct Bar;", "Bar");
        assert!(result.is_some());
    }

    // Regression: double-advance sibling bug skipped nodes after nested modules
    #[test]
    fn test_no_double_advance_skips_sibling_after_module() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = r#"declare module "foo" {
  interface Inner { x: number; }
}
interface Outer { y: string; }"#;
        let result = extractor.extract(source, "Outer");
        assert!(
            result.is_some(),
            "Outer should be found after a module block"
        );
        let extracted = result.unwrap();
        assert!(extracted.contains("Outer"));
        assert!(extracted.contains("y"));
    }

    // Rust function extraction
    #[test]
    fn test_extract_rust_function() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub fn process(input: &str) -> String {
    input.to_uppercase()
}"#;
        let result = extractor.extract(source, "process");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("process"));
        assert!(extracted.contains("input: &str"));
        assert!(extracted.contains("to_uppercase"));
    }

    // Rust trait extraction
    #[test]
    fn test_extract_rust_trait() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub trait Drawable {
    fn draw(&self);
    fn bounds(&self) -> Rect;
}"#;
        let result = extractor.extract(source, "Drawable");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Drawable"));
        assert!(extracted.contains("draw"));
        assert!(extracted.contains("bounds"));
    }

    // Rust impl block extraction (uses "type" field, not "name")
    #[test]
    fn test_extract_rust_impl() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"pub struct Widget { size: u32 }

impl Widget {
    pub fn new() -> Self {
        Widget { size: 0 }
    }
}"#;
        let result = extractor.extract(source, "Widget");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Widget"));
    }

    // Rust type alias extraction
    #[test]
    fn test_extract_rust_type_alias() {
        let extractor = RustSymbolExtractor::new();
        let source = "pub type NodeId = u64;";
        let result = extractor.extract(source, "NodeId");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("NodeId"));
        assert!(extracted.contains("u64"));
    }

    // Rust const extraction
    #[test]
    fn test_extract_rust_const() {
        let extractor = RustSymbolExtractor::new();
        let source = "pub const MAX_SIZE: usize = 1024;";
        let result = extractor.extract(source, "MAX_SIZE");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("MAX_SIZE"));
        assert!(extracted.contains("1024"));
    }

    // Rust static extraction
    #[test]
    fn test_extract_rust_static() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"static GLOBAL_COUNT: AtomicU64 = AtomicU64::new(0);"#;
        let result = extractor.extract(source, "GLOBAL_COUNT");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("GLOBAL_COUNT"));
        assert!(extracted.contains("AtomicU64"));
    }

    // Rust macro_rules! extraction
    #[test]
    fn test_extract_rust_macro() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"macro_rules! my_macro {
    ($x:expr) => { println!("{}", $x) };
}"#;
        let result = extractor.extract(source, "my_macro");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("my_macro"));
        assert!(extracted.contains("println"));
    }

    // impl block found by "type" field when struct not present
    #[test]
    fn test_extract_rust_impl_without_struct() {
        let extractor = RustSymbolExtractor::new();
        let source = r#"impl Display for Foo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "Foo")
    }
}"#;
        let result = extractor.extract(source, "Foo");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("impl Display for Foo"));
        assert!(extracted.contains("fmt"));
    }

    // TS class extraction
    #[test]
    fn test_extract_ts_class_basic() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source =
            "class Animal { name: string; constructor(name: string) { this.name = name; } }";
        let result = extractor.extract(source, "Animal");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Animal"));
        assert!(extracted.contains("constructor"));
    }

    #[test]
    fn test_extract_ts_class_with_extends() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "class Dog extends Animal { bark() { return 'woof'; } }";
        let result = extractor.extract(source, "Dog");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Dog"));
        assert!(extracted.contains("extends"));
        assert!(extracted.contains("bark"));
    }

    // TS function extraction
    #[test]
    fn test_extract_ts_function_basic() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "function greet(name: string): string { return `Hello ${name}`; }";
        let result = extractor.extract(source, "greet");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("greet"));
        assert!(extracted.contains("name: string"));
    }

    #[test]
    fn test_extract_ts_function_async() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source =
            "async function fetchData(url: string): Promise<Response> { return fetch(url); }";
        let result = extractor.extract(source, "fetchData");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("fetchData"));
        assert!(extracted.contains("Promise"));
    }

    // TS enum extraction
    #[test]
    fn test_extract_ts_enum_basic() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = "enum Direction { Up, Down, Left, Right }";
        let result = extractor.extract(source, "Direction");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Direction"));
        assert!(extracted.contains("Up"));
        assert!(extracted.contains("Right"));
    }

    #[test]
    fn test_extract_ts_enum_with_values() {
        let extractor = TypeScriptSymbolExtractor::new();
        let source = r#"enum Color { Red = "RED", Green = "GREEN", Blue = "BLUE" }"#;
        let result = extractor.extract(source, "Color");
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(extracted.contains("Color"));
        assert!(extracted.contains("Red"));
        assert!(extracted.contains("GREEN"));
    }

    #[test]
    fn test_trait_is_object_safe() {
        fn accepts_extractor<E: SymbolExtractor>(extractor: &E) -> Option<String> {
            extractor.extract("test", "test")
        }

        let ts = TypeScriptSymbolExtractor::new();
        let result = accepts_extractor(&ts);
        assert!(result.is_none());

        let rust = RustSymbolExtractor::new();
        let result = accepts_extractor(&rust);
        assert!(result.is_none());
    }

    // --- normalize_ast tests ---

    #[test]
    fn test_normalize_strips_line_comments() {
        let source = r#"fn hello() {
    // this is a comment
    let x = 1;
}"#;
        let result = normalize_ast(source, LANGUAGE_RUST.into());
        assert!(
            !result.contains("this is a comment"),
            "line comment should be stripped"
        );
        assert!(result.contains("let"));
        assert!(result.contains("x"));
    }

    #[test]
    fn test_normalize_strips_block_comments() {
        let source = r#"fn hello() {
    /* block comment here */
    let x = 1;
}"#;
        let result = normalize_ast(source, LANGUAGE_RUST.into());
        assert!(
            !result.contains("block comment"),
            "block comment should be stripped"
        );
        assert!(result.contains("let"));
        assert!(result.contains("x"));
    }

    #[test]
    fn test_normalize_collapses_whitespace() {
        let compact = "fn hello() { let x = 1; }";
        let spacious = "fn    hello()   {\n\n\n    let   x   =   1;\n\n}";
        let lang: tree_sitter::Language = LANGUAGE_RUST.into();
        let a = normalize_ast(compact, lang.clone());
        let b = normalize_ast(spacious, lang);
        assert_eq!(a, b, "different whitespace should produce identical output");
    }

    #[test]
    fn test_normalize_preserves_code_structure() {
        let source = "pub fn process(input: &str) -> String { input.to_uppercase() }";
        let result = normalize_ast(source, LANGUAGE_RUST.into());
        assert!(result.contains("pub"));
        assert!(result.contains("fn"));
        assert!(result.contains("process"));
        assert!(result.contains("input"));
        assert!(result.contains("&"));
        assert!(result.contains("str"));
        assert!(result.contains("->"));
        assert!(result.contains("String"));
        assert!(result.contains("to_uppercase"));
    }

    #[test]
    fn test_normalize_ts_strips_comments() {
        let source = r#"function greet(name: string): string {
    // line comment
    /* block comment */
    return name;
}"#;
        let result = normalize_ast(source, LANGUAGE_TYPESCRIPT.into());
        assert!(
            !result.contains("line comment"),
            "TS line comment should be stripped"
        );
        assert!(
            !result.contains("block comment"),
            "TS block comment should be stripped"
        );
        assert!(result.contains("greet"));
        assert!(result.contains("return"));
        assert!(result.contains("name"));
    }

    #[test]
    fn test_normalize_idempotent() {
        let source = "fn hello() { let x = 1; }";
        let lang: tree_sitter::Language = LANGUAGE_RUST.into();
        let first = normalize_ast(source, lang.clone());
        let second = normalize_ast(&first, lang);
        assert_eq!(
            first, second,
            "normalizing twice should produce same result"
        );
    }
}
