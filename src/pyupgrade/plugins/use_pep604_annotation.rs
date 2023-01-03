use log::error;
use rustpython_ast::{Constant, Expr, ExprKind, Location, Operator};

use crate::ast::helpers::{collect_call_paths, dealias_call_path};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckKind};
use crate::source_code_generator::SourceCodeGenerator;

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

/// UP007
pub fn use_pep604_annotation(checker: &mut Checker, expr: &Expr, value: &Expr, slice: &Expr) {
    // Avoid rewriting forward annotations.
    if any_arg_is_str(slice) {
        return;
    }

    let call_path = dealias_call_path(collect_call_paths(value), &checker.import_aliases);
    if checker.match_typing_call_path(&call_path, "Optional") {
        let mut check = Check::new(CheckKind::UsePEP604Annotation, Range::from_located(expr));
        if checker.patch(check.kind.code()) {
            let mut generator = SourceCodeGenerator::new(
                checker.style.indentation(),
                checker.style.quote(),
                checker.style.line_ending(),
            );
            generator.unparse_expr(&optional(slice), 0);
            match generator.generate() {
                Ok(content) => {
                    check.amend(Fix::replacement(
                        content,
                        expr.location,
                        expr.end_location.unwrap(),
                    ));
                }
                Err(e) => error!("Failed to rewrite PEP604 annotation: {e}"),
            };
        }
        checker.add_check(check);
    } else if checker.match_typing_call_path(&call_path, "Union") {
        let mut check = Check::new(CheckKind::UsePEP604Annotation, Range::from_located(expr));
        if checker.patch(check.kind.code()) {
            match &slice.node {
                ExprKind::Slice { .. } => {
                    // Invalid type annotation.
                }
                ExprKind::Tuple { elts, .. } => {
                    let mut generator = SourceCodeGenerator::new(
                        checker.style.indentation(),
                        checker.style.quote(),
                        checker.style.line_ending(),
                    );
                    generator.unparse_expr(&union(elts), 0);
                    match generator.generate() {
                        Ok(content) => {
                            check.amend(Fix::replacement(
                                content,
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        Err(e) => error!("Failed to rewrite PEP604 annotation: {e}"),
                    }
                }
                _ => {
                    // Single argument.
                    let mut generator = SourceCodeGenerator::new(
                        checker.style.indentation(),
                        checker.style.quote(),
                        checker.style.line_ending(),
                    );
                    generator.unparse_expr(slice, 0);
                    match generator.generate() {
                        Ok(content) => {
                            check.amend(Fix::replacement(
                                content,
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                        Err(e) => error!("Failed to rewrite PEP604 annotation: {e}"),
                    }
                }
            }
        }
        checker.add_check(check);
    }
}
