use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::Stmt;
use ruff_python_ast::identifier::Identifier;

use crate::Violation;

use crate::checkers::ast::Checker;
use crate::rules::pylint::helpers::num_statements;

/// ## What it does
/// Checks for try clauses with too many statements.
///
/// By default, this rule allows up to 5 statements, as configured by the
/// [`lint.pylint.max-statements-in-try`] option.
///
/// ## Why is this bad?
/// Try clauses with many statements make unexpected exceptions harder
/// to detect and debug.
///
/// Instead, consider narrowing the try clause to only encompass code that
/// may raise exceptions you can anticipate or know about, moving all other
/// statements either before or after the try clause, or factoring out a helper function.
///
/// ## Example
/// ```python
/// from random import randint
///
///
/// def random_ratio() -> float:
///     try:
///         a = randint(-100, 100)
///         b = randint(-100, 100)
///         c = randint(-100, 100)
///         d = randint(-100, 100)
///         scale = randint(1, 5)
///         res = scale * (a + b) / (c + d)
///     except ZeroDivisionError:
///         return random_ratio()
///     else:
///         return res
/// ```
///
/// Use instead:
/// ```python
/// from random import randint
///
///
/// def random_ratio() -> float:
///     a = randint(-100, 100)
///     b = randint(-100, 100)
///     c = randint(-100, 100)
///     d = randint(-100, 100)
///     scale = randint(1, 5)
///     try:
///         # every statement that cannot raise was moved outside the try clause
///         res = scale * (a + b) / (c + d)
///     except ZeroDivisionError:
///         return random_ratio()
///     else:
///         return res
/// ```
///
/// ## Options
/// - `lint.pylint.max-statements-in-try`
///
/// ## References
///
/// [Pylint's reference implementation](https://pylint.pycqa.org/en/latest/user_guide/configuration/all-options.html#broad-try-clause-checker)
/// uses a different default setting.
/// To replicate it exactly, set `lint.pylint.max-statements-in-try` to 1.
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct TooManyStatementsInTryClause {
    statements: usize,
    max_statements: usize,
}

impl Violation for TooManyStatementsInTryClause {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TooManyStatementsInTryClause {
            statements,
            max_statements,
        } = self;
        format!("Try clause contains too many statements ({statements} > {max_statements})")
    }
}

/// W0717
pub(crate) fn too_many_try_statements(
    checker: &Checker,
    stmt: &Stmt,
    body: &[Stmt],
    max_statements: usize,
) {
    let statements = num_statements(body);
    if statements > max_statements {
        checker.report_diagnostic(
            TooManyStatementsInTryClause {
                statements,
                max_statements,
            },
            stmt.identifier(),
        );
    }
}
