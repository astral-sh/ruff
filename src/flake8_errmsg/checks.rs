use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checks::{Check, CheckKind};

pub fn check_string_in_exception(exc: &Expr, max_string_length: usize) -> Vec<Check> {
    let mut checks = vec![];

    if let ExprKind::Call { args, .. } = &exc.node {
        if let Some(first) = args.first() {
            match &first.node {
                // Check for string literals
                ExprKind::Constant {
                    value: Constant::Str(string),
                    ..
                } => {
                    if string.len() > max_string_length {
                        checks.push(Check::new(
                            CheckKind::RawStringInException,
                            Range::from_located(first),
                        ));
                    }
                }
                // Check for f-strings
                ExprKind::JoinedStr { .. } => checks.push(Check::new(
                    CheckKind::FStringInException,
                    Range::from_located(first),
                )),
                // Check for .format() calls
                ExprKind::Call { func, .. } => {
                    if let ExprKind::Attribute { value, attr, .. } = &func.node {
                        if attr == "format" && matches!(value.node, ExprKind::Constant { .. }) {
                            checks.push(Check::new(
                                CheckKind::DotFormatInException,
                                Range::from_located(first),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    checks
}
