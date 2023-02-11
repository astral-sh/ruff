use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Cmpop, Constant, Expr, ExprKind};
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::{Diagnostic, Rule};
use crate::violation::Violation;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VarKind {
    TypeVar,
    ParamSpec,
    TypeVarTuple,
}

impl fmt::Display for VarKind {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VarKind::TypeVar => fmt.write_str("TypeVar"),
            VarKind::ParamSpec => fmt.write_str("ParamSpec"),
            VarKind::TypeVarTuple => fmt.write_str("TypeVarTuple"),
        }
    }
}

define_violation!(
    /// ### What it does
    ///
    /// xxxx
    ///
    /// ## Example
    ///
    /// ```python
    /// from typing import TypeVar
    ///
    /// T = TypeVar("T")
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from typing import TypeVar
    ///
    /// _T = TypeVar("_T")
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
    /// ### What it does
    /// xxxx
    ///
    /// ## Example
    /// ```python
    /// from typing import TypeVar
    ///
    /// T = TypeVar("T")
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// from typing import TypeVar
    ///
    /// _T = TypeVar("_T")
    /// ```
    pub struct UnrecognizedPlatformValue {
        pub platform: String,
    }
);
impl Violation for UnrecognizedPlatformValue {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnrecognizedPlatformValue { platform } = self;
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
    }; // TODO

    if !checker.resolve_call_path(left).map_or(false, |call_path| {
        call_path.as_slice() == ["sys", "platform"]
    }) {
        return;
    }

    if !matches!(op, Cmpop::Eq | Cmpop::NotEq)
        && checker
            .settings
            .rules
            .enabled(&Rule::UnrecognizedPlatformCheck)
    {
        checker.diagnostics.push(Diagnostic::new(
            UnrecognizedPlatformCheck,
            Range::from_located(expr),
        ));
        return;
    }

    match &right.node {
        ExprKind::Constant {
            value: Constant::Str(value),
            ..
        } => {
            if !["linux", "win32", "cygwin", "darwin"].contains(&value.as_str())
                && checker
                    .settings
                    .rules
                    .enabled(&Rule::UnrecognizedPlatformValue)
            {
                checker.diagnostics.push(Diagnostic::new(
                    UnrecognizedPlatformValue {
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
                checker.diagnostics.push(Diagnostic::new(
                    UnrecognizedPlatformCheck,
                    Range::from_located(expr),
                ));
            }
        }
    }
}
