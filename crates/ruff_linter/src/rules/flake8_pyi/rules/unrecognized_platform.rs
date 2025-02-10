use ruff_python_ast::{self as ast, CmpOp, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
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
/// understand, and thus result in incorrect inferences by these tools.
/// `sys.platform` checks should be simple string comparisons, like
/// `if sys.platform == "linux"`.
///
/// ## Example
/// ```pyi
/// if sys.platform.startswith("linux"):
///     # Linux specific definitions
///     ...
/// else:
///     # Posix specific definitions
///     ...
/// ```
///
/// Instead, use a simple string comparison, such as `==` or `!=`:
/// ```pyi
/// if sys.platform == "linux":
///     # Linux specific definitions
///     ...
/// else:
///     # Posix specific definitions
///     ...
/// ```
///
/// ## References
/// - [Typing documentation: Version and Platform checking](https://typing.readthedocs.io/en/latest/spec/directives.html#version-and-platform-checks)
#[derive(ViolationMetadata)]
pub(crate) struct UnrecognizedPlatformCheck;

impl Violation for UnrecognizedPlatformCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unrecognized `sys.platform` check".to_string()
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
/// ```pyi
/// if sys.platform == "linus": ...
/// ```
///
/// Use instead:
/// ```pyi
/// if sys.platform == "linux": ...
/// ```
///
/// ## References
/// - [Typing documentation: Version and Platform checking](https://typing.readthedocs.io/en/latest/spec/directives.html#version-and-platform-checks)
#[derive(ViolationMetadata)]
pub(crate) struct UnrecognizedPlatformName {
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
pub(crate) fn unrecognized_platform(checker: &Checker, test: &Expr) {
    let Expr::Compare(ast::ExprCompare {
        left,
        ops,
        comparators,
        ..
    }) = test
    else {
        return;
    };

    let ([op], [right]) = (&**ops, &**comparators) else {
        return;
    };

    if !checker
        .semantic()
        .resolve_qualified_name(left)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["sys", "platform"]))
    {
        return;
    }

    // "in" might also make sense but we don't currently have one.
    if !matches!(op, CmpOp::Eq | CmpOp::NotEq) {
        if checker.enabled(Rule::UnrecognizedPlatformCheck) {
            checker.report_diagnostic(Diagnostic::new(UnrecognizedPlatformCheck, test.range()));
        }
        return;
    }

    if let Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) = right {
        // Other values are possible but we don't need them right now.
        // This protects against typos.
        if checker.enabled(Rule::UnrecognizedPlatformName) {
            if !matches!(value.to_str(), "linux" | "win32" | "cygwin" | "darwin") {
                checker.report_diagnostic(Diagnostic::new(
                    UnrecognizedPlatformName {
                        platform: value.to_string(),
                    },
                    right.range(),
                ));
            }
        }
    } else {
        if checker.enabled(Rule::UnrecognizedPlatformCheck) {
            checker.report_diagnostic(Diagnostic::new(UnrecognizedPlatformCheck, test.range()));
        }
    }
}
