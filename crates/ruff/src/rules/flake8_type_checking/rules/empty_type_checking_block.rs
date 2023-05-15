use log::error;
use rustpython_parser::ast::{Stmt, StmtKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::RefEquality;

use crate::autofix::actions::delete_stmt;
use crate::checkers::ast::Checker;
use crate::registry::AsRule;

/// ## What it does
/// Checks for an empty type-checking block.
///
/// ## Why is this bad?
/// The type-checking block does not do anything and should be removed to avoid
/// confusion.
///
/// ## Example
/// ```python
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     pass
///
/// print("Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// print("Hello, world!")
/// ```
///
/// ## References
/// - [PEP 535](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[violation]
pub struct EmptyTypeCheckingBlock;

impl AlwaysAutofixableViolation for EmptyTypeCheckingBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found empty type-checking block")
    }

    fn autofix_title(&self) -> String {
        format!("Delete empty type-checking block")
    }
}

/// TCH005
pub(crate) fn empty_type_checking_block<'a, 'b>(
    checker: &mut Checker<'a>,
    stmt: &'a Stmt,
    body: &'a [Stmt],
) where
    'b: 'a,
{
    if body.len() == 1 && matches!(body[0].node, StmtKind::Pass) {
        let mut diagnostic = Diagnostic::new(EmptyTypeCheckingBlock, body[0].range());

        // Delete the entire type-checking block.
        if checker.patch(diagnostic.kind.rule()) {
            let parent = checker.ctx.stmts.parent(stmt);
            let deleted: Vec<&Stmt> = checker.deletions.iter().map(Into::into).collect();
            match delete_stmt(
                stmt,
                parent,
                &deleted,
                checker.locator,
                checker.indexer,
                checker.stylist,
            ) {
                Ok(edit) => {
                    if edit.is_deletion() || edit.content() == Some("pass") {
                        checker.deletions.insert(RefEquality(stmt));
                    }
                    #[allow(deprecated)]
                    diagnostic.set_fix(Fix::unspecified(edit));
                }
                Err(e) => error!("Failed to remove empty type-checking block: {e}"),
            }
        }

        checker.diagnostics.push(diagnostic);
    }
}
