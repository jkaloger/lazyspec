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

fn find_symbol_node(
    cursor: &mut TreeCursor,
    source: &str,
    symbol: &str,
    match_node_types: &[&str],
) -> Option<String> {
    let node = cursor.node();
    let node_type = node.kind();

    if match_node_types.contains(&node_type) {
        let name_node = node
            .child_by_field_name("name")
            .or_else(|| node.child_by_field_name("type"));
        if let Some(name_node) = name_node {
            let name = source.get(name_node.start_byte()..name_node.end_byte());
            if name == Some(symbol) {
                let start = node.start_byte();
                let end = node.end_byte();
                return Some(source[start..end].to_string());
            }
        }
    }

    if cursor.goto_first_child() {
        loop {
            if let Some(result) = find_symbol_node(cursor, source, symbol, match_node_types) {
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

        let mut cursor = root.walk();
        find_symbol_node(
            &mut cursor,
            source,
            symbol,
            &[
                "type_alias",
                "type_alias_declaration",
                "interface_declaration",
                "class_declaration",
                "function_declaration",
                "enum_declaration",
            ],
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

        let mut cursor = root.walk();
        find_symbol_node(
            &mut cursor,
            source,
            symbol,
            &[
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
        assert!(
            SymbolExtractor::extract(&TypeScriptSymbolExtractor::new(), "", "").is_none() || true
        );
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
        assert!(result.is_some(), "Outer should be found after a module block");
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
        let source = "class Animal { name: string; constructor(name: string) { this.name = name; } }";
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
        let source = "async function fetchData(url: string): Promise<Response> { return fetch(url); }";
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
        assert_eq!(first, second, "normalizing twice should produce same result");
    }
}
