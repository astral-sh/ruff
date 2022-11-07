use itertools::Itertools;
use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B004
pub fn strip_with_multi_characters(checker: &mut Checker, expr: &Expr, func: &Expr, args: &[Expr]) {
    if let ExprKind::Attribute { attr, .. } = &func.node {
        if attr == "strip" || attr == "lstrip" || attr == "rstrip" {
            if args.len() == 1 {
                if let ExprKind::Constant {
                    value: Constant::Str(s),
                    ..
                } = &args[0].node
                {
                    if s.len() > 1 && s.chars().unique().count() != s.len() {
                        checker.add_check(Check::new(
                            CheckKind::StripWithMultiCharacters,
                            Range::from_located(expr),
                        ));
                    }
                }
            }
        }
    }
}
