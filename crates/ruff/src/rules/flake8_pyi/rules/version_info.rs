use num_bigint::BigInt;
use num_traits::{One, Zero};
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, Ranged};
use smallvec::SmallVec;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for `if` statements with complex conditionals in stubs.
///
/// ## Why is this bad?
/// Stub files support simple conditionals to test for differences in Python
/// versions and platforms. However, type checkers only understand a limited
/// subset of these conditionals; complex conditionals may result in false
/// positives or false negatives.
///
/// ## Example
/// ```python
/// import sys
///
/// if (2, 7) < sys.version_info < (3, 5):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info < (3, 5):
///     ...
/// ```
#[violation]
pub struct ComplexIfStatementInStub;

impl Violation for ComplexIfStatementInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`if`` test must be a simple comparison against `sys.platform` or `sys.version_info`"
        )
    }
}

/// ## What it does
/// Checks for problematic `sys.version_info`-related conditions in stubs.
///
/// ## Why is this bad?
/// Stub files support simple conditionals to test for differences in Python
/// versions using `sys.version_info`. However, there are a number of common
/// mistakes involving `sys.version_info` comparisons that should be avoided.
/// For example, comparing against a string can lead to unexpected behavior.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info[0] == "2":
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info[0] == 2:
///     ...
/// ```
#[violation]
pub struct UnrecognizedVersionInfoCheck;

impl Violation for UnrecognizedVersionInfoCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unrecognized `sys.version_info` check")
    }
}

/// ## What it does
/// Checks for Python version comparisons in stubs that compare against patch
/// versions (e.g., Python 3.8.3) instead of major and minor versions (e.g.,
/// Python 3.8).
///
/// ## Why is this bad?
/// Stub files support simple conditionals to test for differences in Python
/// versions and platforms. However, type checkers only understand a limited
/// subset of these conditionals. In particular, type checkers don't support
/// patch versions (e.g., Python 3.8.3), only major and minor versions (e.g.,
/// Python 3.8). Therefore, version checks in stubs should only use the major
/// and minor versions.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info >= (3, 4, 3):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info >= (3, 4):
///     ...
/// ```
#[violation]
pub struct PatchVersionComparison;

impl Violation for PatchVersionComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Version comparison must use only major and minor version")
    }
}

/// ## What it does
/// Checks for Python version comparisons that compare against a tuple of the
/// wrong length.
///
/// ## Why is this bad?
/// Stub files support simple conditionals to test for differences in Python
/// versions and platforms. When comparing against `sys.version_info`, avoid
/// comparing against tuples of the wrong length, which can lead to unexpected
/// behavior.
///
/// ## Example
/// ```python
/// import sys
///
/// if sys.version_info[:2] == (3,):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// import sys
///
/// if sys.version_info[0] == 3:
///     ...
/// ```
#[violation]
pub struct WrongTupleLengthVersionComparison {
    expected_length: usize,
}

impl Violation for WrongTupleLengthVersionComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Version comparison must be against a length-{} tuple.",
            self.expected_length
        )
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum ExpectedComparator {
    MajorDigit,
    MajorTuple,
    MajorMinorTuple,
    AnyTuple,
}

/// PYI002, PYI003, PYI004, PYI005
pub(crate) fn version_info(checker: &mut Checker, test: &Expr) {
    if let Expr::BoolOp(ast::ExprBoolOp { values, .. }) = test {
        for value in values {
            version_info(checker, value);
        }
        return;
    }

    let Some((left, op, comparator, is_platform)) = compare_expr_components(checker, test) else {
        if checker.enabled(Rule::ComplexIfStatementInStub) {
            checker
                .diagnostics
                .push(Diagnostic::new(ComplexIfStatementInStub, test.range()));
        }
        return;
    };

    // Already covered by PYI007.
    if is_platform {
        return;
    }

    let Ok(expected_comparator) = ExpectedComparator::try_from(left) else {
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
        return;
    };

    check_version_check(checker, expected_comparator, test, op, comparator);
}

