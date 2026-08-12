use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Number};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for `while` loops that use `1` as their condition.
///
/// ## Why is this bad?
/// `while 1:` is a Python 2 idiom, where `True` was a global that could be
/// rebound and so had to be loaded and tested on every iteration. In Python 3
/// `True` is a keyword, so both spellings compile to the same bytecode and
/// `while True:` is clearer about the loop being infinite.
///
/// ## Example
/// ```python
/// while 1:
///     print("Hello, world!")
/// ```
///
/// Use instead:
/// ```python
/// while True:
///     print("Hello, world!")
/// ```
///
/// ## References
/// - [Python documentation: `while`](https://docs.python.org/3/reference/compound_stmts.html#the-while-statement)
/// - [PEP 285 – Adding a bool type](https://peps.python.org/pep-0285/)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct WhileOne;

impl AlwaysFixableViolation for WhileOne {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `while True:` instead of `while 1:`".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `True`".to_string()
    }
}

/// UP048
pub(crate) fn while_one(checker: &Checker, while_stmt: &ast::StmtWhile) {
    let Expr::NumberLiteral(ast::ExprNumberLiteral {
        value: Number::Int(value),
        ..
    }) = &*while_stmt.test
    else {
        return;
    };

    // Also covers other spellings of one, such as `0x1`.
    if value.as_u8() != Some(1) {
        return;
    }

    let range = while_stmt.test.range();
    let mut diagnostic = checker.report_diagnostic(WhileOne, range);
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "True".to_string(),
        range,
    )));
}
