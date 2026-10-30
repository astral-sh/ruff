use ruff_python_ast::{
    ExceptHandler, Stmt, StmtRaise,
    flat::{self, fields as ast},
};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::flat::statement_visitor::{StatementVisitor, walk_stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for needless exception names in `raise` statements.
///
/// ## Why is this bad?
/// It's redundant to specify the exception name in a `raise` statement if the
/// exception is being re-raised.
///
/// ## Example
/// ```python
/// def foo():
///     try:
///         ...
///     except ValueError as exc:
///         raise exc
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     try:
///         ...
///     except ValueError:
///         raise
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it doesn't properly handle bound
/// exceptions that are shadowed between the `except` and `raise` statements.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.231", category = Category::Complexity)]
pub(crate) struct VerboseRaise;

impl AlwaysFixableViolation for VerboseRaise {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `raise` without specifying exception name".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove exception name".to_string()
    }
}

/// TRY201
pub(crate) fn verbose_raise(checker: &Checker, handlers: flat::NodeList<'_, ExceptHandler>) {
    for handler in handlers {
        // If the handler assigned a name to the exception...
        if let ast::ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            name: Some(exception_name),
            body,
            ..
        }) = handler.fields()
        {
            let raises = {
                let mut visitor = RaiseStatementVisitor::default();
                visitor.visit_body(body);
                visitor.raises
            };
            for raise in raises {
                if raise.cause().is_some() {
                    continue;
                }
                if let Some(exc) = raise.exc() {
                    // ...and the raised object is bound to the same name...
                    if let flat::ExprKind::Name(name_node) = exc.classify() {
                        let id = name_node.id();
                        if id == exception_name.as_str() {
                            let mut diagnostic =
                                checker.report_diagnostic(VerboseRaise, exc.range());
                            diagnostic.set_fix(Fix::unsafe_edit(Edit::range_replacement(
                                "raise".to_string(),
                                raise.range(),
                            )));
                        }
                    }
                }
            }
        }
    }
}

#[derive(Default)]
struct RaiseStatementVisitor<'a> {
    raises: Vec<flat::View<'a, StmtRaise>>,
}

impl<'a> StatementVisitor<'a> for RaiseStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: flat::View<'a, Stmt>) {
        match stmt.classify() {
            flat::StmtKind::Raise(raise) => {
                self.raises.push(raise);
            }
            flat::StmtKind::Try(stmt_try) => {
                let body = stmt_try.body();
                let finalbody = stmt_try.finalbody();

                for stmt in body.iter().chain(finalbody) {
                    walk_stmt(self, stmt);
                }
            }
            _ => walk_stmt(self, stmt),
        }
    }
}
