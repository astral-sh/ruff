use ruff_python_ast::{self as ast, Constant, Expr};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;

#[violation]
pub struct StringOrBytesTooLong;

/// ## What it does
/// Checks for the use of string and bytes literals longer than 50 characters.
///
/// ## Why is this bad?
/// If a function has a default value where the string or bytes representation
/// is greater than 50 characters, it is likely to be an implementation detail
/// or a constant that varies depending on the system you're running on.
///
/// Consider replacing such constants with ellipses (`...`).
///
/// ## Example
/// ```python
/// def foo(arg: str = "51 character stringgggggggggggggggggggggggggggggggg") -> None: ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(arg: str = ...) -> None: ...
/// ```
impl AlwaysAutofixableViolation for StringOrBytesTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("String and bytes literals longer than 50 characters are not permitted")
    }

    fn autofix_title(&self) -> String {
        "Replace with `...`".to_string()
    }
}

/// PYI053
pub(crate) fn string_or_bytes_too_long(checker: &mut Checker, expr: &Expr) {
    // Ignore docstrings.
    if is_docstring_stmt(checker.semantic().current_statement()) {
        return;
    }

    let length = match expr {
        Expr::Constant(ast::ExprConstant {
            value: Constant::Str(s),
            ..
        }) => s.chars().count(),
        Expr::Constant(ast::ExprConstant {
            value: Constant::Bytes(bytes),
            ..
        }) => bytes.len(),
        _ => return,
    };
    if length <= 50 {
        return;
    }

    let mut diagnostic = Diagnostic::new(StringOrBytesTooLong, expr.range());
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.set_fix(Fix::suggested(Edit::range_replacement(
            "...".to_string(),
            expr.range(),
        )));
    }
    checker.diagnostics.push(diagnostic);
}
