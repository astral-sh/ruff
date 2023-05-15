use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprContext, Ranged, Stmt};
use thin_vec::thin_vec;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_stmt;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct AssertFalse;

impl AlwaysAutofixableViolation for AssertFalse {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Do not `assert False` (`python -O` removes these calls), raise `AssertionError()`")
    }

    fn autofix_title(&self) -> String {
        "Replace `assert False`".to_string()
    }
}

fn assertion_error(msg: Option<&Expr>) -> Stmt {
    Stmt::Raise(ast::StmtRaise {
        range: TextRange::default(),
        exc: Some(Box::new(Expr::Call(ast::ExprCall {
            func: Box::new(Expr::Name(ast::ExprName {
                id: "AssertionError".into(),
                ctx: ExprContext::Load,
                range: TextRange::default(),
            })),
            args: if let Some(msg) = msg {
                thin_vec![msg.clone()]
            } else {
                thin_vec![]
            },
            keywords: thin_vec![],
            range: TextRange::default(),
        }))),
        cause: None,
    })
}

/// B011
pub(crate) fn assert_false(checker: &mut Checker, stmt: &Stmt, test: &Expr, msg: Option<&Expr>) {
    let Expr::Constant(ast::ExprConstant {
        value: Constant::Bool(false),
        ..
    } )= &test else {
        return;
    };

    let mut diagnostic = Diagnostic::new(AssertFalse, test.range());
    if checker.patch(diagnostic.kind.rule()) {
        #[allow(deprecated)]
        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
            unparse_stmt(&assertion_error(msg), checker.stylist),
            stmt.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
