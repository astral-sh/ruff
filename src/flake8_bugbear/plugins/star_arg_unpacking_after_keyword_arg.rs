use rustpython_ast::{Expr, ExprKind, Keyword};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::{Check, CheckKind};

/// B026
pub fn star_arg_unpacking_after_keyword_arg(
    checker: &mut Checker,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if let Some(keyword) = keywords.first() {
        for arg in args {
            if let ExprKind::Starred { .. } = arg.node {
                if arg.location > keyword.location {
                    checker.add_check(Check::new(
                        CheckKind::StarArgUnpackingAfterKeywordArg,
                        Range::from_located(arg),
                    ));
                }
            }
        }
    }
}
