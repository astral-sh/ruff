use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::Generator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

use super::super::helpers::{find_file_opens, FileOpen};

/// ## What it does
/// Checks for uses of `open` and `read` that can be replaced by `pathlib`
/// methods, like `Path.read_text` and `Path.read_bytes`.
///
/// ## Why is this bad?
/// When reading the entire contents of a file into a variable, it's simpler
/// and more concise to use `pathlib` methods like `Path.read_text` and
/// `Path.read_bytes` instead of `open` and `read` calls via `with` statements.
///
/// ## Example
/// ```python
/// with open(filename) as f:
///     contents = f.read()
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// contents = Path(filename).read_text()
/// ```
///
/// ## References
/// - [Python documentation: `Path.read_bytes`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_bytes)
/// - [Python documentation: `Path.read_text`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_text)
#[derive(ViolationMetadata)]
pub(crate) struct ReadWholeFile {
    filename: SourceCodeSnippet,
    suggestion: SourceCodeSnippet,
}

impl Violation for ReadWholeFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let filename = self.filename.truncated_display();
        let suggestion = self.suggestion.truncated_display();
        format!("`open` and `read` should be replaced by `Path({filename}).{suggestion}`")
    }
}

/// FURB101
pub(crate) fn read_whole_file(checker: &Checker, with: &ast::StmtWith) {
    // `async` check here is more of a precaution.
    if with.is_async {
        return;
    }

    // First we go through all the items in the statement and find all `open` operations.
    let candidates = find_file_opens(with, checker.semantic(), true, checker.target_version());
    if candidates.is_empty() {
        return;
    }

    // Then we need to match each `open` operation with exactly one `read` call.
    let matches = {
        let mut matcher = ReadMatcher::new(candidates);
        visitor::walk_body(&mut matcher, &with.body);
        matcher.into_matches()
    };

    // All the matched operations should be reported.
    let diagnostics: Vec<Diagnostic> = matches
        .iter()
        .map(|open| {
            Diagnostic::new(
                ReadWholeFile {
                    filename: SourceCodeSnippet::from_str(&checker.generator().expr(open.filename)),
                    suggestion: make_suggestion(open, checker.generator()),
                },
                open.item.range(),
            )
        })
        .collect();
    checker.report_diagnostics(diagnostics);
}

/// AST visitor that matches `open` operations with the corresponding `read` calls.
#[derive(Debug)]
struct ReadMatcher<'a> {
    candidates: Vec<FileOpen<'a>>,
    matches: Vec<FileOpen<'a>>,
}

impl<'a> ReadMatcher<'a> {
    fn new(candidates: Vec<FileOpen<'a>>) -> Self {
        Self {
            candidates,
            matches: vec![],
        }
    }

    fn into_matches(self) -> Vec<FileOpen<'a>> {
        self.matches
    }
}

impl<'a> Visitor<'a> for ReadMatcher<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Some(read_from) = match_read_call(expr) {
            if let Some(open) = self
                .candidates
                .iter()
                .position(|open| open.is_ref(read_from))
            {
                self.matches.push(self.candidates.remove(open));
            }
            return;
        }
        visitor::walk_expr(self, expr);
    }
}

/// Match `x.read()` expression and return expression `x` on success.
fn match_read_call(expr: &Expr) -> Option<&Expr> {
    let call = expr.as_call_expr()?;
    let attr = call.func.as_attribute_expr()?;
    let method_name = &attr.attr;

    if method_name != "read"
        || !attr.value.is_name_expr()
        || !call.arguments.args.is_empty()
        || !call.arguments.keywords.is_empty()
    {
        return None;
    }

    Some(&*attr.value)
}

fn make_suggestion(open: &FileOpen<'_>, generator: Generator) -> SourceCodeSnippet {
    let name = ast::ExprName {
        id: open.mode.pathlib_method(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    let call = ast::ExprCall {
        func: Box::new(name.into()),
        arguments: ast::Arguments {
            args: Box::from([]),
            keywords: open.keywords.iter().copied().cloned().collect(),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    SourceCodeSnippet::from_str(&generator.expr(&call.into()))
}
