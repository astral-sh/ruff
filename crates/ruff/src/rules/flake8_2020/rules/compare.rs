use num_bigint::BigInt;
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::super::helpers::is_sys;

/// ## What it does
/// Checks for comparisons of `sys.version` to a string that will evaluate
/// incorrectly on Python 3.10 and later.
///
/// ## Why is this bad?
/// Comparison of `sys.version` to a string is error-prone and may cause subtle
/// bugs.
///
/// Instead, use `sys.version_info` to get the version information as a tuple.
///
/// ## Example
/// ```python
/// import sys
///
/// sys.version > "3.9"  # If using Python 3.10, this evaluates to `False`.
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// sys.version_info > (3, 9)  # If using Python 3.10, this evaluates to `True`.
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersionCmpStr3;

impl Violation for SysVersionCmpStr3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version` compared to string (python3.10), use `sys.version_info`")
    }
}

/// ## What it does
/// Checks for uses of `sys.version_info[0] == 3`.
///
/// ## Why is this bad?
/// In Python 4 and greater, this will evaluate to `False`. This is likely
/// unintended, and may cause code intended to be run on Python 2 to be
/// executed on Python 4.
///
/// Instead, use `>=` to check if the major version number is 3 or greater.
/// This is more future-proof.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info[0] == 3:
///     ...
/// else:
///     print("Python 2")  # This will be printed on Python 4.
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info >= (3,):
///     ...
/// else:
///     print("Python 2")
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersionInfo0Eq3;

impl Violation for SysVersionInfo0Eq3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version_info[0] == 3` referenced (python4), use `>=`")
    }
}

/// ## What it does
/// Checks for comparisons of `sys.version_info[1]` to an integer.
///
/// ## Why is this bad?
/// Comparison of the minor version number to an integer does not consider the
/// major version number. This is likely unintended, and may cause bugs when
/// run on Python 4 and greater.
///
/// Instead, compare `sys.version_info` to a tuple, including the major and
/// minor version numbers. This is more future-proof.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info[1] < 7:
///     print("Python 3.6 or earlier.")  # This will be printed on Python 4.0.
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info < (3, 7):
///     print("Python 3.6 or earlier.")
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersionInfo1CmpInt;

impl Violation for SysVersionInfo1CmpInt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`sys.version_info[1]` compared to integer (python4), compare `sys.version_info` to \
             tuple"
        )
    }
}

/// ## What it does
/// Checks for comparisons of `sys.version_info.minor` to an integer.
///
/// ## Why is this bad?
/// Comparison of the minor version number to an integer is independent of the
/// major version number. This is likely unintended, and may cause bugs when
/// run on Python 4 and greater.
///
/// Instead, compare `sys.version_info` to a tuple, including the major and
/// minor version numbers. This is more future-proof.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info.minor < 7:
///     print("Python 3.6 or earlier.")  # This will be printed on Python 4.0.
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info < (3, 7):
///     print("Python 3.6 or earlier.")
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersionInfoMinorCmpInt;

impl Violation for SysVersionInfoMinorCmpInt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`sys.version_info.minor` compared to integer (python4), compare `sys.version_info` \
             to tuple"
        )
    }
}

/// ## What it does
/// Checks for comparisons of `sys.version` to a string that will evaluate
/// incorrectly on Python 10 and greater.
///
/// ## Why is this bad?
/// Comparison of `sys.version` to a string is error-prone and may cause subtle
/// bugs.
///
/// Instead, use `sys.version_info` to get the version information as a tuple.
///
/// ## Example
/// ```python
/// import sys
///
/// sys.version >= "3"  # This will evaluate to `False` on Python 10.
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// sys.version_info >= (3,)  # This will evaluate to `True` on Python 10.
/// ```
///
/// ## References
/// - [Python documentation: `sys.version`](https://docs.python.org/3/library/sys.html#sys.version)
/// - [Python documentation: `sys.version_info`](https://docs.python.org/3/library/sys.html#sys.version_info)
#[violation]
pub struct SysVersionCmpStr10;

impl Violation for SysVersionCmpStr10 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version` compared to string (python10), use `sys.version_info`")
    }
}

/// YTT103, YTT201, YTT203, YTT204, YTT302
pub(crate) fn compare(checker: &mut Checker, left: &Expr, ops: &[CmpOp], comparators: &[Expr]) {
    match left {
        Expr::Subscript(ast::ExprSubscript { value, slice, .. })
            if is_sys(value, "version_info", checker.semantic()) =>
        {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Int(i),
                ..
            }) = slice.as_ref()
            {
                if *i == BigInt::from(0) {
                    if let (
                        [CmpOp::Eq | CmpOp::NotEq],
                        [Expr::Constant(ast::ExprConstant {
                            value: Constant::Int(n),
                            ..
                        })],
                    ) = (ops, comparators)
                    {
                        if *n == BigInt::from(3) && checker.enabled(Rule::SysVersionInfo0Eq3) {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(SysVersionInfo0Eq3, left.range()));
                        }
                    }
                } else if *i == BigInt::from(1) {
                    if let (
                        [CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE],
                        [Expr::Constant(ast::ExprConstant {
                            value: Constant::Int(_),
                            ..
                        })],
                    ) = (ops, comparators)
                    {
                        if checker.enabled(Rule::SysVersionInfo1CmpInt) {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(SysVersionInfo1CmpInt, left.range()));
                        }
                    }
                }
            }
        }

        Expr::Attribute(ast::ExprAttribute { value, attr, .. })
            if is_sys(value, "version_info", checker.semantic()) && attr == "minor" =>
        {
            if let (
                [CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE],
                [Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(_),
                    ..
                })],
            ) = (ops, comparators)
            {
                if checker.enabled(Rule::SysVersionInfoMinorCmpInt) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersionInfoMinorCmpInt, left.range()));
                }
            }
        }

        _ => {}
    }

    if is_sys(left, "version", checker.semantic()) {
        if let (
            [CmpOp::Lt | CmpOp::LtE | CmpOp::Gt | CmpOp::GtE],
            [Expr::Constant(ast::ExprConstant {
                value: Constant::Str(s),
                ..
            })],
        ) = (ops, comparators)
        {
            if s.len() == 1 {
                if checker.enabled(Rule::SysVersionCmpStr10) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersionCmpStr10, left.range()));
                }
            } else if checker.enabled(Rule::SysVersionCmpStr3) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(SysVersionCmpStr3, left.range()));
            }
        }
    }
}
