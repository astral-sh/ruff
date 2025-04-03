use ruff_python_ast::{self as ast, CmpOp, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for uses of comparators other than `<` and `>=` for
/// `sys.version_info` checks. All other comparators, such
/// as `>`, `<=`, and `==`, are banned.
///
/// ## Why is this bad?
/// Comparing `sys.version_info` with `==` or `<=` has unexpected behavior
/// and can lead to bugs.
///
/// For example, `sys.version_info > (3, 8, 1)` will resolve to `True` if your
/// Python version is 3.8.1; meanwhile, `sys.version_info <= (3, 8)` will _not_
/// resolve to `True` if your Python version is 3.8.10:
///
/// ```python
/// >>> import sys
/// >>> print(sys.version_info)
/// sys.version_info(major=3, minor=8, micro=10, releaselevel='final', serial=0)
/// >>> print(sys.version_info > (3, 8))
/// True
/// >>> print(sys.version_info == (3, 8))
/// False
/// >>> print(sys.version_info <= (3, 8))
/// False
/// >>> print(sys.version_info in (3, 8))
/// False
/// ```
///
/// ## Example
/// ```py
/// import sys
///
/// if sys.version_info > (3, 8): ...
/// ```
///
/// Use instead:
/// ```py
/// import sys
///
/// if sys.version_info >= (3, 9): ...
/// ```
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct BadVersionInfoComparison;

impl Violation for BadVersionInfoComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use `<` or `>=` for `sys.version_info` comparisons".to_string()
    }
}

/// ## What it does
/// Checks for code that branches on `sys.version_info` comparisons where
/// branches corresponding to older Python versions come before branches
/// corresponding to newer Python versions.
///
/// ## Why is this bad?
/// As a convention, branches that correspond to newer Python versions should
/// come first. This makes it easier to understand the desired behavior, which
/// typically corresponds to the latest Python versions.
///
/// This rule enforces the convention by checking for `if` tests that compare
/// `sys.version_info` with `<` rather than `>=`.
///
/// By default, this rule only applies to stub files.
/// In [preview], it will also flag this anti-pattern in non-stub files.
///
/// ## Example
///
/// ```pyi
/// import sys
///
/// if sys.version_info < (3, 10):
///     def read_data(x, *, preserve_order=True): ...
///
/// else:
///     def read_data(x): ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// if sys.version_info >= (3, 10):
///     def read_data(x): ...
///
/// else:
///     def read_data(x, *, preserve_order=True): ...
/// ```
///
/// [preview]: https://docs.astral.sh/ruff/preview/
#[derive(ViolationMetadata)]
pub(crate) struct BadVersionInfoOrder;

impl Violation for BadVersionInfoOrder {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Put branches for newer Python versions first when branching on `sys.version_info` comparisons".to_string()
    }
}

/// PYI006, PYI066
pub(crate) fn bad_version_info_comparison(checker: &Checker, test: &Expr, has_else_clause: bool) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = test
    else {
        return;
    };

    let ([op], [_right]) = (&**ops, &**comparators) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(left)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["sys", "version_info"]))
    {
        return;
    }

    if matches!(op, CmpOp::GtE) {
        // No issue to be raised, early exit.
        return;
    }

    if matches!(op, CmpOp::Lt) {
        if checker.enabled(Rule::BadVersionInfoOrder)
            // See https://github.com/astral-sh/ruff/issues/15347
            && (checker.source_type.is_stub() || checker.settings.preview.is_enabled())
        {
            if has_else_clause {
                checker.report_diagnostic(Diagnostic::new(BadVersionInfoOrder, test.range()));
            }
        }
    } else {
        if checker.enabled(Rule::BadVersionInfoComparison) {
            checker.report_diagnostic(Diagnostic::new(BadVersionInfoComparison, test.range()));
        }
    }
}
