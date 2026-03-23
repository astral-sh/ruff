use ruff_python_ast::{self as ast, Expr, Stmt};

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for `if TYPE_CHECKING:` blocks that appear before regular
/// top-level import statements.
///
/// ## Why is this bad?
/// By convention, `if TYPE_CHECKING:` blocks should be placed after all
/// regular (runtime) imports. Placing them before regular imports makes it
/// harder to distinguish between runtime dependencies and type-only
/// dependencies at a glance.
///
/// Note that while no PEP mandates this ordering, it is a widely adopted
/// convention and is the placement used by the `TC001`–`TC003` autofixes
/// when they introduce a `TYPE_CHECKING` block.
///
/// ## Example
/// ```python
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     from collections.abc import Sequence
///
/// from mylib import MyClass
/// ```
///
/// Use instead:
/// ```python
/// from typing import TYPE_CHECKING
///
/// from mylib import MyClass
///
/// if TYPE_CHECKING:
///     from collections.abc import Sequence
/// ```
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.14.15")]
pub(crate) struct TypeCheckingBlockBeforeImports;

impl Violation for TypeCheckingBlockBeforeImports {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`if TYPE_CHECKING:` block should be after all top-level imports".to_string()
    }
}

/// Returns `true` if the given `if` statement is an `if TYPE_CHECKING:` block.
///
/// This is a syntactic check that does not require the semantic model,
/// handling the two common forms:
/// - `if TYPE_CHECKING:`
/// - `if typing.TYPE_CHECKING:`
fn is_type_checking_block_syntactic(stmt: &ast::StmtIf) -> bool {
    match stmt.test.as_ref() {
        Expr::Name(ast::ExprName { id, .. }) => id == "TYPE_CHECKING",
        Expr::Attribute(ast::ExprAttribute { attr, .. }) => attr == "TYPE_CHECKING",
        _ => false,
    }
}

/// TC009
pub(crate) fn type_checking_block_before_imports(checker: &Checker, suite: &[Stmt]) {
    // Find the index of the last top-level import statement.
    let last_import_index = suite
        .iter()
        .rposition(|stmt| matches!(stmt, Stmt::Import(_) | Stmt::ImportFrom(_)));

    let Some(last_import_index) = last_import_index else {
        // No imports at all — nothing to check.
        return;
    };

    // Flag any `if TYPE_CHECKING:` block that appears before the last import.
    for stmt in &suite[..last_import_index] {
        if let Stmt::If(if_stmt) = stmt {
            if is_type_checking_block_syntactic(if_stmt) {
                checker.report_diagnostic(TypeCheckingBlockBeforeImports, if_stmt.range());
            }
        }
    }
}
