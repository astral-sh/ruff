use rustc_hash::FxHashSet;

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AlwaysFixableViolation, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for duplicate field definitions in classes.
///
/// ## Why is this bad?
/// Defining a field multiple times in a class body is redundant and likely a
/// mistake.
///
/// ## Example
/// ```python
/// class Person:
///     name = Tom
///     ...
///     name = Ben
/// ```
///
/// Use instead:
/// ```python
/// class Person:
///     name = Tom
///     ...
/// ```
#[violation]
pub struct DuplicateClassFieldDefinition {
    name: String,
}

impl AlwaysFixableViolation for DuplicateClassFieldDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateClassFieldDefinition { name } = self;
        format!("Class field `{name}` is defined multiple times")
    }

    fn fix_title(&self) -> String {
        let DuplicateClassFieldDefinition { name } = self;
        format!("Remove duplicate field definition for `{name}`")
    }
}

/// PIE794
pub(crate) fn duplicate_class_field_definition(checker: &mut Checker, body: &[Stmt]) {
    let mut seen_targets: FxHashSet<&str> = FxHashSet::default();
    for stmt in body {
        // Extract the property name from the assignment statement.
        let target = match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                if let [Expr::Name(id)] = targets.as_slice() {
                    id
                } else {
                    continue;
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(id) = target.as_ref() {
                    id
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // If this is an unrolled augmented assignment (e.g., `x = x + 1`), skip it.
        match stmt {
            Stmt::Assign(ast::StmtAssign { value, .. }) => {
                if any_over_expr(value.as_ref(), &|expr| {
                    expr.as_name_expr().is_some_and(|name| name.id == target.id)
                }) {
                    continue;
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign {
                value: Some(value), ..
            }) => {
                if any_over_expr(value.as_ref(), &|expr| {
                    expr.as_name_expr().is_some_and(|name| name.id == target.id)
                }) {
                    continue;
                }
            }
            _ => continue,
        }

        if !seen_targets.insert(target.id.as_str()) {
            let mut diagnostic = Diagnostic::new(
                DuplicateClassFieldDefinition {
                    name: target.id.to_string(),
                },
                stmt.range(),
            );
            let edit =
                fix::edits::delete_stmt(stmt, Some(stmt), checker.locator(), checker.indexer());
            diagnostic.set_fix(Fix::unsafe_edit(edit).isolate(Checker::isolation(
                checker.semantic().current_statement_id(),
            )));
            checker.diagnostics.push(diagnostic);
        }
    }
}
