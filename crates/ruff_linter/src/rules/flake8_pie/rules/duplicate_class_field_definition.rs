use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::helpers::any_over_expr;
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;
use crate::{AlwaysFixableViolation, Fix};

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
///
/// ## Fix safety
/// This fix is always marked as unsafe since we cannot know
/// for certain which assignment was intended.
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.208")]
pub(crate) struct DuplicateClassFieldDefinition {
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
pub(crate) fn duplicate_class_field_definition(checker: &Checker, body: &[Stmt]) {
    let mut seen_targets: FxHashSet<&str> = FxHashSet::default();
    for stmt in body {
        // Extract the property name from the assignment statement.
        let (target, value) = match stmt {
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                let [Expr::Name(target)] = targets.as_slice() else {
                    continue;
                };

                (target, Some(&**value))
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, value, .. }) => {
                if let Expr::Name(id) = target.as_ref() {
                    (id, value.as_deref())
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // If this is an unrolled augmented assignment (e.g., `x = x + 1`), skip it.
        if let Some(value) = value
            && any_over_expr(value, &|expr| {
                expr.as_name_expr().is_some_and(|name| name.id == target.id)
            })
        {
            continue;
        }

        if !seen_targets.insert(target.id.as_str()) {
            let mut diagnostic = checker.report_diagnostic(
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
        }
    }
}
