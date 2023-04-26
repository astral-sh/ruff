use ruff_text_size::TextSize;
use rustpython_parser::ast::{Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
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
    Stmt::new(
        TextSize::default(),
        TextSize::default(),
        StmtKind::Raise {
            exc: Some(Box::new(Expr::new(
                TextSize::default(),
                TextSize::default(),
                ExprKind::Call {
                    func: Box::new(Expr::new(
                        TextSize::default(),
                        TextSize::default(),
                        ExprKind::Name {
                            id: "AssertionError".to_string(),
                            ctx: ExprContext::Load,
                        },
                    )),
                    args: if let Some(msg) = msg {
                        vec![msg.clone()]
                    } else {
                        vec![]
                    },
                    keywords: vec![],
                },
            ))),
            cause: None,
        },
    )
}

/// B011
pub fn assert_false(checker: &mut Checker, stmt: &Stmt, test: &Expr, msg: Option<&Expr>) {
    let ExprKind::Constant {
        value: Constant::Bool(false),
        ..
    } = &test.node else {
        return;
    };

    let mut diagnostic = Diagnostic::new(AssertFalse, test.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Edit::range_replacement(
            unparse_stmt(&assertion_error(msg), checker.stylist),
            stmt.range(),
        ));
    }
    checker.diagnostics.push(diagnostic);
}
