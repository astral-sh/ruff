use std::fmt;

use rustpython_parser::ast::{Expr, ExprKind, Location, Operator};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum CallKind {
    Isinstance,
    Issubclass,
}

impl fmt::Display for CallKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CallKind::Isinstance => fmt.write_str("isinstance"),
            CallKind::Issubclass => fmt.write_str("issubclass"),
        }
    }
}

impl CallKind {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "isinstance" => Some(CallKind::Isinstance),
            "issubclass" => Some(CallKind::Issubclass),
            _ => None,
        }
    }
}

#[violation]
pub struct NonPEP604Isinstance {
    pub kind: CallKind,
}

impl AlwaysAutofixableViolation for NonPEP604Isinstance {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `X | Y` in `{}` call instead of `(X, Y)`", self.kind)
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

/// UP038
pub fn use_pep604_isinstance(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        let Some(kind) = CallKind::from_name(id) else {
            return;
        };
        if !checker.ctx.is_builtin(id) {
            return;
        };
        if let Some(types) = args.get(1) {
            if let ExprKind::Tuple { elts, .. } = &types.node {
                // Ex) `()`
                if elts.is_empty() {
                    return;
                }

                // Ex) `(*args,)`
                if elts
                    .iter()
                    .any(|elt| matches!(elt.node, ExprKind::Starred { .. }))
                {
                    return;
                }

                let mut diagnostic =
                    Diagnostic::new(NonPEP604Isinstance { kind }, Range::from(expr));
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.set_fix(Edit::replacement(
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
