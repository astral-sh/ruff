use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt::{Formatter, Write};

use datatest_stable::Utf8Path;
use itertools::Itertools;
use ruff_annotate_snippets::{Level, Renderer, Snippet};
use ruff_python_ast::token::{Token, Tokens};
use ruff_python_ast::visitor::Visitor;
use ruff_python_ast::visitor::source_order::{SourceOrderVisitor, TraversalSignal, walk_module};
use ruff_python_ast::{self as ast, AnyNodeRef, Mod, PythonVersion};
use ruff_python_parser::semantic_errors::{
    SemanticSyntaxChecker, SemanticSyntaxContext, SemanticSyntaxError,
};
use ruff_python_parser::{Mode, ParseErrorType, ParseOptions, Parsed, parse_unchecked};
use ruff_source_file::{LineIndex, OneIndexed, SourceCode};
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

#[expect(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn valid_syntax(path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    test_valid_syntax(path, &content, "./resources/valid");
    Ok(())
}

#[expect(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn invalid_syntax(path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    test_invalid_syntax(path, &content, "./resources/invalid");
    Ok(())
}

#[expect(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn inline_ok(path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    test_valid_syntax(path, &content, "./resources/inline/ok");
    Ok(())
}

#[expect(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn inline_err(path: &Utf8Path, content: String) -> datatest_stable::Result<()> {
    test_invalid_syntax(path, &content, "./resources/inline/err");
    Ok(())
}

datatest_stable::harness! {
    { test = valid_syntax, root = "./resources/valid", pattern = r"\.pyi?$" },
    { test = inline_ok, root = "./resources/inline/ok", pattern = r"\.pyi?$" },
    { test = invalid_syntax, root = "./resources/invalid", pattern = r"\.pyi?$" },
    { test = inline_err, root="./resources/inline/err", pattern = r"\.pyi?$" }
}

/// Asserts that the parser generates no syntax errors for a valid program.
/// Snapshots the AST.
fn test_valid_syntax(input_path: &Utf8Path, source: &str, root: &str) {
    let test_name = input_path.strip_prefix(root).unwrap_or(input_path).as_str();
    let options = extract_options(source).unwrap_or_else(|| {
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::latest_preview())
    });
    let parsed = parse_unchecked(source, options.clone());

    if parsed.has_syntax_errors() {
        let line_index = LineIndex::from_source_text(source);
        let source_code = SourceCode::new(source, &line_index);

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

    validate_tokens(parsed.tokens(), source.text_len());
    validate_ast(&parsed, source.text_len());

    let mut output = String::new();
    writeln!(&mut output, "## AST").unwrap();
    writeln!(&mut output, "\n```\n{:#?}\n```", parsed.syntax()).unwrap();

    let parsed = parsed.try_into_module().expect("Parsed with Mode::Module");

    let mut visitor =
        SemanticSyntaxCheckerVisitor::new(source).with_python_version(options.target_version());

    for stmt in parsed.suite() {
        visitor.visit_stmt(stmt);
    }

    let semantic_syntax_errors = visitor.into_diagnostics();

    if !semantic_syntax_errors.is_empty() {
        let mut message = "Expected no semantic syntax errors for a valid program:\n".to_string();

        let line_index = LineIndex::from_source_text(source);
        let source_code = SourceCode::new(source, &line_index);

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
        snapshot_suffix => test_name
    }, {
        insta::assert_snapshot!(output);
    });
}

