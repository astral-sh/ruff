use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind, Location, Operator};

use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    // TODO: document referencing [PEP 604]: https://peps.python.org/pep-0604/
    pub struct IsInstanceTypingUnion;
);
impl AlwaysAutofixableViolation for IsInstanceTypingUnion {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `X | Y` for type annotations")
    }

    fn autofix_title(&self) -> String {
        "Convert to `X | Y`".to_string()
    }
}

fn union(elts: &[Expr]) -> Expr {
    if elts.len() == 1 {
        elts[0].clone()
    } else {
        Expr::new(
            Location::default(),
            Location::default(),
            ExprKind::BinOp {
                left: Box::new(union(&elts[..elts.len() - 1])),
                op: Operator::BitOr,
                right: Box::new(elts[elts.len() - 1].clone()),
            },
        )
    }
}

pub fn use_pep604_isinstance(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if (id == "isinstance" || id == "issubclass") && checker.is_builtin(id) {
            if let Some(types) = args.get(1) {
                if let ExprKind::Tuple { elts, .. } = &types.node {
                    let mut diagnostic =
                        Diagnostic::new(IsInstanceTypingUnion, Range::from_located(expr));
                    if checker.patch(diagnostic.kind.rule()) {
                        diagnostic.amend(Fix::replacement(
                            unparse_expr(&union(elts), checker.stylist),
                            types.location,
                            types.end_location.unwrap(),
                        ));
                    }
                    checker.diagnostics.push(diagnostic);
                }
            }
        }
    }
}
