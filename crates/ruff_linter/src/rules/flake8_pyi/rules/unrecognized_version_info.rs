use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::helpers::map_subscript;
use ruff_python_ast::{self as ast, CmpOp, Expr, Int};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

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
/// ```pyi
/// import sys
///
/// if sys.version_info[0] == "2": ...
/// ```
///
/// Use instead:
/// ```pyi
/// import sys
///
/// if sys.version_info[0] == 2: ...
/// ```
///
/// ## References
/// - [Typing documentation: Version and Platform checking](https://typing.readthedocs.io/en/latest/spec/directives.html#version-and-platform-checks)
#[derive(ViolationMetadata)]
pub(crate) struct UnrecognizedVersionInfoCheck;

impl Violation for UnrecognizedVersionInfoCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unrecognized `sys.version_info` check".to_string()
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
/// ```pyi
/// import sys
///
/// if sys.version_info >= (3, 4, 3): ...
/// ```
///
/// Use instead:
/// ```pyi
/// import sys
///
/// if sys.version_info >= (3, 4): ...
/// ```
///
/// ## References
/// - [Typing documentation: Version and Platform checking](https://typing.readthedocs.io/en/latest/spec/directives.html#version-and-platform-checks)
#[derive(ViolationMetadata)]
pub(crate) struct PatchVersionComparison;

impl Violation for PatchVersionComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Version comparison must use only major and minor version".to_string()
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
/// ```pyi
/// import sys
///
/// if sys.version_info[:2] == (3,): ...
/// ```
///
/// Use instead:
/// ```pyi
/// import sys
///
/// if sys.version_info[0] == 3: ...
/// ```
///
/// ## References
/// - [Typing documentation: Version and Platform checking](https://typing.readthedocs.io/en/latest/spec/directives.html#version-and-platform-checks)
#[derive(ViolationMetadata)]
pub(crate) struct WrongTupleLengthVersionComparison {
    expected_length: usize,
}

impl Violation for WrongTupleLengthVersionComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WrongTupleLengthVersionComparison { expected_length } = self;
        format!("Version comparison must be against a length-{expected_length} tuple")
    }
}

/// PYI003, PYI004, PYI005
pub(crate) fn unrecognized_version_info(checker: &Checker, test: &Expr) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = test
    else {
        return;
    };

    let ([op], [comparator]) = (&**ops, &**comparators) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(map_subscript(left))
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["sys", "version_info"]))
    {
        return;
    }

    if let Some(expected) = ExpectedComparator::try_from(left) {
        version_check(checker, expected, test, *op, comparator);
    } else {
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker.report_diagnostic(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
    }
}

fn version_check(
    checker: &Checker,
    expected: ExpectedComparator,
    test: &Expr,
    op: CmpOp,
    comparator: &Expr,
) {
    // Single digit comparison, e.g., `sys.version_info[0] == 2`.
    if expected == ExpectedComparator::MajorDigit {
        if !is_int_constant(comparator) {
            if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
                checker
                    .report_diagnostic(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
            }
        }
        return;
    }

    // Tuple comparison, e.g., `sys.version_info == (3, 4)`.
    let Expr::Tuple(tuple) = comparator else {
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker.report_diagnostic(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
        return;
    };

    if !tuple.iter().all(is_int_constant) {
        // All tuple elements must be integers, e.g., `sys.version_info == (3, 4)` instead of
        // `sys.version_info == (3.0, 4)`.
        if checker.enabled(Rule::UnrecognizedVersionInfoCheck) {
            checker.report_diagnostic(Diagnostic::new(UnrecognizedVersionInfoCheck, test.range()));
        }
    } else if tuple.len() > 2 {
        // Must compare against major and minor version only, e.g., `sys.version_info == (3, 4)`
        // instead of `sys.version_info == (3, 4, 0)`.
        if checker.enabled(Rule::PatchVersionComparison) {
            checker.report_diagnostic(Diagnostic::new(PatchVersionComparison, test.range()));
        }
    }

    if checker.enabled(Rule::WrongTupleLengthVersionComparison) {
        if op == CmpOp::Eq || op == CmpOp::NotEq {
            let expected_length = match expected {
                ExpectedComparator::MajorTuple => 1,
                ExpectedComparator::MajorMinorTuple => 2,
                _ => return,
            };

            if tuple.len() != expected_length {
                checker.report_diagnostic(Diagnostic::new(
                    WrongTupleLengthVersionComparison { expected_length },
                    test.range(),
                ));
            }
        }
    }
}

#[derive(Copy, Clone, Eq, PartialEq)]
enum ExpectedComparator {
    MajorDigit,
    MajorTuple,
    MajorMinorTuple,
    AnyTuple,
}

impl ExpectedComparator {
    /// Returns the expected comparator for the given expression, if any.
    fn try_from(expr: &Expr) -> Option<Self> {
        let Expr::Subscript(ast::ExprSubscript { slice, .. }) = expr else {
            return Some(ExpectedComparator::AnyTuple);
        };

        // Only allow: (1) simple slices of the form `[:n]`, or (2) explicit indexing into the first
        // element (major version) of the tuple.
        match slice.as_ref() {
            Expr::Slice(ast::ExprSlice {
                lower: None,
                upper: Some(upper),
                step: None,
                ..
            }) => {
                if let Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Int(upper),
                    ..
                }) = upper.as_ref()
                {
                    if *upper == 1 {
                        return Some(ExpectedComparator::MajorTuple);
                    }
                    if *upper == 2 {
                        return Some(ExpectedComparator::MajorMinorTuple);
                    }
                }
            }
            Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Int(Int::ZERO),
                ..
            }) => {
                return Some(ExpectedComparator::MajorDigit);
            }
            _ => (),
        }

        None
    }
}

/// Returns `true` if the given expression is an integer constant.
fn is_int_constant(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: ast::Number::Int(_),
            ..
        })
    )
}