/// Assert that the parser generates at least one syntax error for the given input file.
/// Snapshots the AST and the error messages.
fn test_invalid_syntax(input_path: &Utf8Path, source: &str, root: &str) {
    let test_name = input_path.strip_prefix(root).unwrap_or(input_path).as_str();

    let options = extract_options(source).unwrap_or_else(|| {
        ParseOptions::from(Mode::Module).with_target_version(PythonVersion::PY314)
    });
    let parsed = parse_unchecked(source, options.clone());

    validate_tokens(parsed.tokens(), source.text_len());
    validate_ast(&parsed, source.text_len());

    let mut output = String::new();
    writeln!(&mut output, "## AST").unwrap();
    writeln!(&mut output, "\n```\n{:#?}\n```", parsed.syntax()).unwrap();

    let line_index = LineIndex::from_source_text(source);
    let source_code = SourceCode::new(source, &line_index);

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

    let mut visitor =
        SemanticSyntaxCheckerVisitor::new(source).with_python_version(options.target_version());

    for stmt in parsed.suite() {
        visitor.visit_stmt(stmt);
    }

    let semantic_syntax_errors = visitor.into_diagnostics();

    assert!(
        parsed.has_syntax_errors() || !semantic_syntax_errors.is_empty(),
        "Expected parser to generate at least one syntax error for a program containing syntax errors."
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
        snapshot_suffix => test_name
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
#[expect(clippy::print_stdout)]
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
fn validate_tokens(tokens: &[Token], source_length: TextSize) {
    let mut previous: Option<&Token> = None;

    for token in tokens {
        assert!(
            token.end() <= source_length,
            "Token range exceeds the source code length. Token: {token:#?}",
        );

        if let Some(previous) = previous {
            assert_eq!(
                previous.range().ordering(token.range()),
                Ordering::Less,
                "Token ranges are not in increasing order
Previous token: {previous:#?}
Current token: {token:#?}
Tokens: {tokens:#?}
",
            );
        }

        previous = Some(token);
    }
}

/// Verifies that:
/// * the range of the parent node fully encloses all its child nodes
/// * the ranges are strictly increasing when traversing the nodes in pre-order.
/// * all ranges are within the length of the source code.
fn validate_ast(parsed: &Parsed<Mod>, source_len: TextSize) {
    walk_module(
        &mut ValidateAstVisitor::new(parsed.tokens(), source_len),
        parsed.syntax(),
    );
}

#[derive(Debug)]
struct ValidateAstVisitor<'a> {
    tokens: std::iter::Peekable<std::slice::Iter<'a, Token>>,
    parents: Vec<AnyNodeRef<'a>>,
    previous: Option<AnyNodeRef<'a>>,
    source_length: TextSize,
}

impl<'a> ValidateAstVisitor<'a> {
    fn new(tokens: &'a Tokens, source_length: TextSize) -> Self {
        Self {
            tokens: tokens.iter().peekable(),
            parents: Vec::new(),
            previous: None,
            source_length,
        }
    }
}

impl ValidateAstVisitor<'_> {
    /// Check that the node's start doesn't fall within a token.
    /// Called in `enter_node` before visiting children.
    fn assert_start_boundary(&mut self, node: AnyNodeRef<'_>) {
        // Skip tokens that end at or before the node starts.
        self.tokens
            .peeking_take_while(|t| t.end() <= node.start())
            .last();

        if let Some(next) = self.tokens.peek() {
            // At this point, next_token.end() > node.start()
            assert!(
                next.start() >= node.start(),
                "The start of the node falls within a token.\nNode: {node:#?}\n\nToken: {next:#?}\n\nRoot: {root:#?}",
                root = self.parents.first()
            );
        }
    }

    /// Check that the node's end doesn't fall within a token.
    /// Called in `leave_node` after visiting children, so all tokens
    /// within the node have been consumed.
    fn assert_end_boundary(&mut self, node: AnyNodeRef<'_>) {
        // Skip tokens that end at or before the node ends.
        self.tokens
            .peeking_take_while(|t| t.end() <= node.end())
            .last();

        if let Some(next) = self.tokens.peek() {
            // At this point, `next_token.end() > node.end()`
            assert!(
                next.start() >= node.end(),
                "The end of the node falls within a token.\nNode: {node:#?}\n\nToken: {next:#?}\n\nRoot: {root:#?}",
                root = self.parents.first()
            );
        }
    }
}

impl<'ast> SourceOrderVisitor<'ast> for ValidateAstVisitor<'ast> {
    fn enter_node(&mut self, node: AnyNodeRef<'ast>) -> TraversalSignal {
        assert!(
            node.end() <= self.source_length,
            "The range of the node exceeds the length of the source code. Node: {node:#?}",
        );

        if let Some(previous) = self.previous {
            assert_ne!(
                previous.range().ordering(node.range()),
                Ordering::Greater,
                "The ranges of the nodes are not strictly increasing when traversing the AST in pre-order.\nPrevious node: {previous:#?}\n\nCurrent node: {node:#?}\n\nRoot: {root:#?}",
                root = self.parents.first()
            );
        }

        if let Some(parent) = self.parents.last() {
            assert!(
                parent.range().contains_range(node.range()),
                "The range of the parent node does not fully enclose the range of the child node.\nParent node: {parent:#?}\n\nChild node: {node:#?}\n\nRoot: {root:#?}",
                root = self.parents.first()
            );
        }

        self.assert_start_boundary(node);

        self.parents.push(node);

        TraversalSignal::Traverse
    }

    fn leave_node(&mut self, node: AnyNodeRef<'ast>) {
        self.assert_end_boundary(node);

        self.parents.pop().expect("Expected tree to be balanced");

        self.previous = Some(node);
    }
}

enum Scope {
    Module,
    Function { is_async: bool },
    Comprehension { is_async: bool },
    Class,
}

struct SemanticSyntaxCheckerVisitor<'a> {
    checker: SemanticSyntaxChecker,
    diagnostics: RefCell<Vec<SemanticSyntaxError>>,
    python_version: PythonVersion,
    source: &'a str,
    scopes: Vec<Scope>,
}

impl<'a> SemanticSyntaxCheckerVisitor<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            checker: SemanticSyntaxChecker::new(),
            diagnostics: RefCell::default(),
            python_version: PythonVersion::default(),
            source,
            scopes: vec![Scope::Module],
        }
    }

    #[must_use]
    fn with_python_version(mut self, python_version: PythonVersion) -> Self {
        self.python_version = python_version;
        self
    }

    fn into_diagnostics(self) -> Vec<SemanticSyntaxError> {
        self.diagnostics.into_inner()
    }

    fn with_semantic_checker(&mut self, f: impl FnOnce(&mut SemanticSyntaxChecker, &Self)) {
        let mut checker = std::mem::take(&mut self.checker);
        f(&mut checker, self);
        self.checker = checker;
    }
}

