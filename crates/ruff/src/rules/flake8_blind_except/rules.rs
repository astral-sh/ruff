use rustpython_parser::ast::{Expr, ExprKind, Stmt, StmtKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{find_keyword, is_const_true};
use ruff_python_ast::types::Range;
use ruff_python_semantic::analyze::logging;

use crate::checkers::ast::Checker;

#[violation]
pub struct BlindExcept {
    pub name: String,
}

impl Violation for BlindExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlindExcept { name } = self;
        format!("Do not catch blind exception: `{name}`")
    }
}

/// BLE001
pub fn blind_except(
    checker: &mut Checker,
    type_: Option<&Expr>,
    name: Option<&str>,
    body: &[Stmt],
) {
    let Some(type_) = type_ else {
        return;
    };
    let ExprKind::Name { id, .. } = &type_.node else {
        return;
    };
    for exception in ["BaseException", "Exception"] {
        if id == exception && checker.ctx.is_builtin(exception) {
            // If the exception is re-raised, don't flag an error.
            if body.iter().any(|stmt| {
                if let StmtKind::Raise { exc, .. } = &stmt.node {
                    if let Some(exc) = exc {
                        if let ExprKind::Name { id, .. } = &exc.node {
                            name.map_or(false, |name| name == id)
                        } else {
                            false
                        }
                    } else {
                        true
                    }
                } else {
                    false
                }
            }) {
                continue;
            }

            // If the exception is logged, don't flag an error.
            if body.iter().any(|stmt| {
                if let StmtKind::Expr { value } = &stmt.node {
                    if let ExprKind::Call { func, keywords, .. } = &value.node {
                        if logging::is_logger_candidate(&checker.ctx, func) {
                            if let ExprKind::Attribute { attr, .. } = &func.node {
                                if attr == "exception" {
                                    return true;
                                }
                                if attr == "error" {
                                    if let Some(keyword) = find_keyword(keywords, "exc_info") {
                                        if is_const_true(&keyword.node.value) {
                                            return true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                false
            }) {
                continue;
            }

            checker.diagnostics.push(Diagnostic::new(
                BlindExcept {
                    name: id.to_string(),
                },
                Range::from(type_),
            ));
        }
    }
}
