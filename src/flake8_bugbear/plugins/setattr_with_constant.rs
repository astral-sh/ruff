use anyhow::Result;
use log::error;
use rustpython_ast::{Constant, Expr, ExprContext, ExprKind, Location, Stmt, StmtKind};

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::code_gen::SourceGenerator;
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;

fn assignment(obj: &Expr, name: &str, value: &Expr) -> Result<String> {
    let stmt = Stmt::new(
        Location::default(),
        Location::default(),
        StmtKind::Assign {
            targets: vec![Expr::new(
                Location::default(),
                Location::default(),
                ExprKind::Attribute {
                    value: Box::new(obj.clone()),
                    attr: name.to_string(),
                    ctx: ExprContext::Store,
                },
            )],
            value: Box::new(value.clone()),
            type_comment: None,
        },
    );
    let mut generator = SourceGenerator::new();
    generator.unparse_stmt(&stmt);
    generator.generate().map_err(std::convert::Into::into)
}

/// B010
pub fn setattr_with_constant(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "setattr" {
            if let [obj, name, value] = args {
                if let ExprKind::Constant {
                    value: Constant::Str(name),
                    ..
                } = &name.node
                {
                    if IDENTIFIER_REGEX.is_match(name) && !KWLIST.contains(&name.as_str()) {
                        let mut check =
                            Check::new(CheckKind::SetAttrWithConstant, Range::from_located(expr));
                        if checker.patch(check.kind.code()) {
                            match assignment(obj, name, value) {
                                Ok(content) => check.amend(Fix::replacement(
                                    content,
                                    expr.location,
                                    expr.end_location.unwrap(),
                                )),
                                Err(e) => error!("Failed to fix invalid comparison: {}", e),
                            };
                        }
                        checker.add_check(check);
                    }
                }
            }
        }
    }
}
