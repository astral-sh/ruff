use ruff_python_ast::{self as ast, CmpOp, Constant, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;

/// ## What it does
/// Check for unrecognized `sys.platform` checks. Platform checks should be
/// simple string comparisons.
///
/// **Note**: this rule is only enabled in `.pyi` stub files.
///
/// ## Why is this bad?
/// Some `sys.platform` checks are too complex for type checkers to
/// understand, and thus result in false positives. `sys.platform` checks
/// should be simple string comparisons, like `sys.platform == "linux"`.
///
/// ## Example
/// ```python
/// if sys.platform.startswith("linux"):
///     # Linux specific definitions
///     ...
/// else:
///     # Posix specific definitions
///     ...
/// ```
///
/// Instead, use a simple string comparison, such as `==` or `!=`:
/// ```python
/// if sys.platform == "linux":
///     # Linux specific definitions
///     ...
/// else:
///     # Posix specific definitions
///     ...
/// ```
///
/// ## References
/// - [PEP 484](https://peps.python.org/pep-0484/#version-and-platform-checking)
#[violation]
pub struct UnrecognizedPlatformCheck;

impl Violation for UnrecognizedPlatformCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unrecognized `sys.platform` check")
    }
}

/// ## What it does
/// Check for unrecognized platform names in `sys.platform` checks.
///
/// **Note**: this rule is only enabled in `.pyi` stub files.
///
/// ## Why is this bad?
/// If a `sys.platform` check compares to a platform name outside of a
/// small set of known platforms (e.g. "linux", "win32", etc.), it's likely
/// a typo or a platform name that is not recognized by type checkers.
///
/// The list of known platforms is: "linux", "win32", "cygwin", "darwin".
///
/// ## Example
/// ```python
/// if sys.platform == "linus":
///     ...
/// ```
///
/// Use instead:
/// ```python
/// if sys.platform == "linux":
///     ...
/// ```
///
/// ## References
/// - [PEP 484](https://peps.python.org/pep-0484/#version-and-platform-checking)
#[violation]
pub struct UnrecognizedPlatformName {
    platform: String,
}

impl Violation for UnrecognizedPlatformName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnrecognizedPlatformName { platform } = self;
        format!("Unrecognized platform `{platform}`")
    }
}

/// PYI007, PYI008
pub(crate) fn unrecognized_platform(checker: &mut Checker, test: &Expr) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = test
    else {
        return;
    };

    let ([op], [right]) = (ops.as_slice(), comparators.as_slice()) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_call_path(left)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["sys", "platform"]))
    {
        return;
    }

    // "in" might also make sense but we don't currently have one.
    if !matches!(op, CmpOp::Eq | CmpOp::NotEq) {
        if checker.enabled(Rule::UnrecognizedPlatformCheck) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedPlatformCheck, test.range()));
        }
        return;
    }

    if let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(ast::StringConstant { value, .. }),
        ..
    }) = right
    {
        // Other values are possible but we don't need them right now.
        // This protects against typos.
        if checker.enabled(Rule::UnrecognizedPlatformName) {
            if !matches!(value.as_str(), "linux" | "win32" | "cygwin" | "darwin") {
                checker.diagnostics.push(Diagnostic::new(
                    UnrecognizedPlatformName {
                        platform: value.clone(),
                    },
                    right.range(),
                ));
            }
        }
    } else {
        if checker.enabled(Rule::UnrecognizedPlatformCheck) {
            checker
                .diagnostics
                .push(Diagnostic::new(UnrecognizedPlatformCheck, test.range()));
        }
    }
}
