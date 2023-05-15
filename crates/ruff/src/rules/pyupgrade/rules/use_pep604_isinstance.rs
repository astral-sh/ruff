use ruff_text_size::TextRange;
use std::fmt;

use rustpython_parser::ast::{self, Expr, ExprKind, Operator};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub(crate) enum CallKind {
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
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "isinstance" => Some(CallKind::Isinstance),
            "issubclass" => Some(CallKind::Issubclass),
            _ => None,
        }
    }
}

#[violation]
pub struct NonPEP604Isinstance {
    kind: CallKind,
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
            TextRange::default(),
            ast::ExprBinOp {
                left: Box::new(union(&elts[..elts.len() - 1])),
                op: Operator::BitOr,
                right: Box::new(elts[elts.len() - 1].clone()),
            },
        )
    }
}

/// UP038
pub(crate) fn use_pep604_isinstance(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    if let ExprKind::Name(ast::ExprName { id, .. }) = &func.node {
        let Some(kind) = CallKind::from_name(id) else {
            return;
        };
        if !checker.ctx.is_builtin(id) {
            return;
        };
        if let Some(types) = args.get(1) {
            if let ExprKind::Tuple(ast::ExprTuple { elts, .. }) = &types.node {
                // Ex) `()`
                if elts.is_empty() {
                    return;
                }

                // Ex) `(*args,)`
                if elts
                    .iter()
                    .any(|elt| matches!(elt.node, ExprKind::Starred(_)))
                {
                    return;
                }

                let mut diagnostic = Diagnostic::new(NonPEP604Isinstance { kind }, expr.range());
                if checker.patch(diagnostic.kind.rule()) {
                    #[allow(deprecated)]
                    diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                        unparse_expr(&union(elts), checker.stylist),
                        types.range(),
                    )));
                }
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
