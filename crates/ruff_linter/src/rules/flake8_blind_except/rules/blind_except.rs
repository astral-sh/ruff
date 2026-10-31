use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_python_ast::flat::{self, fields as ast};
use ruff_python_ast::helpers::Truthiness;
use ruff_python_ast::{Arguments, Expr, Stmt};
use ruff_python_semantic::SemanticModel;
use ruff_python_semantic::analyze::logging;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::rules::flake8_logging::helpers::is_logger_method_name;

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
/// Exceptions that are logged with `exc_info` enabled will
/// _not_ be flagged, as this is a common pattern for propagating exception
/// traces:
/// ```python
/// try:
///     foo()
/// except BaseException:
///     logging.exception("Something went wrong")
/// ```
///
/// ## Options
///
/// - `lint.logger-objects`
///
/// ## References
/// - [Python documentation: The `try` statement](https://docs.python.org/3/reference/compound_stmts.html#the-try-statement)
/// - [Python documentation: Exception hierarchy](https://docs.python.org/3/library/exceptions.html#exception-hierarchy)
/// - [PEP 8: Programming Recommendations on bare `except`](https://peps.python.org/pep-0008/#programming-recommendations)
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.127", category = Category::Suspicious)]
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
    expr: flat::View<'a, Expr>,
) -> Option<(&'a str, ruff_text_size::TextRange)> {
    match expr.classify() {
        flat::ExprKind::Tuple(expr_tuple) => {
            let elts = expr_tuple.elts();
            elts.iter()
                .find_map(|elt| contains_blind_exception(semantic, elt))
        }
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
    type_: Option<flat::View<'_, Expr>>,
    name: Option<&str>,
    body: flat::Suite<'_>,
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
    fn visit_stmt(&mut self, stmt: flat::View<'a, Stmt>) {
        if self.seen {
            return;
        }
        match stmt.classify() {
            flat::StmtKind::Raise(stmt_raise) => {
                let exc = stmt_raise.exc();
                let cause = stmt_raise.cause();

                // except Exception [as <name>]:
                //     raise [<exc> [from <cause>]]
                let reraised = match (
                    self.name,
                    exc.map(flat::View::<Expr>::fields),
                    cause.map(flat::View::<Expr>::fields),
                ) {
                    // `raise`
                    (_, None, None) => true,
                    // `raise SomeExc from <name>`
                    (Some(name), _, Some(ast::Expr::Name(ast::ExprName { id, .. })))
                        if name == id =>
                    {
                        true
                    }
                    // `raise <name>` and `raise <name> from SomeCause`
                    (Some(name), Some(ast::Expr::Name(ast::ExprName { id, .. })), _)
                        if name == id =>
                    {
                        true
                    }
                    _ => false,
                };
                if reraised {
                    self.seen = true;
                }
            }
            flat::StmtKind::Try(_)
            | flat::StmtKind::FunctionDef(_)
            | flat::StmtKind::ClassDef(_) => {}
            _ => walk_stmt(self, stmt),
        }
    }
}

/// Returns `true` if the `exc_info` keyword argument is truthy.
fn is_exc_info_enabled(arguments: flat::View<'_, Arguments>, semantic: &SemanticModel) -> bool {
    arguments.find_keyword("exc_info").is_some_and(|keyword| {
        Truthiness::from_expr(keyword.value(), |id| semantic.has_builtin_binding(id)).into_bool()
            != Some(false)
    })
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
    fn visit_stmt(&mut self, stmt: flat::View<'a, Stmt>) {
        if self.seen {
            return;
        }
        match stmt.classify() {
            flat::StmtKind::Expr(stmt_expr) => {
                let value = stmt_expr.value();

                if let flat::ExprKind::Call(call) = value.classify() {
                    let func = call.func();
                    let arguments = call.arguments();
                    match func.classify() {
                        flat::ExprKind::Attribute(expr_attribute)
                            if logging::is_logger_candidate(
                                func,
                                self.semantic,
                                self.logger_objects,
                            ) && match expr_attribute.attr().as_str() {
                                "exception" => true,
                                _ if is_logger_method_name(expr_attribute.attr().as_str()) => {
                                    is_exc_info_enabled(arguments, self.semantic)
                                }
                                _ => false,
                            } =>
                        {
                            self.seen = true;
                        }
                        flat::ExprKind::Name(_)
                            if self.semantic.resolve_qualified_name(func).is_some_and(
                                |qualified_name| match qualified_name.segments() {
                                    ["logging", "exception"] => true,
                                    ["logging", method] if is_logger_method_name(method) => {
                                        is_exc_info_enabled(arguments, self.semantic)
                                    }
                                    _ => false,
                                },
                            ) =>
                        {
                            self.seen = true;
                        }
                        _ => {}
                    }
                }
            }
            flat::StmtKind::Try(_)
            | flat::StmtKind::FunctionDef(_)
            | flat::StmtKind::ClassDef(_) => {}
            _ => walk_stmt(self, stmt),
        }
    }
}
