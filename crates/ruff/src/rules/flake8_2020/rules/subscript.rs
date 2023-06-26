use num_bigint::BigInt;
use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::flake8_2020::helpers::is_sys;

/// ## What it does
/// Checks for uses of `sys.version[:3]`.
///
/// ## Why is this bad?
/// `sys.version[:3]` will select the first three characters of the version
/// string. If the major or minor version number consists of two digits, this
/// will truncate the version number (e.g., `"3.10"` becomes `"3.1"`). This is
/// likely unintended, and will cause problems if the version string is used to
/// check for a specific version.
///
/// Instead, use `sys.version_info` to get the version information as a tuple.
/// This is more future-proof and less error-prone.
///
/// ## Example
/// ```python
/// import sys
///
/// sys.version[:3]  # If using Python 3.10, this evaluates to "3.1".
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// "{}.{}".format(*sys.version_info)  # If using Python 3.10, this evaluates to "3.10".
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersionSlice3;

impl Violation for SysVersionSlice3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[:3]` referenced (python3.10), use `sys.version_info`")
    }
}

/// ## What it does
/// Checks for uses of `sys.version[2]`.
///
/// ## Why is this bad?
/// `sys.version[2]` will select the third character of the version string.
/// If the major or minor version number consists of two digits, this will
/// select the first digit of the minor number only (e.g., `"3.10"` becomes
/// `"1"`). This is likely unintended, and will cause problems if the version
/// string is used to check for a minor version number.
///
/// Instead, use `sys.version_info.minor` to get the minor version number. This
/// is more future-proof and less error-prone.
///
/// ## Example
/// ```python
/// import sys
///
/// sys.version[2]  # If using Python 3.10, this evaluates to "1".
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// f"{sys.version_info.minor}"  # If using Python 3.10, this evaluates to "10".
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersion2;

impl Violation for SysVersion2 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[2]` referenced (python3.10), use `sys.version_info`")
    }
}

/// ## What it does
/// Checks for uses of `sys.version[0]`.
///
/// ## Why is this bad?
/// `sys.version[0]` will select the first character of the version string.
/// If the major version number consists of more than one digit, this will
/// select the first digit of the major version number only (e.g., `"10.0"`
/// becomes `"1"`). This is likely unintended, and will cause problems in
/// future versions of Python if the version string is used to check for a
/// major version number.
///
/// Instead, use `sys.version_info.major` to get the major version number. This
/// is more future-proof and less error-prone.
///
/// ## Example
/// ```python
/// import sys
///
/// sys.version[0]  # If using Python 10, this evaluates to "1".
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// f"{sys.version_info.major}"  # If using Python 10, this evaluates to "10".
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersion0;

impl Violation for SysVersion0 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[0]` referenced (python10), use `sys.version_info`")
    }
}

/// ## What it does
/// Checks for uses of `sys.version[:1]`.
///
/// ## Why is this bad?
/// `sys.version[:1]` will select the first character of the version string.
/// If the major version number consists of more than one digit, this will
/// select the first digit of the major version number only (e.g., `"10.0"`
/// becomes `"1"`). This is likely unintended, and will cause problems in
/// future versions of Python if the version string is used to check for a
/// major version number.
///
/// Instead, use `sys.version_info.major` to get the major version number. This
/// is more future-proof and less error-prone.
///
/// ## Example
/// ```python
/// import sys
///
/// sys.version[:1]  # If using Python 10, this evaluates to "1".
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// f"{sys.version_info.major}"  # If using Python 10, this evaluates to "10".
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersionSlice1;

impl Violation for SysVersionSlice1 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[:1]` referenced (python10), use `sys.version_info`")
    }
}

/// YTT101, YTT102, YTT301, YTT303
pub(crate) fn subscript(checker: &mut Checker, value: &Expr, slice: &Expr) {
    if is_sys(value, "version", checker.semantic()) {
        match slice {
            Expr::Slice(ast::ExprSlice {
                lower: None,
                upper: Some(upper),
                step: None,
                range: _,
            }) => {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(i),
                    ..
                }) = upper.as_ref()
                {
                    if *i == BigInt::from(1) && checker.enabled(Rule::SysVersionSlice1) {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(SysVersionSlice1, value.range()));
                    } else if *i == BigInt::from(3) && checker.enabled(Rule::SysVersionSlice3) {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(SysVersionSlice3, value.range()));
                    }
                }
            }

            Expr::Constant(ast::ExprConstant {
                value: Constant::Int(i),
                ..
            }) => {
                if *i == BigInt::from(2) && checker.enabled(Rule::SysVersion2) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersion2, value.range()));
                } else if *i == BigInt::from(0) && checker.enabled(Rule::SysVersion0) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersion0, value.range()));
                }
            }

            _ => {}
        }
    }
}
