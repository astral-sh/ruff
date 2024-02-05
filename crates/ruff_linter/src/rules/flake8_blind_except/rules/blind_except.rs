use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::logging;
use ruff_text_size::Ranged;

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
/// Exceptions that are re-raised will _not_ be flagged, as they're expected to
/// be caught elsewhere:
/// ```python
/// try:
///     foo()
/// except BaseException:
///     raise
/// ```
///
/// Exceptions that are logged via `logging.exception()` or `logging.error()`
/// with `exc_info` enabled will _not_ be flagged, as this is a common pattern
/// for propagating exception traces:
/// ```python
/// try:
///     foo()
/// except BaseException:
///     logging.exception("Something went wrong")
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

    if !matches!(id.as_str(), "BaseException" | "Exception") {
        return;
    }

    if !checker.semantic().is_builtin(id) {
        return;
    }

    // If the exception is re-raised, don't flag an error.
    if body.iter().any(|stmt| {
        if let Stmt::Raise(ast::StmtRaise { exc, .. }) = stmt {
            if let Some(exc) = exc {
                if let Expr::Name(ast::ExprName { id, .. }) = exc.as_ref() {
                    name.is_some_and(|name| id == name)
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
        return;
    }

    // If the exception is logged, don't flag an error.
    if body.iter().any(|stmt| {
        if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt {
            if let Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) = value.as_ref()
            {
                match func.as_ref() {
                    Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                        if logging::is_logger_candidate(
                            func,
                            checker.semantic(),
                            &checker.settings.logger_objects,
                        ) {
                            match attr.as_str() {
                                "exception" => return true,
                                "error" => {
                                    if let Some(keyword) = arguments.find_keyword("exc_info") {
                                        if is_const_true(&keyword.value) {
                                            return true;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Expr::Name(ast::ExprName { .. }) => {
                        if checker
                            .semantic()
                            .resolve_call_path(func.as_ref())
                            .is_some_and(|call_path| match call_path.as_slice() {
                                ["logging", "exception"] => true,
                                ["logging", "error"] => {
                                    if let Some(keyword) = arguments.find_keyword("exc_info") {
                                        if is_const_true(&keyword.value) {
                                            return true;
                                        }
                                    }
                                    false
                                }
                                _ => false,
                            })
                        {
                            return true;
                        }
                    }
                    _ => {
                        return false;
                    }
                }
            }
        }
        false
    }) {
        return;
    }

    checker.diagnostics.push(Diagnostic::new(
        BlindExcept {
            name: id.to_string(),
        },
        type_.range(),
    ));
}