/// Extracts relevant components of the if test.
fn compare_expr_components<'a>(
    checker: &Checker,
    test: &'a Expr,
) -> Option<(&'a Expr, CmpOp, &'a Expr, bool)> {
    test.as_compare_expr().and_then(|cmp| {
        let ast::ExprCompare {
            left,
            ops,
            comparators,
            ..
        } = cmp;

        if comparators.len() != 1 {
            return None;
        }

        let name_expr = if let Expr::Subscript(ast::ExprSubscript { value, .. }) = left.as_ref() {
            value
        } else {
            left
        };

        // The only valid comparisons are against sys.platform and sys.version_info.
        let is_platform = match checker
            .semantic()
            .resolve_call_path(name_expr)
            .as_ref()
            .map(SmallVec::as_slice)
        {
            Some(["sys", "platform"]) => true,
            Some(["sys", "version_info"]) => false,
            _ => return None,
        };

        Some((left.as_ref(), ops[0], &comparators[0], is_platform))
    })
}

fn check_version_check(
    checker: &mut Checker,
    expected_comparator: ExpectedComparator,
    test: &Expr,
    op: CmpOp,
    comparator: &Expr,
) {
    // Single digit comparison, e.g., `sys.version_info[0] == 2`.
    if expected_comparator == ExpectedComparator::MajorDigit {
        if !is_int_constant(comparator) {
            if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
            }
        }
        return;
    }

    // Tuple comparison, e.g., `sys.version_info == (3, 4)`.
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = comparator else {
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
        return;
    };

    if !elts.iter().all(is_int_constant) {
        // All tuple elements must be integers, e.g., `sys.version_info == (3, 4)` instead of
        // `sys.version_info == (3.0, 4)`.
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
    } else if elts.len() > 2 {
        // Must compare against major and minor version only, e.g., `sys.version_info == (3, 4)`
        // instead of `sys.version_info == (3, 4, 0)`.
        if checker.enabled(Rule::PatchVersionComparison) {
            checker
                .diagnostics
                .push(Diagnostic::new(PatchVersionComparison, test.range()));
        }
    }

    if checker.enabled(Rule::WrongTupleLengthVersionComparison) {
        if op == CmpOp::Eq || op == CmpOp::NotEq {
            let expected_length = match expected_comparator {
                ExpectedComparator::MajorTuple => 1,
                ExpectedComparator::MajorMinorTuple => 2,
                _ => return,
            };

            if elts.len() != expected_length {
                checker.diagnostics.push(Diagnostic::new(
                    WrongTupleLengthVersionComparison { expected_length },
                    test.range(),
                ));
            }
        }
    }
}

impl TryFrom<&Expr> for ExpectedComparator {
    type Error = ();

    fn try_from(value: &Expr) -> Result<Self, Self::Error> {
        let Expr::Subscript(ast::ExprSubscript { slice, .. }) = value else {
            return Ok(ExpectedComparator::AnyTuple)
        };

        // Only allow simple slices of the form [:n] or explicit indexing into the first element
        match slice.as_ref() {
            Expr::Slice(ast::ExprSlice {
                lower: None,
                upper: Some(n),
                step: None,
                ..
            }) => {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(n),
                    ..
                }) = n.as_ref()
                {
                    if *n == BigInt::one() {
                        return Ok(ExpectedComparator::MajorTuple);
                    }
                    if *n == BigInt::from(2) {
                        return Ok(ExpectedComparator::MajorMinorTuple);
                    }
                }
            }
            Expr::Constant(ast::ExprConstant {
                value: Constant::Int(n),
                ..
            }) if n.is_zero() => {
                return Ok(ExpectedComparator::MajorDigit);
            }
            _ => (),
        }

        Err(())
    }
}

fn is_int_constant(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Constant(ast::ExprConstant {
            value: ast::Constant::Int(_),
            ..
        })
    )
}
