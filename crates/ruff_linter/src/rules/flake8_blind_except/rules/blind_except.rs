use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::is_const_true;
use ruff_python_ast::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::logging;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `except` clauses that catch all exceptions.  This includes
/// `except BaseException` and `except Exception`.
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
/// Exceptions that are logged via `logging.exception()` or are logged via
/// `logging.error()` or `logging.critical()` with `exc_info` enabled will
/// _not_ be flagged, as this is a common pattern for propagating exception
/// traces:
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
#[violation_metadata(stable_since = "v0.0.127")]
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

fn contains_blind_exception<'a>(
    semantic: &'a SemanticModel,
    expr: &'a Expr,
) -> Option<(&'a str, ruff_text_size::TextRange)> {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. }) => elts
            .iter()
            .find_map(|elt| contains_blind_exception(semantic, elt)),
        _ => {
            let builtin_exception_type = semantic.resolve_builtin_symbol(expr)?;
            matches!(builtin_exception_type, "BaseException" | "Exception")
                .then(|| (builtin_exception_type, expr.range()))
        }
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
    let Some((builtin_exception_type, range)) = contains_blind_exception(semantic, type_) else {
        return;
    };

    // If the exception is re-raised, don't flag an error.
    let mut visitor = ReraiseVisitor::new(name);
    visitor.visit_body(body);
    if visitor.seen() {
        return;
    }

    // If the exception is logged, don't flag an error.
    let mut visitor = LogExceptionVisitor::new(semantic, &checker.settings().logger_objects);
    visitor.visit_body(body);
    if visitor.seen() {
        return;
    }

    checker.report_diagnostic(
        BlindExcept {
            name: builtin_exception_type.into(),
        },
        range,
    );
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
        if self.seen {
            return;
        }
        match stmt {
            Stmt::Raise(ast::StmtRaise { exc, cause, .. }) => {
                // except Exception [as <name>]:
                //     raise [<exc> [from <cause>]]
                let reraised = match (self.name, exc.as_deref(), cause.as_deref()) {
                    // `raise`
                    (_, None, None) => true,
                    // `raise SomeExc from <name>`
                    (Some(name), _, Some(Expr::Name(ast::ExprName { id, .. }))) if name == id => {
                        true
                    }
                    // `raise <name>` and `raise <name> from SomeCause`
                    (Some(name), Some(Expr::Name(ast::ExprName { id, .. })), _) if name == id => {
                        true
                    }
                    _ => false,
                };
                if reraised {
                    self.seen = true;
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
        if self.seen {
            return;
        }
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
                                    "error" | "critical" => arguments
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
                                    ["logging", "error" | "critical"] => arguments
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
