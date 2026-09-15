//! Verify the configured native grammar and actual resolved colors together.
use super::language_for_path;
use gpui::HighlightStyle;
use gpui_component::highlighter::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};
use ropey::Rope;

fn assert_token_style(language: &str, source: &str, token: &str, expected: &str) {
    let mut highlighter = SyntaxHighlighter::new(language);
    assert!(highlighter.update(None, &Rope::from(source), None));
    let tree = highlighter.tree().expect("configured grammar must parse");
    assert!(
        !tree.root_node().has_error(),
        "{language}: {}",
        tree.root_node().to_sexp()
    );
    let theme = HighlightTheme::default_dark();
    let styles = highlighter.styles(&(0..source.len()), theme.as_ref());
    let offset = source.find(token).expect("fixture token");
    let actual = styles
        .iter()
        .find(|(range, _)| range.contains(&offset))
        .map(|(_, style)| *style)
        .unwrap_or_default();
    let expected_style = theme.style(expected).expect("theme category");
    assert_ne!(expected_style, HighlightStyle::default());
    assert_eq!(
        actual.color, expected_style.color,
        "{language}: token {token:?} should be {expected}"
    );
}

#[test]
fn language_support_react_includes_javascript_types_and_markup() {
    let source = "// React\nexport function Widget({ title }: { title: string }) {\n  const count: number = 42;\n  return <main className=\"card\"><h1>{title}</h1><Widget.Icon /></main>;\n}\n";
    for (token, category) in [
        ("// React", "comment"),
        ("export", "keyword"),
        ("function", "keyword"),
        ("Widget(", "type"),
        ("const", "keyword"),
        ("number", "type"),
        ("42", "number"),
        ("return", "keyword"),
        ("main", "tag"),
        ("className", "attribute"),
        ("\"card\"", "string"),
    ] {
        assert_token_style("tsx", source, token, category);
    }
    let jsx =
        "export function Widget({ title }) { return <main className=\"card\">{title}</main>; }";
    for language in ["jsx", "javascriptreact", "js", "mjs", "cjs"] {
        assert_token_style(language, jsx, "return", "keyword");
        assert_token_style(language, jsx, "Widget(", "type");
        assert_token_style(language, jsx, "main", "tag");
        assert_token_style(language, jsx, "className", "attribute");
    }
    assert_token_style(
        "tsx",
        "function render() { return 1; }",
        "render",
        "function",
    );
}

#[test]
fn language_support_common_extensions_and_named_files_resolve() {
    let registry = LanguageRegistry::singleton();
    for (path, expected) in [
        ("Component.TSX", "tsx"),
        ("Component.jsx", "javascript"),
        ("index.mts", "typescript"),
        ("index.cts", "typescript"),
        ("index.mjs", "javascript"),
        ("index.cjs", "javascript"),
        ("include/types.hpp", "cpp"),
        ("lib.cc", "cpp"),
        ("main.cxx", "cpp"),
        ("module.pyi", "python"),
        ("test.exs", "elixir"),
        ("build/CMakeLists.txt", "cmake"),
        ("Gemfile", "ruby"),
        ("Cargo.lock", "toml"),
        (".bashrc", "bash"),
        (r"src\Component.JSX", "javascript"),
    ] {
        let config = registry
            .language(&language_for_path(path))
            .unwrap_or_else(|| panic!("missing grammar for {path}"));
        assert_eq!(config.name.as_ref(), expected, "{path}");
        assert!(config.has_grammar());
        assert!(
            !config.highlights.is_empty(),
            "empty highlighting for {path}"
        );
    }
}

#[test]
fn language_support_bundled_queries_compile() {
    let registry = LanguageRegistry::singleton();
    for language in registry.languages() {
        let config = registry.language(&language).unwrap();
        if config.has_grammar() {
            assert!(
                !config.highlights.is_empty(),
                "empty highlighting for {language}"
            );
        }
        // Constructor validates the complete query, including injections.
        let mut highlighter = SyntaxHighlighter::new(&language);
        assert!(
            highlighter.update(None, &Rope::from(""), None),
            "{language}"
        );
    }
}

#[test]
fn language_support_typescript_templates_keep_embedded_colors() {
    let source = "const styles = css`main { color: red; }`;";
    assert_token_style("tsx", source, "const", "keyword");
    assert_token_style("tsx", source, "color", "property");
}

#[test]
fn language_support_other_grammars_and_nested_theme_categories() {
    assert_token_style("swift", "func hello() { return }", "func", "keyword");
    assert_token_style(
        "proto",
        "syntax = \"proto3\"; message Reply { string text = 1; }",
        "message",
        "keyword",
    );
    assert_token_style("rust", "fn main() { let count = 42; }", "fn", "keyword");
    assert_token_style(
        "python",
        "def hello():\n    return 42\n",
        "return",
        "keyword",
    );
    assert_token_style("csharp", "class Example {}", "class", "keyword");
    assert_token_style("cmake", "message(\"hello\")", "message", "function");
    let colors: gpui_component::highlighter::SyntaxColors =
        serde_json::from_value(serde_json::json!({
            "string.special": {"color": "#123456"}, "comment.doc": {"color": "#abcdef"}
        }))
        .unwrap();
    assert_eq!(
        colors.style("string.special.path"),
        colors.style("string.special")
    );
    assert!(colors.style("comment.doc").is_some());
}
