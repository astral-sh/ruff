use ruff_python_ast::{self as ast, CmpOp, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Checks for uses of comparators other than `<` and `>=` for
/// `sys.version_info` checks in `.pyi` files. All other comparators, such
/// as `>`, `<=`, and `==`, are banned.
///
/// ## Why is this bad?
/// Comparing `sys.version_info` with `==` or `<=` has unexpected behavior
/// and can lead to bugs.
///
/// For example, `sys.version_info > (3, 8)` will also match `3.8.10`,
/// while `sys.version_info <= (3, 8)` will _not_ match `3.8.10`:
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
/// ```pyi
/// import sys
///
/// if sys.version_info > (3, 8): ...
/// ```
///
/// Use instead:
/// ```pyi
/// import sys
///
/// if sys.version_info >= (3, 9): ...
/// ```
#[violation]
pub struct BadVersionInfoComparison;

impl Violation for BadVersionInfoComparison {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `<` or `>=` for `sys.version_info` comparisons")
    }
}

/// ## What it does
/// Checks for if-else statements with `sys.version_info` comparisons that use
/// `<` comparators.
///
/// ## Why is this bad?
/// As a convention, branches that correspond to newer Python versions should
/// come first when using `sys.version_info` comparisons. This makes it easier
/// to understand the desired behavior, which typically corresponds to the
/// latest Python versions.
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
#[violation]
pub struct BadVersionInfoOrder;

impl Violation for BadVersionInfoOrder {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `>=` when using `if`-`else` with `sys.version_info` comparisons")
    }
}

/// PYI006, PYI066
pub(crate) fn bad_version_info_comparison(
    checker: &mut Checker,
    test: &Expr,
    has_else_clause: bool,
) {
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
        if checker.enabled(Rule::BadVersionInfoOrder) {
            if has_else_clause {
                checker
                    .diagnostics
                    .push(Diagnostic::new(BadVersionInfoOrder, test.range()));
            }
        }
    } else {
        if checker.enabled(Rule::BadVersionInfoComparison) {
            checker
                .diagnostics
                .push(Diagnostic::new(BadVersionInfoComparison, test.range()));
        };
    }
}
