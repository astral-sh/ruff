use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::whitespace;
use ruff_python_ast::{self as ast, Arguments, Expr, Stmt};
use ruff_python_codegen::Stylist;
use ruff_source_file::LineRanges;
use ruff_text_size::Ranged;

use crate::Locator;
use crate::checkers::ast::Checker;
use crate::preview::is_raise_exception_byte_string_enabled;
use crate::registry::Rule;
use crate::{Edit, Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for the use of string literals in exception constructors.
///
/// In [preview], this rule checks for byte string literals in
/// exception constructors.
///
/// ## Why is this bad?
/// Python includes the `raise` in the default traceback (and formatters
/// like Rich and IPython do too).
///
/// By using a string literal, the error message will be duplicated in the
/// traceback, which can make the traceback less readable.
///
/// ## Example
/// Given:
/// ```python
/// raise RuntimeError("'Some value' is incorrect")
/// ```
///
/// Python will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 2, in <module>
///     raise RuntimeError("'Some value' is incorrect")
/// RuntimeError: 'Some value' is incorrect
/// ```
///
/// Instead, assign the string to a variable:
/// ```python
/// msg = "'Some value' is incorrect"
/// raise RuntimeError(msg)
/// ```
///
/// Which will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 3, in <module>
///     raise RuntimeError(msg)
/// RuntimeError: 'Some value' is incorrect
/// ```
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct RawStringInException;

impl Violation for RawStringInException {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Exception must not use a string literal, assign to variable first".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Assign to variable; remove string literal".to_string())
    }
}

/// ## What it does
/// Checks for the use of f-strings in exception constructors.
///
/// ## Why is this bad?
/// Python includes the `raise` in the default traceback (and formatters
/// like Rich and IPython do too).
///
/// By using an f-string, the error message will be duplicated in the
/// traceback, which can make the traceback less readable.
///
/// ## Example
/// Given:
/// ```python
/// sub = "Some value"
/// raise RuntimeError(f"{sub!r} is incorrect")
/// ```
///
/// Python will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 2, in <module>
///     raise RuntimeError(f"{sub!r} is incorrect")
/// RuntimeError: 'Some value' is incorrect
/// ```
///
/// Instead, assign the string to a variable:
/// ```python
/// sub = "Some value"
/// msg = f"{sub!r} is incorrect"
/// raise RuntimeError(msg)
/// ```
///
/// Which will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 3, in <module>
///     raise RuntimeError(msg)
/// RuntimeError: 'Some value' is incorrect
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct FStringInException;

impl Violation for FStringInException {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Exception must not use an f-string literal, assign to variable first".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Assign to variable; remove f-string literal".to_string())
    }
}

/// ## What it does
/// Checks for the use of `.format` calls on string literals in exception
/// constructors.
///
/// ## Why is this bad?
/// Python includes the `raise` in the default traceback (and formatters
/// like Rich and IPython do too).
///
/// By using a `.format` call, the error message will be duplicated in the
/// traceback, which can make the traceback less readable.
///
/// ## Example
/// Given:
/// ```python
/// sub = "Some value"
/// raise RuntimeError("'{}' is incorrect".format(sub))
/// ```
///
/// Python will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 2, in <module>
///     raise RuntimeError("'{}' is incorrect".format(sub))
/// RuntimeError: 'Some value' is incorrect
/// ```
///
/// Instead, assign the string to a variable:
/// ```python
/// sub = "Some value"
/// msg = "'{}' is incorrect".format(sub)
/// raise RuntimeError(msg)
/// ```
///
/// Which will produce a traceback like:
/// ```console
/// Traceback (most recent call last):
///   File "tmp.py", line 3, in <module>
///     raise RuntimeError(msg)
/// RuntimeError: 'Some value' is incorrect
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct DotFormatInException;

impl Violation for DotFormatInException {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Exception must not use a `.format()` string directly, assign to variable first".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Assign to variable; remove `.format()` string".to_string())
    }
}

