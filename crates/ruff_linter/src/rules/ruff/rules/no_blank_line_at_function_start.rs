use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StmtFunctionDef;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for blank lines between the `def`/`async def` header of a function
/// and the first statement of its body, when the function has no docstring.
///
/// ## Why is this bad?
/// A blank line between a function header and its body is rarely intentional;
/// it usually means a debug print was left in or a stray line was added
/// accidentally. Allowing it as part of the formatter style leads to
/// inconsistent code, and reviewers have to either nitpick or let it slide.
///
/// The pydocstyle rules `D201` (blank line before docstring) and `D202`
/// (blank line after docstring) cover the equivalent situation when the
/// function starts with a docstring. This rule covers the non-docstring case
/// for symmetry.
///
/// ## Example
/// ```python
/// def hello():
///
///     print("meow")
/// ```
///
/// Use instead:
/// ```python
/// def hello():
///     print("meow")
/// ```
///
/// ## Fix safety
/// This fix removes whitespace only and never changes program semantics.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.18")]
pub(crate) struct NoBlankLineAtFunctionStart;

impl AlwaysFixableViolation for NoBlankLineAtFunctionStart {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Blank line at the start of a function body".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove blank line".into()
    }
}

/// RUF077
pub(crate) fn no_blank_line_at_function_start(checker: &Checker, function_def: &StmtFunctionDef) {
    let body = &function_def.body;

    // Skip empty bodies.
    if body.is_empty() {
        return;
    }

    // If the first body statement is a docstring, D201/D202 already handle
    // that case — bail out to avoid duplicate diagnostics.
    let first_stmt = &body[0];
    if is_docstring_stmt(first_stmt) {
        return;
    }

    // Locate the position immediately after the newline that terminates the
    // `def foo(...):` header line. The signature's `parameters` node ends at
    // the colon, so we scan forward from there for the next `\n`.
    let Some(header_end) = header_line_end(checker, function_def) else {
        return;
    };

    if header_end >= first_stmt.start() {
        // Defensive: pathological AST where the body starts on the same
        // line as the header. The `body.is_empty()` check above should
        // already cover the most obvious case.
        return;
    }

    // Count the newlines in the slice between the header's newline and the
    // first body statement. Each newline corresponds to one blank line
    // between the header and the body.
    let between = checker.locator().slice(TextRange::new(header_end, first_stmt.start()));
    let blank_line_count = between.bytes().filter(|&b| b == b'\n').count();

    if blank_line_count == 0 {
        return;
    }

    // Compute the fix: delete from after the header's newline through the
    // start of the line that contains the first body statement. This
    // collapses all blank lines into nothing while preserving the body's
    // leading indentation.
    let body_line_start = start_of_line_containing(checker, first_stmt.start());
    let mut diagnostic = checker.report_diagnostic(NoBlankLineAtFunctionStart, first_stmt.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::deletion(header_end, body_line_start)));
}

/// Returns the position immediately after the first `\n` encountered
/// after the function signature's colon, or `None` if no such `\n` exists.
fn header_line_end(checker: &Checker, function_def: &StmtFunctionDef) -> Option<TextSize> {
    let contents = checker.locator().contents();
    let bytes = contents.as_bytes();
    let mut pos = function_def.parameters.end();
    while pos.to_usize() < bytes.len() {
        let b = bytes[pos.to_usize()];
        pos = pos + TextSize::from(1);
        if b == b'\n' {
            return Some(pos);
        }
    }
    None
}

/// Returns the position of column 0 of the line containing `pos`.
fn start_of_line_containing(checker: &Checker, pos: TextSize) -> TextSize {
    let contents = checker.locator().contents();
    let bytes = contents.as_bytes();
    let mut start = pos.to_usize();
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    TextSize::from(start as u32)
}

/// Returns `true` if the statement is a string literal expression
/// (i.e. a docstring).
fn is_docstring_stmt(stmt: &ruff_python_ast::Stmt) -> bool {
    matches!(
        stmt,
        ruff_python_ast::Stmt::Expr(ruff_python_ast::StmtExpr { value, .. })
            if matches!(value.as_ref(), ruff_python_ast::Expr::StringLiteral(_))
    )
}