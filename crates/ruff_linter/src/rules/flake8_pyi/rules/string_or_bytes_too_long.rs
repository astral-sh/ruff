use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::helpers::is_docstring_stmt;
use ruff_python_ast::{self as ast, StringLike};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the use of string and bytes literals longer than 50 characters
/// in stub (`.pyi`) files.
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
/// def foo(arg: str = "51 character stringgggggggggggggggggggggggggggggggg") -> None:
///     ...
/// ```
///
/// Use instead:
/// ```python
/// def foo(arg: str = ...) -> None:
///     ...
/// ```
#[violation]
pub struct StringOrBytesTooLong;

impl AlwaysFixableViolation for StringOrBytesTooLong {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("String and bytes literals longer than 50 characters are not permitted")
    }

    fn fix_title(&self) -> String {
        "Replace with `...`".to_string()
    }
}

/// PYI053
pub(crate) fn string_or_bytes_too_long(checker: &mut Checker, string: StringLike) {
    // Ignore docstrings.
    if is_docstring_stmt(checker.semantic().current_statement()) {
        return;
    }

    if let Some(depr_message) =
        warnings_dot_deprecated_message(checker.semantic().current_expression_parent(), checker)
    {
        if let StringLike::StringLiteral(literal) = string {
            if depr_message == literal {
                return;
            }
        }
    }

    let length = match string {
        StringLike::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.chars().count(),
        StringLike::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => value.len(),
        StringLike::FStringLiteral(ast::FStringLiteralElement { value, .. }) => {
            value.chars().count()
        }
    };
    if length <= 50 {
        return;
    }

    let mut diagnostic = Diagnostic::new(StringOrBytesTooLong, string.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "...".to_string(),
        string.range(),
    )));
    checker.diagnostics.push(diagnostic);
}

fn warnings_dot_deprecated_message<'a>(
    expr: Option<&'a ast::Expr>,
    checker: &Checker,
) -> Option<&'a ast::ExprStringLiteral> {
    // If `expr` represents a call to `warnings.deprecated()`
    // (or the `typing_extensions`` backport), return the string-literal
    // deprecation message passed as the first argument to the call.
    // Else, return `None`.
    let Some(ast::Expr::Call(ast::ExprCall {
        func,
        arguments: ast::Arguments { args, .. },
        ..
    })) = expr
    else {
        return None;
    };
    let [ast::Expr::StringLiteral(depr_message @ ast::ExprStringLiteral { .. }), ..] =
        &args.as_slice()
    else {
        return None;
    };
    if checker
        .semantic()
        .resolve_call_path(func)
        .is_some_and(|call_path| {
            matches!(
                call_path.as_slice(),
                ["warnings" | "typing_extensions", "deprecated"]
            )
        })
    {
        Some(depr_message)
    } else {
        None
    }
}
