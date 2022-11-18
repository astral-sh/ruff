use rustpython_ast::{Constant, Expr, ExprContext, ExprKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;

fn attribute(value: &Expr, attr: &str) -> Expr {
    Expr::new(
        Default::default(),
        Default::default(),
        ExprKind::Attribute {
            value: Box::new(value.clone()),
            attr: attr.to_string(),
            ctx: ExprContext::Load,
        },
    )
}

/// B009
pub fn getattr_with_constant(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "getattr" {
            if let [obj, arg] = args {
                if let ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } = &arg.node
                {
                    if IDENTIFIER_REGEX.is_match(value) && !KWLIST.contains(&value.as_str()) {
                        let mut check =
                            Check::new(CheckKind::GetAttrWithConstant, Range::from_located(expr));
                        if checker.patch(check.kind.code()) {
                            let mut generator = SourceGenerator::new();
                            if let Ok(()) = generator.unparse_expr(&attribute(obj, value), 0) {
                                if let Ok(content) = generator.generate() {
                                    check.amend(Fix::replacement(
                                        content,
                                        expr.location,
                                        expr.end_location.unwrap(),
                                    ));
                                }
                            }
                        }
                        checker.add_check(check);
                    }
                }
            }
        }
    }
}
