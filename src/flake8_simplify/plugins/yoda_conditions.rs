use rustpython_ast::{Cmpop, Expr, ExprKind, Location};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};
use crate::source_code_generator::SourceCodeGenerator;

/// SIM300
pub fn yoda_conditions(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    if !matches!(ops[..], [Cmpop::Eq]) {
        return;
    }

    if comparators.len() != 1 {
        return;
    }

    if !matches!(left.node, ExprKind::Constant { .. }) {
        return;
    }

    let right = comparators.first().unwrap();
    if matches!(left.node, ExprKind::Constant { .. })
        & matches!(right.node, ExprKind::Constant { .. })
    {
        return;
    }

    let mut check = Check::new(
        CheckKind::YodaConditions(left.to_string(), right.to_string()),
        Range::from_located(expr),
    );

    if checker.patch(check.kind.code()) {
        let cmp = Expr::new(
            Location::default(),
            Location::default(),
            ExprKind::Compare {
                left: Box::new(right.clone()),
                ops: vec![Cmpop::Eq],
                comparators: vec![left.clone()],
            },
        );
        let mut generator = SourceCodeGenerator::new(
            checker.style.indentation(),
            checker.style.quote(),
            checker.style.line_ending(),
        );
        generator.unparse_expr(&cmp, 0);

        if let Ok(content) = generator.generate() {
            check.amend(Fix::replacement(
                content,
                left.location,
                right.end_location.unwrap(),
            ));
        };
    }

    checker.add_check(check);
}
