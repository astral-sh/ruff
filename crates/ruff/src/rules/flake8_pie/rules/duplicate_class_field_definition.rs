use rustc_hash::FxHashSet;
use rustpython_parser::ast::{self, Expr, Ranged, Stmt};

use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::{AlwaysAutofixableViolation, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

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

impl AlwaysAutofixableViolation for DuplicateClassFieldDefinition {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DuplicateClassFieldDefinition { name } = self;
        format!("Class field `{name}` is defined multiple times")
    }

    fn autofix_title(&self) -> String {
        let DuplicateClassFieldDefinition { name } = self;
        format!("Remove duplicate field definition for `{name}`")
    }
}

/// PIE794
pub(crate) fn duplicate_class_field_definition<'a, 'b>(
    checker: &mut Checker<'a>,
    parent: &'b Stmt,
    body: &'b [Stmt],
) where
    'b: 'a,
{
    let mut seen_targets: FxHashSet<&str> = FxHashSet::default();
    for stmt in body {
        // Extract the property name from the assignment statement.
        let target = match stmt {
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                if targets.len() != 1 {
                    continue;
                }
                if let Expr::Name(ast::ExprName { id, .. }) = &targets[0] {
                    id
                } else {
                    continue;
                }
            }
            Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                    id
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        if !seen_targets.insert(target) {
            let mut diagnostic = Diagnostic::new(
                DuplicateClassFieldDefinition {
                    name: target.to_string(),
                },
                stmt.range(),
            );
            if checker.patch(diagnostic.kind.rule()) {
                let edit = autofix::edits::delete_stmt(
                    stmt,
                    Some(parent),
                    checker.locator,
                    checker.indexer,
                );
                diagnostic.set_fix(Fix::suggested(edit).isolate(checker.isolation(Some(parent))));
            }
            checker.diagnostics.push(diagnostic);
        }
    }
}
