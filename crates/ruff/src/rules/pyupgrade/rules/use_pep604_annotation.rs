use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, ExprKind, Operator};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;
use ruff_python_semantic::analyze::typing::Pep604Operator;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct NonPEP604Annotation;

impl Violation for NonPEP604Annotation {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `X | Y` for type annotations")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Convert to `X | Y`".to_string())
    }
}

fn optional(expr: &Expr) -> Expr {
    Expr::new(
        TextRange::default(),
        ast::ExprBinOp {
            left: Box::new(expr.clone()),
            op: Operator::BitOr,
            right: Box::new(Expr::new(
                TextRange::default(),
                ast::ExprConstant {
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
            TextRange::default(),
            ast::ExprBinOp {
                left: Box::new(union(&elts[..elts.len() - 1])),
                op: Operator::BitOr,
                right: Box::new(elts[elts.len() - 1].clone()),
            },
        )
    }
}

/// UP007
pub(crate) fn use_pep604_annotation(
    checker: &mut Checker,
    expr: &Expr,
    slice: &Expr,
    operator: Pep604Operator,
) {
    // Avoid fixing forward references, or types not in an annotation.
    let fixable =
        checker.ctx.in_type_definition() && !checker.ctx.in_complex_string_type_definition();
    match operator {
        Pep604Operator::Optional => {
            let mut diagnostic = Diagnostic::new(NonPEP604Annotation, expr.range());
            if fixable && checker.patch(diagnostic.kind.rule()) {
                #[allow(deprecated)]
                diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                    unparse_expr(&optional(slice), checker.stylist),
                    expr.range(),
                )));
            }
            checker.diagnostics.push(diagnostic);
        }
        Pep604Operator::Union => {
            let mut diagnostic = Diagnostic::new(NonPEP604Annotation, expr.range());
            if fixable && checker.patch(diagnostic.kind.rule()) {
                match &slice.node {
                    ExprKind::Slice(_) => {
                        // Invalid type annotation.
                    }
                    ExprKind::Tuple(ast::ExprTuple { elts, .. }) => {
                        #[allow(deprecated)]
                        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                            unparse_expr(&union(elts), checker.stylist),
                            expr.range(),
                        )));
                    }
                    _ => {
                        // Single argument.
                        #[allow(deprecated)]
                        diagnostic.set_fix(Fix::unspecified(Edit::range_replacement(
                            unparse_expr(slice, checker.stylist),
                            expr.range(),
                        )));
                    }
                }
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
