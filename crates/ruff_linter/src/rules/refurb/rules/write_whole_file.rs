use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::relocate::relocate_expr;
use ruff_python_ast::visitor::{self, Visitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_codegen::Generator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;

use super::super::helpers::{FileOpen, find_file_opens};

/// ## What it does
/// Checks for uses of `open` and `write` that can be replaced by `pathlib`
/// methods, like `Path.write_text` and `Path.write_bytes`.
///
/// ## Why is this bad?
/// When writing a single string to a file, it's simpler and more concise
/// to use `pathlib` methods like `Path.write_text` and `Path.write_bytes`
/// instead of `open` and `write` calls via `with` statements.
///
/// ## Example
/// ```python
/// with open(filename, "w") as f:
///     f.write(contents)
/// ```
///
/// Use instead:
/// ```python
/// from pathlib import Path
///
/// Path(filename).write_text(contents)
/// ```
///
/// ## References
/// - [Python documentation: `Path.write_bytes`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.write_bytes)
/// - [Python documentation: `Path.write_text`](https://docs.python.org/3/library/pathlib.html#pathlib.Path.write_text)
#[derive(ViolationMetadata)]
pub(crate) struct WriteWholeFile {
    filename: SourceCodeSnippet,
    suggestion: SourceCodeSnippet,
}

impl Violation for WriteWholeFile {
    #[derive_message_formats]
    fn message(&self) -> String {
        let filename = self.filename.truncated_display();
        let suggestion = self.suggestion.truncated_display();
        format!("`open` and `write` should be replaced by `Path({filename}).{suggestion}`")
    }
}

/// FURB103
pub(crate) fn write_whole_file(checker: &Checker, with: &ast::StmtWith) {
    // `async` check here is more of a precaution.
    if with.is_async {
        return;
    }

    // First we go through all the items in the statement and find all `open` operations.
    let candidates = find_file_opens(with, checker.semantic(), false, checker.target_version());
    if candidates.is_empty() {
        return;
    }

    // Then we need to match each `open` operation with exactly one `write` call.
    let mut matcher = WriteMatcher::new(checker, candidates);
    visitor::walk_body(&mut matcher, &with.body);
}

/// AST visitor that matches `open` operations with the corresponding `write` calls.
struct WriteMatcher<'a, 'b> {
    checker: &'a Checker<'b>,
    candidates: Vec<FileOpen<'a>>,
    loop_counter: u32,
}

impl<'a, 'b> WriteMatcher<'a, 'b> {
    fn new(checker: &'a Checker<'b>, candidates: Vec<FileOpen<'a>>) -> Self {
        Self {
            checker,
            candidates,
            loop_counter: 0,
        }
    }
}

impl<'a> Visitor<'a> for WriteMatcher<'a, '_> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        if matches!(stmt, ast::Stmt::While(_) | ast::Stmt::For(_)) {
            self.loop_counter += 1;
            visitor::walk_stmt(self, stmt);
            self.loop_counter -= 1;
        } else {
            visitor::walk_stmt(self, stmt);
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Some((write_to, content)) = match_write_call(expr) {
            if let Some(open) = self
                .candidates
                .iter()
                .position(|open| open.is_ref(write_to))
            {
                if self.loop_counter == 0 {
                    let open = self.candidates.remove(open);
                    self.checker.report_diagnostic(Diagnostic::new(
                        WriteWholeFile {
                            filename: SourceCodeSnippet::from_str(
                                &self.checker.generator().expr(open.filename),
                            ),
                            suggestion: make_suggestion(&open, content, self.checker.generator()),
                        },
                        open.item.range(),
                    ));
                } else {
                    self.candidates.remove(open);
                }
            }
            return;
        }
        visitor::walk_expr(self, expr);
    }
}

/// Match `x.write(foo)` expression and return expression `x` and `foo` on success.
fn match_write_call(expr: &Expr) -> Option<(&Expr, &Expr)> {
    let call = expr.as_call_expr()?;
    let attr = call.func.as_attribute_expr()?;
    let method_name = &attr.attr;

    if method_name != "write"
        || !attr.value.is_name_expr()
        || call.arguments.args.len() != 1
        || !call.arguments.keywords.is_empty()
    {
        return None;
    }

    // `write` only takes in a single positional argument.
    Some((&*attr.value, call.arguments.args.first()?))
}

fn make_suggestion(open: &FileOpen<'_>, arg: &Expr, generator: Generator) -> SourceCodeSnippet {
    let name = ast::ExprName {
        id: open.mode.pathlib_method(),
        ctx: ast::ExprContext::Load,
        range: TextRange::default(),
    };
    let mut arg = arg.clone();
    relocate_expr(&mut arg, TextRange::default());
    let call = ast::ExprCall {
        func: Box::new(name.into()),
        arguments: ast::Arguments {
            args: Box::new([arg]),
            keywords: open.keywords.iter().copied().cloned().collect(),
            range: TextRange::default(),
        },
        range: TextRange::default(),
    };
    SourceCodeSnippet::from_str(&generator.expr(&call.into()))
}
