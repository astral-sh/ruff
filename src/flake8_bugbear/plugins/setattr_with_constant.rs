use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};
use crate::python::identifiers::IDENTIFIER_REGEX;
use crate::python::keyword::KWLIST;

/// B010
pub fn setattr_with_constant(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Name { id, .. } = &func.node {
        if id == "setattr" {
            if let [_, arg, _] = args {
                if let ExprKind::Constant {
                    value: Constant::Str(value),
                    ..
                } = &arg.node
                {
                    if IDENTIFIER_REGEX.is_match(value) && !KWLIST.contains(&value.as_str()) {
                        checker.add_check(Check::new(
                            CheckKind::SetAttrWithConstant,
                            Range::from_located(expr),
                        ));
                    }
                }
            }
        }
    }
}
