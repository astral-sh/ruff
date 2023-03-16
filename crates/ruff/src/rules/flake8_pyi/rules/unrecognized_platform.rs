use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

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
    pub platform: String,
}

impl Violation for UnrecognizedPlatformName {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnrecognizedPlatformName { platform } = self;
        format!("Unrecognized platform `{platform}`")
    }
}

/// PYI007, PYI008
pub fn unrecognized_platform(
    checker: &mut Checker,
    expr: &Expr,
    left: &Expr,
    ops: &[Cmpop],
    comparators: &[Expr],
) {
    let ([op], [right]) = (ops, comparators) else {
        return;
    };

    let diagnostic_unrecognized_platform_check =
        Diagnostic::new(UnrecognizedPlatformCheck, Range::from(expr));
    if !checker
        .ctx
        .resolve_call_path(left)
        .map_or(false, |call_path| {
            call_path.as_slice() == ["sys", "platform"]
        })
    {
        return;
    }

    // "in" might also make sense but we don't currently have one.
    if !matches!(op, Cmpop::Eq | Cmpop::NotEq)
        && checker
            .settings
            .rules
            .enabled(Rule::UnrecognizedPlatformCheck)
    {
        checker
            .diagnostics
            .push(diagnostic_unrecognized_platform_check);
        return;
    }

    match &right.node {
        ExprKind::Constant {
            value: Constant::Str(value),
            ..
        } => {
            // Other values are possible but we don't need them right now.
            // This protects against typos.
            if !["linux", "win32", "cygwin", "darwin"].contains(&value.as_str())
                && checker
                    .settings
                    .rules
                    .enabled(Rule::UnrecognizedPlatformName)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UnrecognizedPlatformName {
                        platform: value.clone(),
                    },
                    Range::from(right),
                ));
            }
        }
        _ => {
            if checker
                .settings
                .rules
                .enabled(Rule::UnrecognizedPlatformCheck)
            {
                checker
                    .diagnostics
                    .push(diagnostic_unrecognized_platform_check);
            }
        }
    }
}
