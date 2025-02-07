use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::statement_visitor::{walk_stmt, StatementVisitor};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::analyze::logging;
use ruff_python_semantic::SemanticModel;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `except` clauses that catch all exceptions.  This includes
/// bare `except`, `except BaseException` and `except Exception`.
///
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
/// - [PEP 8: Programming Recommendations on bare `except`](https://peps.python.org/pep-0008/#programming-recommendations)
#[derive(ViolationMetadata)]
pub(crate) struct BlindExcept {
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
    checker: &Checker,
    type_: Option<&Expr>,
    name: Option<&str>,
    body: &[Stmt],
) {
    let Some(type_) = type_ else {
        return;
    };

    let semantic = checker.semantic();
    let Some(builtin_exception_type) = semantic.resolve_builtin_symbol(type_) else {
        return;
    };
    if !matches!(builtin_exception_type, "BaseException" | "Exception") {
        return;
    }

    // If the exception is re-raised, don't flag an error.
    let mut visitor = ReraiseVisitor::new(name);
    visitor.visit_body(body);
    if visitor.seen() {
        return;
    }

    // If the exception is logged, don't flag an error.
    let mut visitor = LogExceptionVisitor::new(semantic, &checker.settings.logger_objects);
    visitor.visit_body(body);
    if visitor.seen() {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(
        BlindExcept {
            name: builtin_exception_type.to_string(),
        },
        type_.range(),
    ));
}

/// A visitor to detect whether the exception with the given name was re-raised.
struct ReraiseVisitor<'a> {
    name: Option<&'a str>,
    seen: bool,
}

impl<'a> ReraiseVisitor<'a> {
    /// Create a new [`ReraiseVisitor`] with the given exception name.
    fn new(name: Option<&'a str>) -> Self {
        Self { name, seen: false }
    }

    /// Returns `true` if the exception was re-raised.
    fn seen(&self) -> bool {
        self.seen
    }
}

impl<'a> StatementVisitor<'a> for ReraiseVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Raise(ast::StmtRaise { exc, cause, .. }) => {
                if let Some(cause) = cause {
                    if let Expr::Name(ast::ExprName { id, .. }) = cause.as_ref() {
                        if self.name.is_some_and(|name| id == name) {
                            self.seen = true;
                        }
                    }
                } else {
                    if let Some(exc) = exc {
                        if let Expr::Name(ast::ExprName { id, .. }) = exc.as_ref() {
                            if self.name.is_some_and(|name| id == name) {
                                self.seen = true;
                            }
                        }
                    } else {
                        self.seen = true;
                    }
                }
            }
            Stmt::Try(_) | Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, stmt),
        }
    }
}

/// A visitor to detect whether the exception was logged.
struct LogExceptionVisitor<'a> {
    semantic: &'a SemanticModel<'a>,
    logger_objects: &'a [String],
    seen: bool,
}

impl<'a> LogExceptionVisitor<'a> {
    /// Create a new [`LogExceptionVisitor`] with the given exception name.
    fn new(semantic: &'a SemanticModel<'a>, logger_objects: &'a [String]) -> Self {
        Self {
            semantic,
            logger_objects,
            seen: false,
        }
    }

    /// Returns `true` if the exception was logged.
    fn seen(&self) -> bool {
        self.seen
    }
}

impl<'a> StatementVisitor<'a> for LogExceptionVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Expr(ast::StmtExpr { value, .. }) => {
                if let Expr::Call(ast::ExprCall {
                    func, arguments, ..
                }) = value.as_ref()
                {
                    match func.as_ref() {
                        Expr::Attribute(ast::ExprAttribute { attr, .. }) => {
                            if logging::is_logger_candidate(
                                func,
                                self.semantic,
                                self.logger_objects,
                            ) {
                                if match attr.as_str() {
                                    "exception" => true,
                                    "error" => arguments
                                        .find_keyword("exc_info")
                                        .is_some_and(|keyword| is_const_true(&keyword.value)),
                                    _ => false,
                                } {
                                    self.seen = true;
                                }
                            }
                        }
                        Expr::Name(ast::ExprName { .. }) => {
                            if self.semantic.resolve_qualified_name(func).is_some_and(
                                |qualified_name| match qualified_name.segments() {
                                    ["logging", "exception"] => true,
                                    ["logging", "error"] => arguments
                                        .find_keyword("exc_info")
                                        .is_some_and(|keyword| is_const_true(&keyword.value)),
                                    _ => false,
                                },
                            ) {
                                self.seen = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            Stmt::Try(_) | Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {}
            _ => walk_stmt(self, stmt),
        }
    }
}
