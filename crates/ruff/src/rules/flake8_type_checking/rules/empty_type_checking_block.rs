use rustpython_parser::ast;
use rustpython_parser::ast::Ranged;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};

use crate::autofix;
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
pub(crate) fn empty_type_checking_block(checker: &mut Checker, stmt: &ast::StmtIf) {
    if stmt.body.len() != 1 {
        return;
    }

    let stmt = &stmt.body[0];
    if !stmt.is_pass_stmt() {
        return;
    }

    let mut diagnostic = Diagnostic::new(EmptyTypeCheckingBlock, stmt.range());
    if checker.patch(diagnostic.kind.rule()) {
        // Delete the entire type-checking block.
        let stmt = checker.semantic().stmt();
        let parent = checker.semantic().stmt_parent();
        let edit = autofix::edits::delete_stmt(stmt, parent, checker.locator, checker.indexer);
        diagnostic.set_fix(Fix::automatic(edit).isolate(checker.isolation(parent)));
    }
    checker.diagnostics.push(diagnostic);
}
