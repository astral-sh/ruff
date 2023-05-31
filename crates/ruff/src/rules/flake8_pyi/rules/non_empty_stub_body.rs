use rustpython_parser::ast::{self, Constant, Expr, Ranged, Stmt};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};

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
    if body.len() != 1 {
        return;
    }
    if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = &body[0] {
        if let Expr::Constant(ast::ExprConstant { value, .. }) = value.as_ref() {
            if matches!(value, Constant::Ellipsis | Constant::Str(_)) {
                return;
            }
        }
    }
    let mut diagnostic = Diagnostic::new(NonEmptyStubBody, body[0].range());
    if checker.patch(Rule::NonEmptyStubBody) {
        diagnostic.set_fix(Fix::automatic(Edit::range_replacement(
            format!("..."),
            body[0].range(),
        )));
    };
    checker.diagnostics.push(diagnostic);
}
