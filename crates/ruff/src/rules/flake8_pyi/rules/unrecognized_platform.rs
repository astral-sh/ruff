use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;

define_violation!(
    /// ## What it does
    /// Check for unrecognized `sys.platform` checks. Platform checks should be
    /// simple string comparisons.
    ///
    /// > **Note**
    /// >
    /// > This rule only supports the stub file.
    ///
    /// ## Why is this bad?
    /// Some checks are too complex for type checkers to understand. Please use
    /// simple string comparisons. Such as `sys.platform == "linux"`.
    ///
    /// ## Example
    /// Use a simple string comparison instead. Such as `==` or `!=`.
    /// ```python
    /// if sys.platform == 'win32':
    ///     # Windows specific definitions
    /// else:
    ///     # Posix specific definitions
    /// ```
    pub struct UnrecognizedPlatformCheck;
);
impl Violation for UnrecognizedPlatformCheck {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unrecognized sys.platform check")
    }
}

define_violation!(
    /// ## What it does
    /// Check for unrecognized platform names in `sys.platform` checks.
    ///
    /// > **Note**
    /// >
    /// > This rule only supports the stub file.
    ///
    /// ## Why is this bad?
    /// To prevent you from typos, we warn if you use a platform name outside a
    /// small set of known platforms (e.g. "linux" and "win32").
    ///
    /// ## Example
    /// Use a platform name from the list of known platforms. Currently, the
    /// list of known platforms is: "linux", "win32", "cygwin", "darwin".
    /// ```python
    /// if sys.platform == 'win32':
    ///    # Windows specific definitions
    /// else:
    ///    # Posix specific definitions
    /// ```
    pub struct UnrecognizedPlatformName {
        pub platform: String,
    }
);
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
        Diagnostic::new(UnrecognizedPlatformCheck, Range::from_located(expr));
    if !checker.resolve_call_path(left).map_or(false, |call_path| {
        call_path.as_slice() == ["sys", "platform"]
    }) {
        return;
    }

    // "in" might also make sense but we don't currently have one
    if !matches!(op, Cmpop::Eq | Cmpop::NotEq)
        && checker
            .settings
            .rules
            .enabled(&Rule::UnrecognizedPlatformCheck)
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
            // other values are possible but we don't need them right now
            // this protects against typos
            if !["linux", "win32", "cygwin", "darwin"].contains(&value.as_str())
                && checker
                    .settings
                    .rules
                    .enabled(&Rule::UnrecognizedPlatformName)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UnrecognizedPlatformName {
                        platform: value.clone(),
                    },
                    Range::from_located(right),
                ));
            }
        }
        _ => {
            if checker
                .settings
                .rules
                .enabled(&Rule::UnrecognizedPlatformCheck)
            {
                checker
                    .diagnostics
                    .push(diagnostic_unrecognized_platform_check);
            }
        }
    }
}
