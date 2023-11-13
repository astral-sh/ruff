use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::{Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::whitespace::trailing_comment_start_offset;
use ruff_python_ast::Stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix;

/// ## What it does
/// Checks for unnecessary `pass` statements in functions, classes, and other
/// blocks.
///
/// In [preview], this rule also checks for unnecessary ellipsis (`...`)
/// literals.
///
/// ## Why is this bad?
/// In Python, the `pass` statement and ellipsis (`...`) literal serve as
/// placeholders, allowing for syntactically correct empty code blocks. The
/// primary purpose of these nodes is to avoid syntax errors in situations
/// where a statement or expression is syntactically required, but no code
/// needs to be executed.
///
/// If a `pass` or ellipsis is present in a code block that includes at least
/// one other statement (even, e.g., a docstring), it is unnecessary and should
/// be removed.
///
/// ## Example
/// ```python
/// def func():
///     """Placeholder docstring."""
///     pass
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     """Placeholder docstring."""
/// ```
///
/// In [preview]:
/// ```python
/// def func():
///     """Placeholder docstring."""
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def func():
///     """Placeholder docstring."""
/// ```
///
/// ## References
/// - [Python documentation: The `pass` statement](https://docs.python.org/3/reference/simple_stmts.html#the-pass-statement)
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[violation]
pub struct UnnecessaryPlaceholder {
    kind: Placeholder,
}

impl AlwaysFixableViolation for UnnecessaryPlaceholder {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { kind } = self;
        match kind {
            Placeholder::Pass => format!("Unnecessary `pass` statement"),
            Placeholder::Ellipsis => format!("Unnecessary `...` literal"),
        }
    }

    fn fix_title(&self) -> String {
        let Self { kind } = self;
        match kind {
            Placeholder::Pass => format!("Remove unnecessary `pass`"),
            Placeholder::Ellipsis => format!("Remove unnecessary `...`"),
        }
    }
}

/// PIE790
pub(crate) fn unnecessary_placeholder(checker: &mut Checker, body: &[Stmt]) {
    if body.len() < 2 {
        return;
    }

    for stmt in body {
        let kind = match stmt {
            Stmt::Pass(_) => Placeholder::Pass,
            Stmt::Expr(expr)
                if expr.value.is_ellipsis_literal_expr()
                    && checker.settings.preview.is_enabled() =>
            {
                Placeholder::Ellipsis
            }
            _ => continue,
        };

        let mut diagnostic = Diagnostic::new(UnnecessaryPlaceholder { kind }, stmt.range());
        let edit = if let Some(index) = trailing_comment_start_offset(stmt, checker.locator()) {
            Edit::range_deletion(stmt.range().add_end(index))
        } else {
            fix::edits::delete_stmt(stmt, None, checker.locator(), checker.indexer())
        };
        diagnostic.set_fix(Fix::safe_edit(edit).isolate(Checker::isolation(
            checker.semantic().current_statement_id(),
        )));
        checker.diagnostics.push(diagnostic);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Placeholder {
    Pass,
    Ellipsis,
}

impl std::fmt::Display for Placeholder {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Pass => fmt.write_str("pass"),
            Self::Ellipsis => fmt.write_str("..."),
        }
    }
}
