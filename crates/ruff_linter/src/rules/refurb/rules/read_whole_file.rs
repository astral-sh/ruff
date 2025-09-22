use itertools::Itertools;
use ruff_diagnostics::{Applicability, Edit, Fix};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{
    self as ast, Expr, Stmt,
    visitor::{self, Visitor},
};
use ruff_python_codegen::Generator;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use crate::importer::ImportRequest;
use crate::rules::refurb::helpers::{FileOpen, find_file_opens};
use crate::{FixAvailability, Violation};

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
/// ## Fix Safety
/// This rule's fix is marked as unsafe if the replacement would remove comments attached to the original expression.
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
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`open` and `read` should be replaced by `Path({}).{}`",
            self.filename.truncated_display(),
            self.suggestion.truncated_display(),
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!(
            "Replace with `Path().{}`",
            self.suggestion.truncated_display(),
        ))
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
    let mut matcher = ReadMatcher::new(checker, candidates, with);
    visitor::walk_body(&mut matcher, &with.body);
}

/// AST visitor that matches `open` operations with the corresponding `read` calls.
struct ReadMatcher<'a, 'b> {
    checker: &'a Checker<'b>,
    candidates: Vec<FileOpen<'a>>,
    with_stmt: &'a ast::StmtWith,
}

impl<'a, 'b> ReadMatcher<'a, 'b> {
    fn new(
        checker: &'a Checker<'b>,
        candidates: Vec<FileOpen<'a>>,
        with_stmt: &'a ast::StmtWith,
    ) -> Self {
        Self {
            checker,
            candidates,
            with_stmt,
        }
    }
}

impl<'a> Visitor<'a> for ReadMatcher<'a, '_> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Some(read_from) = match_read_call(expr) {
            if let Some(open) = self
                .candidates
                .iter()
                .position(|open| open.is_ref(read_from))
            {
                let open = self.candidates.remove(open);
                let mut diagnostic = self.checker.report_diagnostic(
                    ReadWholeFile {
                        filename: SourceCodeSnippet::from_str(
                            &self.checker.generator().expr(open.filename),
                        ),
                        suggestion: make_suggestion(&open, self.checker.generator()),
                    },
                    open.item.range(),
                );

                if !crate::preview::is_fix_read_whole_file_enabled(self.checker.settings()) {
                    return;
                }

                if let Some(fix) = generate_fix(self.checker, &open, expr, self.with_stmt) {
                    diagnostic.set_fix(fix);
                }
            }
            return;
        }
        visitor::walk_expr(self, expr);
    }
}

fn is_simple_with_block(with_stmt: &ast::StmtWith) -> bool {
    with_stmt.items.len() == 1 && matches!(with_stmt.body.as_slice(), [Stmt::Assign(_)])
}

fn generate_fix(
    checker: &Checker,
    open: &FileOpen,
    read_expr: &Expr,
    with_stmt: &ast::StmtWith,
) -> Option<Fix> {
    // `closefd` and `opener` are not supported by pathlib, so check if they
    // are set to non-default values.
    // https://github.com/astral-sh/ruff/issues/7620
    // Signature as of Python 3.13 (https://docs.python.org/3/library/functions.html#open):
    // ```text
    // builtins.open(
    //   file,          0
    //   mode='r',      1 <= not supported by read_text() / read_text()
    //   buffering=-1,  2 <= not supported by read_text() / read_text()
    //   encoding=None, 3
    //   errors=None,   4
    //   newline=None,  5 <= not supported by read_text() / read_text()
    //   closefd=True,  6 <= not supported by pathlib
    //   opener=None    7 <= not supported by pathlib
    // )
    // ```
    // For `pathlib.Path.read_text()` (https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_text):
    // ```
    // def read_text(self, encoding=None, errors=None):
    //      """
    //      Open the file in text mode, read it, and close the file.
    //      """
    //      encoding = io.text_encoding(encoding)
    //      with self.open(mode='r', encoding=encoding, errors=errors) as f:
    //          return f.read()
    //
    // ```
    // For `pathlib.Path.read_bytes()` (https://docs.python.org/3/library/pathlib.html#pathlib.Path.read_bytes):
    // ```text
    // Path.read_bytes()
    // ```
    if !is_simple_with_block(with_stmt) {
        return None;
    }

    let target = match with_stmt.body.first() {
        Some(Stmt::Assign(assign)) if assign.value.range().contains_range(read_expr.range()) => {
            match assign.targets.first() {
                Some(Expr::Name(name)) => name.id.as_str(),
                _ => return None,
            }
        }
        _ => return None,
    };

    let locator = checker.locator();
    let filename_code = locator.slice(open.filename.range());

    let (import_edit, binding) = checker
        .importer()
        .get_or_import_symbol(
            &ImportRequest::import("pathlib", "Path"),
            with_stmt.start(),
            checker.semantic(),
        )
        .ok()?;

    let replacement = if open.keywords.is_empty() {
        format!(
            "{} = {}({}).{}()",
            target,
            binding,
            filename_code,
            open.mode.pathlib_method()
        )
    } else {
        format!(
            "{} = {}({}).{}({})",
            target,
            binding,
            filename_code,
            open.mode.pathlib_method(),
            open.keywords
                .iter()
                .map(|kw| locator.slice(kw.range()))
                .join(", ")
        )
    };

    let applicability = if checker.comment_ranges().intersects(with_stmt.range()) {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Some(Fix::applicable_edits(
        Edit::range_replacement(replacement, with_stmt.range()),
        [import_edit],
        applicability,
    ))
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
    let name = open.mode.pathlib_method();

    if open.keywords.is_empty() {
        return SourceCodeSnippet::from_str(&format!("{name}()"));
    }

    let call = ast::ExprCall {
        func: Box::new(
            ast::ExprName {
                id: name,
                ctx: ast::ExprContext::Load,
                range: TextRange::default(),
                node_index: ruff_python_ast::AtomicNodeIndex::NONE,
            }
            .into(),
        ),
        arguments: ast::Arguments {
            args: Box::from([]),
            keywords: open.keywords.iter().copied().cloned().collect(),
            range: TextRange::default(),
            node_index: ruff_python_ast::AtomicNodeIndex::NONE,
        },
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::NONE,
    };
    SourceCodeSnippet::from_str(&generator.expr(&call.into()))
}
