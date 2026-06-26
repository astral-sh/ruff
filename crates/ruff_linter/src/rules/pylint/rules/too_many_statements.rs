use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_ast::identifier::Identifier;

use crate::Violation;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::num_statements;

/// ## What it does
/// Checks for functions or methods with too many statements.
///
/// By default, this rule allows up to 50 statements, as configured by the
/// [`lint.pylint.max-statements`] option.
///
/// ## Why is this bad?
/// Functions or methods with many statements are harder to understand
/// and maintain.
///
/// Instead, consider refactoring the function or method into smaller
/// functions or methods, or identifying generalizable patterns and
/// replacing them with generic logic or abstractions.
///
/// ## Example
/// ```python
/// def is_even(number: int) -> bool:
///     if number == 0:
///         return True
///     elif number == 1:
///         return False
///     elif number == 2:
///         return True
///     elif number == 3:
///         return False
///     elif number == 4:
///         return True
///     elif number == 5:
///         return False
///     else:
///         ...
/// ```
///
/// Use instead:
/// ```python
/// def is_even(number: int) -> bool:
///     return number % 2 == 0
/// ```
///
/// ## Options
/// - `lint.pylint.max-statements`
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.240")]
pub(crate) struct TooManyStatements {
    statements: usize,
    max_statements: usize,
}

impl Violation for TooManyStatements {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyStatements {
            statements,
            max_statements,
        } = self;
        format!("Too many statements ({statements} > {max_statements})")
    }
}

/// PLR0915
pub(crate) fn too_many_statements(
    checker: &Checker,
    stmt: &Stmt,
    body: &[Stmt],
    max_statements: usize,
) {
    let statements = num_statements(body);
    if statements > max_statements {
        checker.report_diagnostic(
            TooManyStatements {
                statements,
                max_statements,
            },
            stmt.identifier(),
        );
    }
}
