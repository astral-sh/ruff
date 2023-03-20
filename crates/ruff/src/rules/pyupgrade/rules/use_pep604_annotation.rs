use rustpython_parser::ast::{Constant, Expr, ExprKind, Location, Operator};

use ruff_diagnostics::{AutofixKind, Availability, Diagnostic, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NonPEP604Annotation {
    pub fixable: bool,
}

impl Violation for NonPEP604Annotation {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `X | Y` for type annotations")
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        self.fixable.then_some(|_| format!("Convert to `X | Y`"))
    }
}

fn optional(expr: &Expr) -> Expr {
    Expr::new(
        Location::default(),
        Location::default(),
        ExprKind::BinOp {
            left: Box::new(expr.clone()),
            op: Operator::BitOr,
            right: Box::new(Expr::new(
                Location::default(),
                Location::default(),
                ExprKind::Constant {
                    value: Constant::None,
                    kind: None,
                },
            )),
        },
    )
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

/// Returns `true` if any argument in the slice is a string.
fn any_arg_is_str(slice: &Expr) -> bool {
    match &slice.node {
        ExprKind::Constant {
            value: Constant::Str(_),
            ..
        } => true,
        ExprKind::Tuple { elts, .. } => elts.iter().any(any_arg_is_str),
        _ => false,
    }
}

enum TypingMember {
    Union,
    Optional,
}

/// UP007
pub fn use_pep604_annotation(checker: &mut Checker, expr: &Expr, value: &Expr, slice: &Expr) {
    let Some(typing_member) = checker.ctx.resolve_call_path(value).as_ref().and_then(|call_path| {
        if checker.ctx.match_typing_call_path(call_path, "Optional") {
            Some(TypingMember::Optional)
        } else if checker.ctx.match_typing_call_path(call_path, "Union") {
            Some(TypingMember::Union)
        } else {
            None
        }
    }) else {
        return;
    };

    // Avoid fixing forward annotations.
    let fixable = !checker.ctx.in_deferred_string_type_definition && !any_arg_is_str(slice);

    match typing_member {
        TypingMember::Optional => {
            let mut diagnostic =
                Diagnostic::new(NonPEP604Annotation { fixable }, Range::from(expr));
            if fixable && checker.patch(diagnostic.kind.rule()) {
                diagnostic.amend(Fix::replacement(
                    unparse_expr(&optional(slice), checker.stylist),
                    expr.location,
                    expr.end_location.unwrap(),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
        TypingMember::Union => {
            let mut diagnostic =
                Diagnostic::new(NonPEP604Annotation { fixable }, Range::from(expr));
            if fixable && checker.patch(diagnostic.kind.rule()) {
                match &slice.node {
                    ExprKind::Slice { .. } => {
                        // Invalid type annotation.
                    }
                    ExprKind::Tuple { elts, .. } => {
                        diagnostic.amend(Fix::replacement(
                            unparse_expr(&union(elts), checker.stylist),
                            expr.location,
                            expr.end_location.unwrap(),
                        ));
                    }
                    _ => {
                        // Single argument.
                        diagnostic.amend(Fix::replacement(
                            unparse_expr(slice, checker.stylist),
                            expr.location,
                            expr.end_location.unwrap(),
                        ));
                    }
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
