use rustpython_parser::ast::{Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

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
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a string literal, assign to variable first")
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
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use an f-string literal, assign to variable first")
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
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a `.format()` string directly, assign to variable first")
    }
}

/// EM101, EM102, EM103
pub fn string_in_exception(checker: &mut Checker, exc: &Expr) {
    if let ExprKind::Call { args, .. } = &exc.node {
        if let Some(first) = args.first() {
            match &first.node {
                // Check for string literals
                ExprKind::Constant {
                    value: Constant::Str(string),
                    ..
                } => {
                    if checker.settings.rules.enabled(Rule::RawStringInException) {
                        if string.len() > checker.settings.flake8_errmsg.max_string_length {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(RawStringInException, Range::from(first)));
                        }
                    }
                }
                // Check for f-strings
                ExprKind::JoinedStr { .. } => {
                    if checker.settings.rules.enabled(Rule::FStringInException) {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(FStringInException, Range::from(first)));
                    }
                }
                // Check for .format() calls
                ExprKind::Call { func, .. } => {
                    if checker.settings.rules.enabled(Rule::DotFormatInException) {
                        if let ExprKind::Attribute { value, attr, .. } = &func.node {
                            if attr == "format" && matches!(value.node, ExprKind::Constant { .. }) {
                                checker.diagnostics.push(Diagnostic::new(
                                    DotFormatInException,
                                    Range::from(first),
                                ));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
