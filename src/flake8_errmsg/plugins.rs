use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;

/// EM101, EM102, EM103
pub fn string_in_exception(checker: &mut Checker, exc: &Expr) {
    if let ExprKind::Call { args, .. } = &exc.node {
        if let Some(first) = args.first() {
            match &first.node {
                // Check for string literals
                ExprKind::Constant {
                    value: Constant::Str(string),
                    ..
                } => {
                    if checker.settings.enabled.contains(&RuleCode::EM101) {
                        if string.len() > checker.settings.flake8_errmsg.max_string_length {
                            checker.diagnostics.push(Diagnostic::new(
                                violations::RawStringInException,
                                Range::from_located(first),
                            ));
                        }
                    }
                }
                // Check for f-strings
                ExprKind::JoinedStr { .. } => {
                    if checker.settings.enabled.contains(&RuleCode::EM102) {
                        checker.diagnostics.push(Diagnostic::new(
                            violations::FStringInException,
                            Range::from_located(first),
                        ));
                    }
                }
                // Check for .format() calls
                ExprKind::Call { func, .. } => {
                    if checker.settings.enabled.contains(&RuleCode::EM103) {
                        if let ExprKind::Attribute { value, attr, .. } = &func.node {
                            if attr == "format" && matches!(value.node, ExprKind::Constant { .. }) {
                                checker.diagnostics.push(Diagnostic::new(
                                    violations::DotFormatInException,
                                    Range::from_located(first),
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
