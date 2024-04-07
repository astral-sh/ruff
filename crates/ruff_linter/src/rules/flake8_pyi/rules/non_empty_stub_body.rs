use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for non-empty function stub bodies.
///
/// ## Why is this bad?
/// Stub files are never executed at runtime; they should be thought of as
/// "data files" for type checkers or IDEs. Function bodies are redundant
/// for this purpose.
///
/// ## Example
/// ```python
/// def double(x: int) -> int:
///     return x * 2
/// ```
///
/// Use instead:
/// ```python
/// def double(x: int) -> int: ...
/// ```
///
/// ## References
/// - [The recommended style for stub functions and methods](https://typing.readthedocs.io/en/latest/source/stubs.html#id6)
///   in the typing docs.
#[violation]
pub struct NonEmptyStubBody;

impl AlwaysFixableViolation for NonEmptyStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain only `...`")
    }

    fn fix_title(&self) -> String {
        format!("Replace function body with `...`")
    }
}

/// PYI010
pub(crate) fn non_empty_stub_body(checker: &mut Checker, body: &[Stmt]) {
    // Ignore multi-statement bodies (covered by PYI048).
    let [stmt] = body else {
        return;
    };

    // Ignore `pass` statements (covered by PYI009).
    if stmt.is_pass_stmt() {
        return;
    }

    // Ignore docstrings (covered by PYI021).
    if is_docstring_stmt(stmt) {
        return;
    }

    // Ignore `...` (the desired case).
    if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt {
        if value.is_ellipsis_literal_expr() {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(NonEmptyStubBody, stmt.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        format!("..."),
        stmt.range(),
    )));
    checker.diagnostics.push(diagnostic);
}
