use ruff_python_ast as ast;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

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

impl AlwaysFixableViolation for EmptyTypeCheckingBlock {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Found empty type-checking block")
    }

    fn fix_title(&self) -> String {
        format!("Delete empty type-checking block")
    }
}

/// TCH005
pub(crate) fn empty_type_checking_block(checker: &mut Checker, stmt: &ast::StmtIf) {
    if !typing::is_type_checking_block(stmt, checker.semantic()) {
        return;
    }

    if !stmt.elif_else_clauses.is_empty() {
        return;
    }

    let [stmt] = stmt.body.as_slice() else {
        return;
    };
    if !stmt.is_pass_stmt() {
        return;
    }

    let mut diagnostic = Diagnostic::new(EmptyTypeCheckingBlock, stmt.range());
    // Delete the entire type-checking block.
    let stmt = checker.semantic().current_statement();
    let parent = checker.semantic().current_statement_parent();
    let edit = fix::edits::delete_stmt(stmt, parent, checker.locator(), checker.indexer());
    diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
        checker.semantic().current_statement_parent_id(),
    )));
    checker.diagnostics.push(diagnostic);
}
