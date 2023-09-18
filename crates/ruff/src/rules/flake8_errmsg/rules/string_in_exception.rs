use ruff_python_ast::{self as ast, Arguments, Constant, Expr, ExprContext, Stmt};
use ruff_text_size::{Ranged, TextRange};

use ruff_diagnostics::{AutofixKind, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::whitespace;
use ruff_python_codegen::{Generator, Stylist};

use crate::checkers::ast::Checker;
use crate::registry::{AsRule, Rule};

/// ## What it does
/// Checks for the use of string literals in exception constructors.
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
///     raise RuntimeError("Some value is incorrect")
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
#[violation]
pub struct RawStringInException;

impl Violation for RawStringInException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a string literal, assign to variable first")
    }

    fn autofix_title(&self) -> Option<String> {
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
///   File "tmp.py", line 3, in <module>
///     raise RuntimeError(msg)
/// RuntimeError: 'Some value' is incorrect
/// ```
#[violation]
pub struct FStringInException;

impl Violation for FStringInException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use an f-string literal, assign to variable first")
    }

    fn autofix_title(&self) -> Option<String> {
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
#[violation]
pub struct DotFormatInException;

impl Violation for DotFormatInException {
    const AUTOFIX: AutofixKind = AutofixKind::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a `.format()` string directly, assign to variable first")
    }

    fn autofix_title(&self) -> Option<String> {
        Some("Assign to variable; remove `.format()` string".to_string())
    }
}

/// EM101, EM102, EM103
pub(crate) fn string_in_exception(checker: &mut Checker, stmt: &Stmt, exc: &Expr) {
    if let Expr::Call(ast::ExprCall {
        arguments: Arguments { args, .. },
        ..
    }) = exc
    {
        if let Some(first) = args.first() {
            match first {
                // Check for string literals.
                Expr::Constant(ast::ExprConstant {
                    value: Constant::Str(string),
                    ..
                }) => {
                    if checker.enabled(Rule::RawStringInException) {
                        if string.len() >= checker.settings.flake8_errmsg.max_string_length {
                            let mut diagnostic =
                                Diagnostic::new(RawStringInException, first.range());
                            if checker.patch(diagnostic.kind.rule()) {
                                if let Some(indentation) =
                                    whitespace::indentation(checker.locator(), stmt)
                                {
                                    if checker.semantic().is_available("msg") {
                                        diagnostic.set_fix(generate_fix(
                                            stmt,
                                            first,
                                            indentation,
                                            checker.stylist(),
                                            checker.generator(),
                                        ));
                                    }
                                }
                            }
                            checker.diagnostics.push(diagnostic);
                        }
                    }
                }
                // Check for f-strings.
                Expr::FString(_) => {
                    if checker.enabled(Rule::FStringInException) {
                        let mut diagnostic = Diagnostic::new(FStringInException, first.range());
                        if checker.patch(diagnostic.kind.rule()) {
                            if let Some(indentation) =
                                whitespace::indentation(checker.locator(), stmt)
                            {
                                if checker.semantic().is_available("msg") {
                                    diagnostic.set_fix(generate_fix(
                                        stmt,
                                        first,
                                        indentation,
                                        checker.stylist(),
                                        checker.generator(),
                                    ));
                                }
                            }
                        }
                        checker.diagnostics.push(diagnostic);
                    }
                }
                // Check for .format() calls.
                Expr::Call(ast::ExprCall { func, .. }) => {
                    if checker.enabled(Rule::DotFormatInException) {
                        if let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) =
                            func.as_ref()
                        {
                            if attr == "format" && value.is_constant_expr() {
                                let mut diagnostic =
                                    Diagnostic::new(DotFormatInException, first.range());
                                if checker.patch(diagnostic.kind.rule()) {
                                    if let Some(indentation) =
                                        whitespace::indentation(checker.locator(), stmt)
                                    {
                                        if checker.semantic().is_available("msg") {
                                            diagnostic.set_fix(generate_fix(
                                                stmt,
                                                first,
                                                indentation,
                                                checker.stylist(),
                                                checker.generator(),
                                            ));
                                        }
                                    }
                                }
                                checker.diagnostics.push(diagnostic);
                            }
                        }
                    }
                }
                _ => {}
            }
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
///   `raise` statement. The variable name is `msg`.
/// 2. Replace the exception argument with the variable name.
fn generate_fix(
    stmt: &Stmt,
    exc_arg: &Expr,
    indentation: &str,
    stylist: &Stylist,
    generator: Generator,
) -> Fix {
    let assignment = Stmt::Assign(ast::StmtAssign {
        targets: vec![Expr::Name(ast::ExprName {
            id: "msg".into(),
            ctx: ExprContext::Store,
            range: TextRange::default(),
        })],
        value: Box::new(exc_arg.clone()),
        range: TextRange::default(),
    });

    Fix::suggested_edits(
        Edit::insertion(
            format!(
                "{}{}{}",
                generator.stmt(&assignment),
                stylist.line_ending().as_str(),
                indentation,
            ),
            stmt.start(),
        ),
        [Edit::range_replacement(
            String::from("msg"),
            exc_arg.range(),
        )],
    )
}
