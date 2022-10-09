use rustpython_ast::{Constant, Expr, ExprKind, Operator};

use crate::ast::helpers::match_name_or_attr;
use crate::ast::types::Range;
use crate::autofix::fixer;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind, Fix};
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

pub fn use_pep604_annotation(checker: &mut Checker, expr: &Expr, value: &Expr, slice: &Expr) {
    if match_name_or_attr(value, "Optional") {
        let mut check = Check::new(CheckKind::UsePEP604Annotation, Range::from_located(expr));
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            let mut generator = SourceGenerator::new();
            if let Ok(()) = generator.unparse_expr(&optional(slice), 0) {
                if let Ok(content) = generator.generate() {
                    check.amend(Fix {
                        content,
                        location: expr.location,
                        end_location: expr.end_location,
                        applied: false,
                    })
                }
            }
        }
        checker.add_check(check);
    } else if match_name_or_attr(value, "Union") {
        let mut check = Check::new(CheckKind::UsePEP604Annotation, Range::from_located(expr));
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            match &slice.node {
                ExprKind::Slice { .. } => {
                    // Invalid type annotation.
                }
                ExprKind::Tuple { elts, .. } => {
                    let mut generator = SourceGenerator::new();
                    if let Ok(()) = generator.unparse_expr(&union(elts), 0) {
                        if let Ok(content) = generator.generate() {
                            check.amend(Fix {
                                content,
                                location: expr.location,
                                end_location: expr.end_location,
                                applied: false,
                            })
                        }
                    }
                }
                _ => {
                    // Single argument.
                    let mut generator = SourceGenerator::new();
                    if let Ok(()) = generator.unparse_expr(slice, 0) {
                        if let Ok(content) = generator.generate() {
                            check.amend(Fix {
                                content,
                                location: expr.location,
                                end_location: expr.end_location,
                                applied: false,
                            });
                        }
                    }
                }
            }
        }
        checker.add_check(check);
    }
}
