use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Checks for raw usage of a string literal in Exception raising.
    ///
    /// ## Why is this bad?
    /// Python includes the line with the raise in the default traceback (and most
    /// other formatters, like Rich and IPython to too).
    ///
    /// ## Example
    ///
    /// This exception
    ///
    /// ```python
    /// raise RuntimeError("'Some value' is incorrect")
    /// ```
    ///
    /// will produce a traceback like this:
    ///
    /// ```python
    /// Traceback (most recent call last):
    ///   File "tmp.py", line 2, in <module>
    ///     raise RuntimeError("Some value is incorrect")
    /// RuntimeError: 'Some value' is incorrect
    /// ```
    ///
    /// If this is longer or more complex, the duplication can be quite confusing for a
    /// user unaccustomed to reading tracebacks.
    ///
    /// While if you always assign to something like `msg`
    ///
    /// ```python
    /// msg = "'Some value' is incorrect"
    /// raise RuntimeError(msg)
    /// ```
    ///
    /// then you get:
    ///
    /// ```python
    /// Traceback (most recent call last):
    ///   File "tmp.py", line 3, in <module>
    ///     raise RuntimeError(msg)
    /// RuntimeError: 'Some value' is incorrect
    /// ```
    ///
    /// Now there's a simpler traceback, less code, and no double message. If you have
    /// a long message, this also often formats better when using Black, too.
    pub struct RawStringInException;
);
impl Violation for RawStringInException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use a string literal, assign to variable first")
    }
}

define_violation!(
    /// ## What it does
    /// Checks for raw usage of an f-string literal in Exception raising.
    ///
    /// ## Why is this bad?
    /// Python includes the line with the raise in the default traceback (and most
    /// other formatters, like Rich and IPython to too).
    ///
    /// ## Example
    ///
    /// This exception
    ///
    /// ```python
    /// sub = "Some value"
    /// raise RuntimeError(f"{sub!r} is incorrect")
    /// ```
    ///
    /// will produce a traceback like this:
    ///
    /// ```python
    /// Traceback (most recent call last):
    ///   File "tmp.py", line 2, in <module>
    ///     raise RuntimeError(f"{sub!r} is incorrect")
    /// RuntimeError: 'Some value' is incorrect
    /// ```
    ///
    /// If this is longer or more complex, the duplication can be quite confusing for a
    /// user unaccustomed to reading tracebacks.
    ///
    /// While if you always assign to something like `msg`
    ///
    /// ```python
    /// sub = "Some value"
    /// msg = f"{sub!r} is incorrect"
    /// raise RuntimeError(msg)
    /// ```
    ///
    /// then you get:
    ///
    /// ```python
    /// Traceback (most recent call last):
    ///   File "tmp.py", line 3, in <module>
    ///     raise RuntimeError(msg)
    /// RuntimeError: 'Some value' is incorrect
    /// ```
    ///
    /// Now there's a simpler traceback, less code, and no double message. If you have
    /// a long message, this also often formats better when using Black, too.
    pub struct FStringInException;
);
impl Violation for FStringInException {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Exception must not use an f-string literal, assign to variable first")
    }
}

define_violation!(
    /// ## What it does
    /// Checks for raw usage of `.format` on a string literal in Exception raising.
    ///
    /// ## Why is this bad?
    /// Python includes the line with the raise in the default traceback (and most
    /// other formatters, like Rich and IPython to too).
    ///
    /// ## Example
    ///
    /// This exception
    ///
    /// ```python
    /// sub = "Some value"
    /// raise RuntimeError("'{}' is incorrect".format(sub))
    /// ```
    ///
    /// will produce a traceback like this:
    ///
    /// ```python
    /// Traceback (most recent call last):
    ///   File "tmp.py", line 2, in <module>
    ///     raise RuntimeError("'{}' is incorrect".format(sub))
    /// RuntimeError: 'Some value' is incorrect
    /// ```
    ///
    /// If this is longer or more complex, the duplication can be quite confusing for a
    /// user unaccustomed to reading tracebacks.
    ///
    /// While if you always assign to something like `msg`
    ///
    /// ```python
    /// sub = "Some value"
    /// msg = "'{}' is incorrect".format(sub)
    /// raise RuntimeError(msg)
    /// ```
    ///
    /// then you get:
    ///
    /// ```python
    /// Traceback (most recent call last):
    ///   File "tmp.py", line 3, in <module>
    ///     raise RuntimeError(msg)
    /// RuntimeError: 'Some value' is incorrect
    /// ```
    ///
    /// Now there's a simpler traceback, less code, and no double message. If you have
    /// a long message, this also often formats better when using Black, too.
    pub struct DotFormatInException;
);
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
                    if checker.settings.rules.enabled(&Rule::RawStringInException) {
                        if string.len() > checker.settings.flake8_errmsg.max_string_length {
                            checker.diagnostics.push(Diagnostic::new(
                                RawStringInException,
                                Range::from_located(first),
                            ));
                        }
                    }
                }
                // Check for f-strings
                ExprKind::JoinedStr { .. } => {
                    if checker.settings.rules.enabled(&Rule::FStringInException) {
                        checker.diagnostics.push(Diagnostic::new(
                            FStringInException,
                            Range::from_located(first),
                        ));
                    }
                }
                // Check for .format() calls
                ExprKind::Call { func, .. } => {
                    if checker.settings.rules.enabled(&Rule::DotFormatInException) {
                        if let ExprKind::Attribute { value, attr, .. } = &func.node {
                            if attr == "format" && matches!(value.node, ExprKind::Constant { .. }) {
                                checker.diagnostics.push(Diagnostic::new(
                                    DotFormatInException,
                                    Range::from_located(first),
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
