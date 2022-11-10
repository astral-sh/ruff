use once_cell::sync::Lazy;
use regex::Regex;
use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::python::keyword::KWLIST;

static IDENTIFIER_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap());

/// B009
pub fn getattr_with_constant(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "getattr" {
            if let [_, arg] = args {
                if let ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } = &arg.node
                {
                    if IDENTIFIER_REGEX.is_match(value) && !KWLIST.contains(&value.as_str()) {
                        checker.add_check(Check::new(
                            CheckKind::GetAttrWithConstant,
                            Range::from_located(expr),
                        ));
                    }
                }
            }
        }
    }
}
