use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct NonEmptyStubBody;

impl AlwaysAutofixableViolation for NonEmptyStubBody {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function body must contain only `...`")
    }

    fn autofix_title(&self) -> String {
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
        if let Expr::Constant(ast::ExprConstant { value, .. }) = value.as_ref() {
            if value.is_ellipsis() {
                return;
            }
        }
    }

    let mut diagnostic = Diagnostic::new(NonEmptyStubBody, stmt.range());
    if checker.patch(Rule::NonEmptyStubBody) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            format!("..."),
            stmt.range(),
        )));
    };
    checker.diagnostics.push(diagnostic);
}
