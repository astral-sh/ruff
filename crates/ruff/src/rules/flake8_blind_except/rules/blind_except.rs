use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::{find_keyword, is_const_true};
use ruff_python_semantic::analyze::logging;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `except` clauses that catch all exceptions.
///
/// ## Why is this bad?
/// Overly broad `except` clauses can lead to unexpected behavior, such as
/// catching `KeyboardInterrupt` or `SystemExit` exceptions that prevent the
/// user from exiting the program.
///
/// Instead of catching all exceptions, catch only those that are expected to
/// be raised in the `try` block.
///
/// ## Example
/// ```python
/// try:
///     foo()
/// except BaseException:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// try:
///     foo()
/// except FileNotFoundError:
///     ...
/// ```
///
/// ## References
/// - [Python documentation: The `try` statement](https://docs.python.org/3/reference/compound_stmts.html#the-try-statement)
/// - [Python documentation: Exception hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
#[violation]
pub struct BlindExcept {
    name: String,
}

impl Violation for BlindExcept {
    #[derive_message_formats]
    fn message(&self) -> String {
        let BlindExcept { name } = self;
        format!("Do not catch blind exception: `{name}`")
    }
}

/// BLE001
pub(crate) fn blind_except(
    checker: &mut Checker,
    type_: Option<&Expr>,
    name: Option<&str>,
    body: &[Stmt],
) {
    let Some(type_) = type_ else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = &type_ else {
        return;
    };
    for exception in ["BaseException", "Exception"] {
        if id == exception && checker.semantic().is_builtin(exception) {
            // If the exception is re-raised, don't flag an error.
            if body.iter().any(|stmt| {
                if let Stmt::Raise(ast::StmtRaise { exc, .. }) = stmt {
                    if let Some(exc) = exc {
                        if let Expr::Name(ast::ExprName { id, .. }) = exc.as_ref() {
                            name.map_or(false, |name| id == name)
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
                if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt {
                    if let Expr::Call(ast::ExprCall { func, keywords, .. }) = value.as_ref() {
                        if logging::is_logger_candidate(func, checker.semantic()) {
                            if let Some(attribute) = func.as_attribute_expr() {
                                let attr = attribute.attr.as_str();
                                if attr == "exception" {
                                    return true;
                                }
                                if attr == "error" {
                                    if let Some(keyword) = find_keyword(keywords, "exc_info") {
                                        if is_const_true(&keyword.value) {
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
                type_.range(),
            ));
        }
    }
}
