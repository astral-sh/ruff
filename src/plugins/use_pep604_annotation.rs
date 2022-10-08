use anyhow::{anyhow, Result};
use rustpython_ast::{Expr, ExprKind};

use crate::ast::helpers::match_name_or_attr;
use crate::ast::types::Range;
use crate::autofix::fixer;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind, Fix};
use crate::code_gen::SourceGenerator;

pub fn use_pep604_annotation(checker: &mut Checker, expr: &Expr, value: &Expr, slice: &Expr) {
    if match_name_or_attr(value, "Optional") {
        let mut check = Check::new(CheckKind::UsePEP604Annotation, Range::from_located(expr));
        if matches!(checker.autofix, fixer::Mode::Generate | fixer::Mode::Apply) {
            let mut generator = SourceGenerator::new();
            if let Ok(()) = generator.unparse_expr(slice, 0) {
                if let Ok(content) = generator.generate() {
                    check.amend(Fix {
                        content: format!("{} | None", content),
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
                    // Multiple arguments.
                    let parts: Result<Vec<String>> = elts
                        .iter()
                        .map(|expr| {
                            let mut generator = SourceGenerator::new();
                            generator
                                .unparse_expr(expr, 0)
                                .map_err(|_| anyhow!("Failed to parse."))?;
                            generator
                                .generate()
                                .map_err(|_| anyhow!("Failed to generate."))
                        })
                        .collect();
                    if let Ok(parts) = parts {
                        let content = parts.join(" | ");
                        check.amend(Fix {
                            content,
                            location: expr.location,
                            end_location: expr.end_location,
                            applied: false,
                        })
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
