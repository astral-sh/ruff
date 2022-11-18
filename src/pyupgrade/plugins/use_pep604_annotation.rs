use rustpython_ast::{Constant, Expr, ExprKind, Operator};

use crate::ast::helpers::{collect_call_paths, dealias_call_path};
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;

fn optional(expr: &Expr) -> Expr {
    Expr::new(
        Default::default(),
        Default::default(),
        ExprKind::BinOp {
            left: Box::new(expr.clone()),
            op: Operator::BitOr,
            right: Box::new(Expr::new(
                Default::default(),
                Default::default(),
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
            Default::default(),
            Default::default(),
            ExprKind::BinOp {
                left: Box::new(union(&elts[..elts.len() - 1])),
                op: Operator::BitOr,
                right: Box::new(elts[elts.len() - 1].clone()),
            },
        )
    }
}

/// U007
pub fn use_pep604_annotation(checker: &mut Checker, expr: &Expr, value: &Expr, slice: &Expr) {
    let call_path = dealias_call_path(collect_call_paths(value), &checker.import_aliases);
    if checker.match_typing_call_path(&call_path, "Optional") {
        let mut check = Check::new(CheckKind::UsePEP604Annotation, Range::from_located(expr));
        if checker.patch(check.kind.code()) {
            let mut generator = SourceGenerator::new();
            if let Ok(()) = generator.unparse_expr(&optional(slice), 0) {
                if let Ok(content) = generator.generate() {
                    check.amend(Fix::replacement(
                        content,
                        expr.location,
                        expr.end_location.unwrap(),
                    ))
                }
            }
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
                    let mut generator = SourceGenerator::new();
                    if let Ok(()) = generator.unparse_expr(&union(elts), 0) {
                        if let Ok(content) = generator.generate() {
                            check.amend(Fix::replacement(
                                content,
                                expr.location,
                                expr.end_location.unwrap(),
                            ))
                        }
                    }
                }
                _ => {
                    // Single argument.
                    let mut generator = SourceGenerator::new();
                    if let Ok(()) = generator.unparse_expr(slice, 0) {
                        if let Ok(content) = generator.generate() {
                            check.amend(Fix::replacement(
                                content,
                                expr.location,
                                expr.end_location.unwrap(),
                            ));
                        }
                    }
                }
            }
        }
        checker.add_check(check);
    }
}