/// EM101, EM102, EM103
pub(crate) fn string_in_exception(checker: &Checker, stmt: &Stmt, exc: &Expr) {
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments { args, .. },
        ..
    }) = exc
    else {
        return;
    };

    if checker.semantic().match_typing_expr(func, "cast") {
        return;
    }

    if let Some(first) = args.first() {
        match first {
            // Check for string literals.
            Expr::StringLiteral(ast::ExprStringLiteral { value: string, .. }) => {
                if checker.is_rule_enabled(Rule::RawStringInException) {
                    if string.len() >= checker.settings().flake8_errmsg.max_string_length {
                        let mut diagnostic =
                            checker.report_diagnostic(RawStringInException, first.range());
                        if let Some(indentation) = whitespace::indentation(checker.source(), stmt) {
                            diagnostic.set_fix(generate_fix(
                                stmt,
                                first,
                                indentation,
                                checker.stylist(),
                                checker.locator(),
                            ));
                        }
                    }
                }
            }
            // Check for byte string literals.
            Expr::BytesLiteral(ast::ExprBytesLiteral { value: bytes, .. }) => {
                if checker.settings().rules.enabled(Rule::RawStringInException) {
                    if bytes.len() >= checker.settings().flake8_errmsg.max_string_length
                        && is_raise_exception_byte_string_enabled(checker.settings())
                    {
                        let mut diagnostic =
                            checker.report_diagnostic(RawStringInException, first.range());
                        if let Some(indentation) = whitespace::indentation(checker.source(), stmt) {
                            diagnostic.set_fix(generate_fix(
                                stmt,
                                first,
                                indentation,
                                checker.stylist(),
                                checker.locator(),
                            ));
                        }
                    }
                }
            }
            // Check for f-strings.
            Expr::FString(_) => {
                if checker.is_rule_enabled(Rule::FStringInException) {
                    let mut diagnostic =
                        checker.report_diagnostic(FStringInException, first.range());
                    if let Some(indentation) = whitespace::indentation(checker.source(), stmt) {
                        diagnostic.set_fix(generate_fix(
                            stmt,
                            first,
                            indentation,
                            checker.stylist(),
                            checker.locator(),
                        ));
                    }
                }
            }
            // Check for .format() calls.
            Expr::Call(ast::ExprCall { func, .. }) => {
                if checker.is_rule_enabled(Rule::DotFormatInException) {
                    if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = func.as_ref() {
                        if attr == "format" && value.is_literal_expr() {
                            let mut diagnostic =
                                checker.report_diagnostic(DotFormatInException, first.range());
                            if let Some(indentation) =
                                whitespace::indentation(checker.source(), stmt)
                            {
                                diagnostic.set_fix(generate_fix(
                                    stmt,
                                    first,
                                    indentation,
                                    checker.stylist(),
                                    checker.locator(),
                                ));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// Generate the [`Fix`] for EM001, EM002, and EM003 violations.
///
/// This assumes that the violation is fixable and that the patch should
/// be generated. The exception argument should be either a string literal,
/// an f-string, or a `.format` string.
///
/// The fix includes two edits:
/// 1. Insert the exception argument into a variable assignment before the
///    `raise` statement. The variable name is `msg`.
/// 2. Replace the exception argument with the variable name.
fn generate_fix(
    stmt: &Stmt,
    exc_arg: &Expr,
    stmt_indentation: &str,
    stylist: &Stylist,
    locator: &Locator,
) -> Fix {
    Fix::unsafe_edits(
        Edit::insertion(
            if locator.contains_line_break(exc_arg.range()) {
                format!(
                    "msg = ({line_ending}{stmt_indentation}{indentation}{}{line_ending}{stmt_indentation}){line_ending}{stmt_indentation}",
                    locator.slice(exc_arg.range()),
                    line_ending = stylist.line_ending().as_str(),
                    indentation = stylist.indentation().as_str(),
                )
            } else {
                format!(
                    "msg = {}{}{}",
                    locator.slice(exc_arg.range()),
                    stylist.line_ending().as_str(),
                    stmt_indentation,
                )
            },
            stmt.start(),
        ),
        [Edit::range_replacement(
            String::from("msg"),
            exc_arg.range(),
        )],
    )
}
