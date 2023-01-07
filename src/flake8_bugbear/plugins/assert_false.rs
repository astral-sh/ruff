use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::source_code_generator::SourceCodeGenerator;
use crate::violations;

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

    let mut check = Diagnostic::new(violations::DoNotAssertFalse, Range::from_located(test));
    if checker.patch(check.kind.code()) {
        let mut generator: SourceCodeGenerator = checker.style.into();
        generator.unparse_stmt(&assertion_error(msg));
        check.amend(Fix::replacement(
            generator.generate(),
            stmt.location,
            stmt.end_location.unwrap(),
        ));
    }
    checker.checks.push(check);
}
