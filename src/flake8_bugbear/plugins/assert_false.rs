use log::error;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};
use crate::source_code_generator::SourceCodeGenerator;

fn assertion_error(msg: Option<&Expr>) -> Stmt {
    Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Raise {
            exc: Some(Box::new(Expr::new(
                Location::default(),
                Location::default(),
                ExprKind::Call {
                    func: Box::new(Expr::new(
                        Location::default(),
                        Location::default(),
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

    let mut check = Check::new(CheckKind::DoNotAssertFalse, Range::from_located(test));
    if checker.patch(check.kind.code()) {
        let mut generator = SourceCodeGenerator::new(
            checker.style.indentation(),
            checker.style.quote(),
            checker.style.line_ending(),
        );
        generator.unparse_stmt(&assertion_error(msg));
        match generator.generate() {
            Ok(content) => {
                check.amend(Fix::replacement(
                    content,
                    stmt.location,
                    stmt.end_location.unwrap(),
                ));
            }
            Err(e) => error!("Failed to rewrite `assert False`: {e}"),
        };
    }
    checker.add_check(check);
}
