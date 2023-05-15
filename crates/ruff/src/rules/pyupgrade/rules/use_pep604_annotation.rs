use ruff_text_size::TextRange;
use rustpython_parser::ast::{self, Constant, Expr, Operator, Ranged};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::unparse_expr;

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
    Expr::BinOp(ast::ExprBinOp {
        left: Box::new(expr.clone()),
        op: Operator::BitOr,
        right: Box::new(Expr::Constant(ast::ExprConstant {
            value: Constant::None,
            kind: None,
            range: TextRange::default(),
        })),
        range: TextRange::default(),
    })
}

fn union(elts: &[Expr]) -> Expr {
    if elts.len() == 1 {
        elts[0].clone()
    } else {
        Expr::BinOp(ast::ExprBinOp {
            left: Box::new(union(&elts[..elts.len() - 1])),
            op: Operator::BitOr,
            right: Box::new(elts[elts.len() - 1].clone()),
            range: TextRange::default(),
        })
    }
}

/// Returns `true` if any argument in the slice is a string.
fn any_arg_is_str(slice: &Expr) -> bool {
    match slice {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(_),
            ..
        }) => true,
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts.iter().any(any_arg_is_str),
        _ => false,
    }
}

#[derive(Copy, Clone)]
enum TypingMember {
    Union,
    Optional,
}

/// UP007
pub(crate) fn use_pep604_annotation(
    checker: &mut Checker,
    expr: &Expr,
    value: &Expr,
    slice: &Expr,
) {
    // If any of the _arguments_ are forward references, we can't use PEP 604.
    // Ex) `Union["str", "int"]` can't be converted to `"str" | "int"`.
    if any_arg_is_str(slice) {
        return;
    }

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

    // Avoid fixing forward references, or types not in an annotation.
    let fixable =
        checker.ctx.in_type_definition() && !checker.ctx.in_complex_string_type_definition();

    match typing_member {
        TypingMember::Optional => {
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
        TypingMember::Union => {
            let mut diagnostic = Diagnostic::new(NonPEP604Annotation, expr.range());
            if fixable && checker.patch(diagnostic.kind.rule()) {
                match slice {
                    Expr::Slice(_) => {
                        // Invalid type annotation.
                    }
                    Expr::Tuple(ast::ExprTuple { elts, .. }) => {
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
