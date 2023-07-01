use num_bigint::BigInt;
use num_traits::{One, Zero};
use rustpython_parser::ast::{self, CmpOp, Constant, Expr, ExprSubscript, Ranged};
use smallvec::SmallVec;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for if statements with complex conditionals in stubs.
///
/// ## Why is this bad?
/// Stub files support simple conditionals to indicate differences between Python versions or
/// platforms, but type checkers only understand a limited subset of Python syntax, and this
/// warning triggers on conditionals that type checkers will probably not understand.
///
/// ## Example
/// ```python
/// if (2, 7) < sys.version_info < (3, 5):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if sys.version_info < (3, 5):
///     ...
/// ```
#[violation]
pub struct ComplexIfStatementInStub;

impl Violation for ComplexIfStatementInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("If test must be a simple comparison against sys.platform or sys.version_info")
    }
}

/// ## What it does
/// Checks for problematic `sys.version_info`-related conditions in stubs.
///
/// ## Why is this bad?
/// Invalid `sys.version_info` checks may do the wrong thing and can be difficult for type
/// checkers to validate.
///
/// ## Example
/// ```python
/// if sys.version_info[0] == "2":
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if sys.version_info[0] == 2:
///     ...
/// ```
#[violation]
pub struct UnrecognizedVersionInfoCheck;

impl Violation for UnrecognizedVersionInfoCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unrecognized sys.version_info check")
    }
}

/// ## What it does
/// Checks for version comparisons that compare against minor version numbers.
///
/// ## Why is this bad?
/// Version comparison must use only major and minor version. Type checkers like mypy don't know
/// about patch versions of Python (e.g. 3.4.3 versus 3.4.4), only major and minor versions
/// (e.g., 3.3 versus 3.4). Therefore, version checks in stubs should only use the major and
/// minor versions. If new functionality was introduced in a patch version, you may assume that
/// it was there all along.
///
/// ## Example
/// ```python
/// if sys.version_info >= (3, 4, 3):
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if sys.version_info >= (3, 4):
///     ...
/// ```
#[violation]
pub struct TooSpecificVersionComparison;

impl Violation for TooSpecificVersionComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Version comparison must use only major and minor version")
    }
}

/// ## What it does
/// Checks for version comparisons that compare against the wrong length tuple.
///
/// ## Why is this bad?
/// This may cause undesired behavior due to a mismatch in length between the two arguments of the
/// comparison.
///
/// ## Example
/// ```python
/// if sys.version_info[:2] == (3,):
///     ...
/// ```
///
/// Use instead:
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

// PYI002, PYI003, PYI004, PYI005
pub(crate) fn version_info_checks(checker: &mut Checker, test: &Expr) {
    let Some((left, op, comparator, is_platform)) = compare_expr_components(checker, test) else {
        if checker.enabled(Rule::ComplexIfStatementInStub) {
            checker
                .diagnostics
                .push(Diagnostic::new(ComplexIfStatementInStub, test.range()));
        }
        return;
    };

    if is_platform {
        // covered by PYI007
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
fn compare_expr_components<'b>(
    checker: &Checker,
    test: &'b Expr,
) -> Option<(&'b Expr, CmpOp, &'b Expr, bool)> {
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
    // Single digit comparison
    if expected_comparator == ExpectedComparator::MajorDigit {
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) && !is_int_constant(comparator) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
        return;
    }

    // Tuple comparison
    let Expr::Tuple(ast::ExprTuple { elts, .. }) = comparator else {
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
        return;
    };

    if !elts.iter().all(is_int_constant) {
        // All tuple elements must be integers
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
    } else if checker.enabled(Rule::TooSpecificVersionComparison) && elts.len() > 2 {
        // Must compare against major and minor version only
        checker
            .diagnostics
            .push(Diagnostic::new(TooSpecificVersionComparison, test.range()));
    }

    // Validate tuple length
    if checker.enabled(Rule::WrongTupleLengthVersionComparison)
        && (op == CmpOp::Eq || op == CmpOp::NotEq)
    {
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

impl TryFrom<&Expr> for ExpectedComparator {
    type Error = ();

    fn try_from(value: &Expr) -> Result<Self, Self::Error> {
        let Expr::Subscript(ExprSubscript { slice, .. }) = value else {
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
