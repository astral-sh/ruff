use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;

fn assertion_error(msg: Option<&Expr>) -> Stmt {
    Stmt::new(
        Default::default(),
        Default::default(),
        StmtKind::Raise {
            exc: Some(Box::new(Expr::new(
                Default::default(),
                Default::default(),
                ExprKind::Call {
                    func: Box::new(Expr::new(
                        Default::default(),
                        Default::default(),
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
    if let ExprKind::Constant {
        value: Constant::Bool(false),
        ..
    } = &test.node
    {
        let mut check = Check::new(CheckKind::DoNotAssertFalse, Range::from_located(test));
        if checker.patch(check.kind.code()) {
            let mut generator = SourceGenerator::new();
            if let Ok(()) = generator.unparse_stmt(&assertion_error(msg)) {
                if let Ok(content) = generator.generate() {
                    check.amend(Fix::replacement(
                        content,
                        stmt.location,
                        stmt.end_location.unwrap(),
                    ));
                }
            }
        }
        checker.add_check(check);
    }
}