impl SemanticSyntaxContext for SemanticSyntaxCheckerVisitor<'_> {
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

    fn has_nonlocal_binding(&self, _name: &str) -> bool {
        true
    }

    fn in_async_context(&self) -> bool {
        if let Some(scope) = self.scopes.iter().next_back() {
            match scope {
                Scope::Class | Scope::Module => false,
                Scope::Comprehension { is_async } => *is_async,
                Scope::Function { is_async } => *is_async,
            }
        } else {
            false
        }
    }

    fn in_sync_comprehension(&self) -> bool {
        for scope in &self.scopes {
            if let Scope::Comprehension { is_async: false } = scope {
                return true;
            }
        }
        false
    }

    fn in_module_scope(&self) -> bool {
        self.scopes.len() == 1
    }

    fn in_function_scope(&self) -> bool {
        true
    }

    fn in_notebook(&self) -> bool {
        false
    }

    fn in_await_allowed_context(&self) -> bool {
        true
    }

    fn in_yield_allowed_context(&self) -> bool {
        true
    }

    fn in_generator_context(&self) -> bool {
        true
    }

    fn in_loop_context(&self) -> bool {
        true
    }

    fn is_bound_parameter(&self, _name: &str) -> bool {
        false
    }
}

impl Visitor<'_> for SemanticSyntaxCheckerVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        self.with_semantic_checker(|semantic, context| semantic.visit_stmt(stmt, context));
        match stmt {
            ast::Stmt::ClassDef(ast::StmtClassDef {
                arguments,
                body,
                decorator_list,
                type_params,
                ..
            }) => {
                for decorator in decorator_list {
                    self.visit_decorator(decorator);
                }
                if let Some(type_params) = type_params {
                    self.visit_type_params(type_params);
                }
                if let Some(arguments) = arguments {
                    self.visit_arguments(arguments);
                }
                self.scopes.push(Scope::Class);
                self.visit_body(body);
                self.scopes.pop().unwrap();
            }
            ast::Stmt::FunctionDef(ast::StmtFunctionDef { is_async, .. }) => {
                self.scopes.push(Scope::Function {
                    is_async: *is_async,
                });
                ast::visitor::walk_stmt(self, stmt);
                self.scopes.pop().unwrap();
            }
            _ => {
                ast::visitor::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        self.with_semantic_checker(|semantic, context| semantic.visit_expr(expr, context));
        match expr {
            ast::Expr::Lambda(_) => {
                self.scopes.push(Scope::Function { is_async: false });
                ast::visitor::walk_expr(self, expr);
                self.scopes.pop().unwrap();
            }
            ast::Expr::ListComp(ast::ExprListComp {
                elt, generators, ..
            })
            | ast::Expr::SetComp(ast::ExprSetComp {
                elt, generators, ..
            })
            | ast::Expr::Generator(ast::ExprGenerator {
                elt, generators, ..
            }) => {
                for comprehension in generators {
                    self.visit_comprehension(comprehension);
                }
                self.scopes.push(Scope::Comprehension {
                    is_async: generators.iter().any(|generator| generator.is_async),
                });
                self.visit_expr(elt);
                self.scopes.pop().unwrap();
            }
            ast::Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                ..
            }) => {
                for comprehension in generators {
                    self.visit_comprehension(comprehension);
                }
                self.scopes.push(Scope::Comprehension {
                    is_async: generators.iter().any(|generator| generator.is_async),
                });
                self.visit_expr(key);
                self.visit_expr(value);
                self.scopes.pop().unwrap();
            }
            _ => {
                ast::visitor::walk_expr(self, expr);
            }
        }
    }
}
