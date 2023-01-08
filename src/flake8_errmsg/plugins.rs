use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::registry::{Diagnostic, RuleCode};
use crate::violations;
use crate::xxxxxxxxs::ast::xxxxxxxx;

/// EM101, EM102, EM103
pub fn string_in_exception(xxxxxxxx: &mut xxxxxxxx, exc: &Expr) {
    if let ExprKind::Call { args, .. } = &exc.node {
        if let Some(first) = args.first() {
            match &first.node {
                // Check for string literals
                ExprKind::Constant {
                    value: Constant::Str(string),
                    ..
                } => {
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::EM101) {
                        if string.len() > xxxxxxxx.settings.flake8_errmsg.max_string_length {
                            xxxxxxxx.diagnostics.push(Diagnostic::new(
                                violations::RawStringInException,
                                Range::from_located(first),
                            ));
                        }
                    }
                }
                // Check for f-strings
                ExprKind::JoinedStr { .. } => {
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::EM102) {
                        xxxxxxxx.diagnostics.push(Diagnostic::new(
                            violations::FStringInException,
                            Range::from_located(first),
                        ));
                    }
                }
                // Check for .format() calls
                ExprKind::Call { func, .. } => {
                    if xxxxxxxx.settings.enabled.contains(&RuleCode::EM103) {
                        if let ExprKind::Attribute { value, attr, .. } = &func.node {
                            if attr == "format" && matches!(value.node, ExprKind::Constant { .. }) {
                                xxxxxxxx.diagnostics.push(Diagnostic::new(
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
