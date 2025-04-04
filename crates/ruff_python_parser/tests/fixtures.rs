use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt::{Formatter, Write};
use std::fs;
use std::path::Path;

use ruff_annotate_snippets::{Level, Renderer, Snippet};
use ruff_python_ast::visitor::source_order::{walk_module, SourceOrderVisitor, TraversalSignal};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::{AnyNodeRef, Mod, PythonVersion};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxCheckerVisitor, SemanticSyntaxContext, SemanticSyntaxError,
};
use ruff_python_parser::{parse_unchecked, Mode, ParseErrorType, ParseOptions, Token};
use ruff_source_file::{LineIndex, OneIndexed, SourceCode};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

#[test]
fn valid_syntax() {
    insta::glob!("../resources", "valid/**/*.py", test_valid_syntax);
}

#[test]
fn invalid_syntax() {
    insta::glob!("../resources", "invalid/**/*.py", test_invalid_syntax);
}

#[test]
fn inline_ok() {
    insta::glob!("../resources/inline", "ok/**/*.py", test_valid_syntax);
}

#[test]
fn inline_err() {
    insta::glob!("../resources/inline", "err/**/*.py", test_invalid_syntax);
}

/// Asserts that the parser generates no syntax errors for a valid program.
/// Snapshots the AST.
fn test_valid_syntax(input_path: &Path) {
    let source = fs::read_to_string(input_path).expect("Expected test file to exist");
    let options = extract_options(&source).unwrap_or_else(|| {
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::latest())
    });
    let parsed = parse_unchecked(&source, options.clone());

    if parsed.has_syntax_errors() {
        let line_index = LineIndex::from_source_text(&source);
        let source_code = SourceCode::new(&source, &line_index);

        let mut message = "Expected no syntax errors for a valid program but the parser generated the following errors:\n".to_string();

        for error in parsed.errors() {
            writeln!(
                &mut message,
                "{}\n",
                CodeFrame {
                    range: error.location,
                    error,
                    source_code: &source_code,
                }
            )
            .unwrap();
        }

        for error in parsed.unsupported_syntax_errors() {
            writeln!(
                &mut message,
                "{}\n",
                CodeFrame {
                    range: error.range,
                    error: &ParseErrorType::OtherError(error.to_string()),
                    source_code: &source_code,
                }
            )
            .unwrap();
        }

        panic!("{input_path:?}: {message}");
    }

    validate_tokens(parsed.tokens(), source.text_len(), input_path);
    validate_ast(parsed.syntax(), source.text_len(), input_path);

    let mut output = String::new();
    writeln!(&mut output, "## AST").unwrap();
    writeln!(&mut output, "\n```\n{:#?}\n```", parsed.syntax()).unwrap();

    let parsed = parsed.try_into_module().expect("Parsed with Mode::Module");

    let mut visitor = SemanticSyntaxCheckerVisitor::new(
        TestContext::new(&source).with_python_version(options.target_version()),
    );

    for stmt in parsed.suite() {
        visitor.visit_stmt(stmt);
    }

    let semantic_syntax_errors = visitor.into_context().diagnostics.into_inner();

    if !semantic_syntax_errors.is_empty() {
        let mut message = "Expected no semantic syntax errors for a valid program:\n".to_string();

        let line_index = LineIndex::from_source_text(&source);
        let source_code = SourceCode::new(&source, &line_index);

        for error in semantic_syntax_errors {
            writeln!(
                &mut message,
                "{}\n",
                CodeFrame {
                    range: error.range,
                    error: &ParseErrorType::OtherError(error.to_string()),
                    source_code: &source_code,
                }
            )
            .unwrap();
        }

        panic!("{input_path:?}: {message}");
    }

    insta::with_settings!({
        omit_expression => true,
        input_file => input_path,
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!(output);
    });
}

