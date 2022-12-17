use itertools::Itertools;
use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::checks::{Check, CheckKind};

/// B005
pub fn strip_with_multi_characters(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    let ExprKind::Attribute { attr, .. } = &func.node else {
        return;
    };
    if !matches!(attr.as_str(), "strip" | "lstrip" | "rstrip") {
        return;
    }
    if args.len() != 1 {
        return;
    }

    let ExprKind::Constant {
        value: Constant::Str(value),
        ..
    } = &args[0].node else {
        return;
    };

    if value.len() > 1 && value.chars().unique().count() != value.len() {
        checker.add_check(Check::new(
            CheckKind::StripWithMultiCharacters,
            Range::from_located(expr),
        ));
    }
}
