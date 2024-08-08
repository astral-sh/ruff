use ruff_python_ast::{self as ast, ExceptHandler, Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::RaiseStatementVisitor;
use ruff_python_ast::statement_visitor::StatementVisitor;
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for exception with added note not reraised.
///
/// ## Why is this bad?
/// The `.add_note' function does not use the exception.
/// It will implicitly be equivalent to passing the exception.
///
///
/// ## Example
/// ```python
/// try:
/// ...
/// except Exception as e:
///   e.add_note("...")
/// ```
///
/// Use instead:
/// ```python
/// try:
/// ...
/// except Exception as e:
///   e.add_note("...")
///   raise e
/// ```
///
/// ## References
/// - [Python documentation: `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#the-raise-statement)
///
#[violation]
pub struct SuppressedException;

impl Violation for SuppressedException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Caught exception with call to `add_note` not used. Did you forget to `raise` it?")
    }
}

#[derive(Debug, Clone)]
struct AddNote<'a> {
    receiver: &'a ast::ExprName,
}

fn match_str_expr(expr: &Expr) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };

    let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() else {
        return false;
    };

    if *id == "str" {
        return true;
    }
    false
}
fn match_add_note(stmt: &Stmt) -> Option<AddNote> {
    let Stmt::Expr(ast::StmtExpr { value, .. }) = stmt else {
        return None;
    };

    let Expr::Call(ast::ExprCall { func, .. }) = value.as_ref() else {
        return None;
    };

    let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() else {
        return None;
    };

    if attr != "add_note" {
        return None;
    }

    let Expr::Name(receiver @ ast::ExprName { .. }) = value.as_ref() else {
        return None;
    };

    Some(AddNote { receiver })
}

// B040
pub(crate) fn suppressed_exception(checker: &mut Checker, handlers: &[ExceptHandler]) {
    let mut count_str_references: usize = 0;
    for handler in handlers {
        let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { body, name, .. }) =
            handler;

        let raises = {
            let mut visitor = RaiseStatementVisitor::default();
            visitor.visit_body(body);
            visitor.raises
        };

        let mut exception_raised = false;
        if let Some(exception_name) = name {
            // Check if the exception was used in the raise or in the cause. This is also true for a bare raise, as the exception is used.
            for (.., exc, cause) in raises {
                if let Some(exc) = exc {
                    if !exc
                        .as_name_expr()
                        .is_some_and(|ast::ExprName { id, .. }| exception_name.id == *id)
                    {
                        exception_raised = false;
                    }

                    if let Some(cause) = cause {
                        if cause
                            .as_name_expr()
                            .is_some_and(|ast::ExprName { id, .. }| exception_name.id == *id)
                        {
                            exception_raised = true;
                        }
                    }
                } else {
                    exception_raised = true;
                }
            }

            let semantic = checker.semantic();
            let bindings: Vec<&Binding> = semantic
                .current_scope()
                .get_all(exception_name.id.as_str())
                .map(|binding_id| semantic.binding(binding_id))
                .collect();

            let Some(binding) = bindings.first() else {
                return;
            };

            for reference_id in binding.references() {
                let reference = checker.semantic().reference(reference_id);
                if let Some(node_id) = reference.expression_id() {
                    if let Some(expr) = semantic.parent_expression(node_id) {
                        if match_str_expr(expr) {
                            count_str_references += 1;
                        }
                    }
                }
            }

            let count_references = binding.references().count();
            let count_add_note = body
                .iter()
                .filter_map(match_add_note)
                .filter(|add_note| add_note.receiver.id == exception_name.id)
                .count();

            if count_add_note > 0 && !exception_raised {
                if count_references - count_str_references == count_add_note
                    || count_references == 0
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SuppressedException, exception_name.range()));
                }
            } else {
                return;
            }
        } else {
            return;
        }
    }
}