/// Assert that the parser generates at least one syntax error for the given input file.
/// Snapshots the AST and the error messages.
fn test_invalid_syntax(input_path: &Path) {
    let source = fs::read_to_string(input_path).expect("Expected test file to exist");
    let options = extract_options(&source).unwrap_or_else(|| {
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::latest())
    });
    let parsed = parse_unchecked(&source, options.clone());

    validate_tokens(parsed.tokens(), source.text_len(), input_path);
    validate_ast(parsed.syntax(), source.text_len(), input_path);

    let mut output = String::new();
    writeln!(&mut output, "## AST").unwrap();
    writeln!(&mut output, "\n```\n{:#?}\n```", parsed.syntax()).unwrap();

    let line_index = LineIndex::from_source_text(&source);
    let source_code = SourceCode::new(&source, &line_index);

    if !parsed.errors().is_empty() {
        writeln!(&mut output, "## Errors\n").unwrap();
    }

    for error in parsed.errors() {
        writeln!(
            &mut output,
            "{}\n",
            CodeFrame {
                range: error.location,
                error,
                source_code: &source_code,
            }
        )
        .unwrap();
    }

    if !parsed.unsupported_syntax_errors().is_empty() {
        writeln!(&mut output, "## Unsupported Syntax Errors\n").unwrap();
    }

    for error in parsed.unsupported_syntax_errors() {
        writeln!(
            &mut output,
            "{}\n",
            CodeFrame {
                range: error.range,
                error: &ParseErrorType::OtherError(error.to_string()),
                source_code: &source_code,
            }
        )
        .unwrap();
    }

    let parsed = parsed.try_into_module().expect("Parsed with Mode::Module");

    let mut visitor = SemanticSyntaxCheckerVisitor::new(
        TestContext::new(&source).with_python_version(options.target_version()),
    );

    for stmt in parsed.suite() {
        visitor.visit_stmt(stmt);
    }

    let semantic_syntax_errors = visitor.into_context().diagnostics.into_inner();

    assert!(
        parsed.has_syntax_errors() || !semantic_syntax_errors.is_empty(),
        "{input_path:?}: Expected parser to generate at least one syntax error for a program containing syntax errors."
    );

    if !semantic_syntax_errors.is_empty() {
        writeln!(&mut output, "## Semantic Syntax Errors\n").unwrap();
    }

    for error in semantic_syntax_errors {
        writeln!(
            &mut output,
            "{}\n",
            CodeFrame {
                range: error.range,
                error: &ParseErrorType::OtherError(error.to_string()),
                source_code: &source_code,
            }
        )
        .unwrap();
    }

    insta::with_settings!({
        omit_expression => true,
        input_file => input_path,
        prepend_module_to_snapshot => false,
    }, {
        insta::assert_snapshot!(output);
    });
}

/// Copy of [`ParseOptions`] for deriving [`Deserialize`] with serde as a dev-dependency.
#[derive(serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
struct JsonParseOptions {
    #[serde(default)]
    mode: JsonMode,
    #[serde(default)]
    target_version: PythonVersion,
}

/// Copy of [`Mode`] for deserialization.
#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
enum JsonMode {
    #[default]
    Module,
    Expression,
    ParenthesizedExpression,
    Ipython,
}

impl From<JsonParseOptions> for ParseOptions {
    fn from(value: JsonParseOptions) -> Self {
        let mode = match value.mode {
            JsonMode::Module => Mode::Module,
            JsonMode::Expression => Mode::Expression,
            JsonMode::ParenthesizedExpression => Mode::ParenthesizedExpression,
            JsonMode::Ipython => Mode::Ipython,
        };
        Self::from(mode).with_target_version(value.target_version)
    }
}

/// Extract [`ParseOptions`] from an initial pragma line, if present.
///
/// For example,
///
/// ```python
/// # parse_options: { "target-version": "3.10" }
/// def f(): ...
fn extract_options(source: &str) -> Option<ParseOptions> {
    let header = source.lines().next()?;
    let (_label, options) = header.split_once("# parse_options: ")?;
    let options: Option<JsonParseOptions> = serde_json::from_str(options.trim()).ok();
    options.map(ParseOptions::from)
}

// Test that is intentionally ignored by default.
// Use it for quickly debugging a parser issue.
#[test]
#[ignore]
#[allow(clippy::print_stdout)]
fn parser_quick_test() {
    let source = "\
f'{'
f'{foo!r'
";

    let parsed = parse_unchecked(source, ParseOptions::from(Mode::Module));

    println!("AST:\n----\n{:#?}", parsed.syntax());
    println!("Tokens:\n-------\n{:#?}", parsed.tokens());

    if parsed.has_invalid_syntax() {
        println!("Errors:\n-------");

        let line_index = LineIndex::from_source_text(source);
        let source_code = SourceCode::new(source, &line_index);

        for error in parsed.errors() {
            // Sometimes the code frame doesn't show the error message, so we print
            // the message as well.
            println!("Syntax Error: {error}");
            println!(
                "{}\n",
                CodeFrame {
                    range: error.location,
                    error,
                    source_code: &source_code,
                }
            );
        }

        println!();
    }
}

struct CodeFrame<'a> {
    range: TextRange,
    error: &'a ParseErrorType,
    source_code: &'a SourceCode<'a, 'a>,
}

impl std::fmt::Display for CodeFrame<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        // Copied and modified from ruff_linter/src/message/text.rs
        let content_start_index = self.source_code.line_index(self.range.start());
        let mut start_index = content_start_index.saturating_sub(2);

        // Trim leading empty lines.
        while start_index < content_start_index {
            if !self.source_code.line_text(start_index).trim().is_empty() {
                break;
            }
            start_index = start_index.saturating_add(1);
        }

        let content_end_index = self.source_code.line_index(self.range.end());
        let mut end_index = content_end_index
            .saturating_add(2)
            .min(OneIndexed::from_zero_indexed(self.source_code.line_count()));

        // Trim trailing empty lines.
        while end_index > content_end_index {
            if !self.source_code.line_text(end_index).trim().is_empty() {
                break;
            }

            end_index = end_index.saturating_sub(1);
        }

        let start_offset = self.source_code.line_start(start_index);
        let end_offset = self.source_code.line_end(end_index);

        let annotation_range = self.range - start_offset;
        let source = self
            .source_code
            .slice(TextRange::new(start_offset, end_offset));

        let label = format!("Syntax Error: {error}", error = self.error);

        let span = usize::from(annotation_range.start())..usize::from(annotation_range.end());
        let annotation = Level::Error.span(span).label(&label);
        let snippet = Snippet::source(source)
            .line_start(start_index.get())
            .annotation(annotation)
            .fold(false);
        let message = Level::None.title("").snippet(snippet);
        let renderer = Renderer::plain().cut_indicator("…");
        let rendered = renderer.render(message);
        writeln!(f, "{rendered}")
    }
}

/// Verifies that:
/// * the ranges are strictly increasing when loop the tokens in insertion order
/// * all ranges are within the length of the source code
fn validate_tokens(tokens: &[Token], source_length: TextSize, test_path: &Path) {
    let mut previous: Option<&Token> = None;

    for token in tokens {
        assert!(
            token.end() <= source_length,
            "{path}: Token range exceeds the source code length. Token: {token:#?}",
            path = test_path.display()
        );

        if let Some(previous) = previous {
            assert_eq!(
                previous.range().ordering(token.range()),
                Ordering::Less,
                "{path}: Token ranges are not in increasing order
Previous token: {previous:#?}
Current token: {token:#?}
Tokens: {tokens:#?}
",
                path = test_path.display(),
            );
        }

        previous = Some(token);
    }
}

/// Verifies that:
/// * the range of the parent node fully encloses all its child nodes
/// * the ranges are strictly increasing when traversing the nodes in pre-order.
/// * all ranges are within the length of the source code.
fn validate_ast(root: &Mod, source_len: TextSize, test_path: &Path) {
    walk_module(&mut ValidateAstVisitor::new(source_len, test_path), root);
}

#[derive(Debug)]
struct ValidateAstVisitor<'a> {
    parents: Vec<AnyNodeRef<'a>>,
    previous: Option<AnyNodeRef<'a>>,
    source_length: TextSize,
    test_path: &'a Path,
}

impl<'a> ValidateAstVisitor<'a> {
    fn new(source_length: TextSize, test_path: &'a Path) -> Self {
        Self {
            parents: Vec::new(),
            previous: None,
            source_length,
            test_path,
        }
    }
}

impl<'ast> SourceOrderVisitor<'ast> for ValidateAstVisitor<'ast> {
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        assert!(
            node.end() <= self.source_length,
            "{path}: The range of the node exceeds the length of the source code. Node: {node:#?}",
            path = self.test_path.display()
        );

        if let Some(previous) = self.previous {
            assert_ne!(previous.range().ordering(node.range()), Ordering::Greater,
                "{path}: The ranges of the nodes are not strictly increasing when traversing the AST in pre-order.\nPrevious node: {previous:#?}\n\nCurrent node: {node:#?}\n\nRoot: {root:#?}",
                path = self.test_path.display(),
                root = self.parents.first()
            );
        }

        if let Some(parent) = self.parents.last() {
            assert!(parent.range().contains_range(node.range()),
                "{path}: The range of the parent node does not fully enclose the range of the child node.\nParent node: {parent:#?}\n\nChild node: {node:#?}\n\nRoot: {root:#?}",
                path = self.test_path.display(),
                root = self.parents.first()
            );
        }

        self.parents.push(node);

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'ast>) {
        self.parents.pop().expect("Expected tree to be balanced");

        self.previous = Some(node);
    }
}

#[derive(Debug)]
struct TestContext<'a> {
    diagnostics: RefCell<Vec<SemanticSyntaxError>>,
    python_version: PythonVersion,
    source: &'a str,
}

impl<'a> TestContext<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            diagnostics: RefCell::default(),
            python_version: PythonVersion::default(),
            source,
        }
    }

    #[must_use]
    fn with_python_version(mut self, python_version: PythonVersion) -> Self {
        self.python_version = python_version;
        self
    }
}

impl SemanticSyntaxContext for TestContext<'_> {
    fn seen_docstring_boundary(&self) -> bool {
        false
    }

    fn future_annotations_or_stub(&self) -> bool {
        false
    }

    fn python_version(&self) -> PythonVersion {
        self.python_version
    }

    fn report_semantic_error(&self, error: SemanticSyntaxError) {
        self.diagnostics.borrow_mut().push(error);
    }

    fn source(&self) -> &str {
        self.source
    }

    fn global(&self, _name: &str) -> Option<TextRange> {
        None
    }
}
