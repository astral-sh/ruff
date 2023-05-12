use num_bigint::BigInt;
use rustpython_parser::ast::{self, Attributed, Cmpop, Constant, Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

#[violation]
pub struct SysVersionSlice3;

impl Violation for SysVersionSlice3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[:3]` referenced (python3.10), use `sys.version_info`")
    }
}

#[violation]
pub struct SysVersion2;

impl Violation for SysVersion2 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[2]` referenced (python3.10), use `sys.version_info`")
    }
}

#[violation]
pub struct SysVersionCmpStr3;

impl Violation for SysVersionCmpStr3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version` compared to string (python3.10), use `sys.version_info`")
    }
}

#[violation]
pub struct SysVersionInfo0Eq3;

impl Violation for SysVersionInfo0Eq3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version_info[0] == 3` referenced (python4), use `>=`")
    }
}

#[violation]
pub struct SixPY3;

impl Violation for SixPY3 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`six.PY3` referenced (python4), use `not six.PY2`")
    }
}

#[violation]
pub struct SysVersionInfo1CmpInt;

impl Violation for SysVersionInfo1CmpInt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`sys.version_info[1]` compared to integer (python4), compare `sys.version_info` to \
             tuple"
        )
    }
}

#[violation]
pub struct SysVersionInfoMinorCmpInt;

impl Violation for SysVersionInfoMinorCmpInt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`sys.version_info.minor` compared to integer (python4), compare `sys.version_info` \
             to tuple"
        )
    }
}

#[violation]
pub struct SysVersion0;

impl Violation for SysVersion0 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[0]` referenced (python10), use `sys.version_info`")
    }
}

#[violation]
pub struct SysVersionCmpStr10;

impl Violation for SysVersionCmpStr10 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version` compared to string (python10), use `sys.version_info`")
    }
}

#[violation]
pub struct SysVersionSlice1;

impl Violation for SysVersionSlice1 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[:1]` referenced (python10), use `sys.version_info`")
    }
}

fn is_sys(checker: &Checker, expr: &Expr, target: &str) -> bool {
    checker
        .ctx
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == ["sys", target])
}

/// YTT101, YTT102, YTT301, YTT303
pub(crate) fn subscript(checker: &mut Checker, value: &Expr, slice: &Expr) {
    if is_sys(checker, value, "version") {
        match &slice.node {
            ExprKind::Slice(ast::ExprSlice {
                lower: None,
                upper: Some(upper),
                step: None,
            }) => {
                if let ExprKind::Constant(ast::ExprConstant {
                    value: Constant::Int(i),
                    ..
                }) = &upper.node
                {
                    if *i == BigInt::from(1)
                        && checker.settings.rules.enabled(Rule::SysVersionSlice1)
                    {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(SysVersionSlice1, value.range()));
                    } else if *i == BigInt::from(3)
                        && checker.settings.rules.enabled(Rule::SysVersionSlice3)
                    {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(SysVersionSlice3, value.range()));
                    }
                }
            }

            ExprKind::Constant(ast::ExprConstant {
                value: Constant::Int(i),
                ..
            }) => {
                if *i == BigInt::from(2) && checker.settings.rules.enabled(Rule::SysVersion2) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersion2, value.range()));
                } else if *i == BigInt::from(0) && checker.settings.rules.enabled(Rule::SysVersion0)
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersion0, value.range()));
                }
            }

            _ => {}
        }
    }
}

/// YTT103, YTT201, YTT203, YTT204, YTT302
pub(crate) fn compare(checker: &mut Checker, left: &Expr, ops: &[Cmpop], comparators: &[Expr]) {
    match &left.node {
        ExprKind::Subscript(ast::ExprSubscript { value, slice, .. })
            if is_sys(checker, value, "version_info") =>
        {
            if let ExprKind::Constant(ast::ExprConstant {
                value: Constant::Int(i),
                ..
            }) = &slice.node
            {
                if *i == BigInt::from(0) {
                    if let (
                        [Cmpop::Eq | Cmpop::NotEq],
                        [Attributed {
                            node:
                                ExprKind::Constant(ast::ExprConstant {
                                    value: Constant::Int(n),
                                    ..
                                }),
                            ..
                        }],
                    ) = (ops, comparators)
                    {
                        if *n == BigInt::from(3)
                            && checker.settings.rules.enabled(Rule::SysVersionInfo0Eq3)
                        {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(SysVersionInfo0Eq3, left.range()));
                        }
                    }
                } else if *i == BigInt::from(1) {
                    if let (
                        [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
                        [Attributed {
                            node:
                                ExprKind::Constant(ast::ExprConstant {
                                    value: Constant::Int(_),
                                    ..
                                }),
                            ..
                        }],
                    ) = (ops, comparators)
                    {
                        if checker.settings.rules.enabled(Rule::SysVersionInfo1CmpInt) {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(SysVersionInfo1CmpInt, left.range()));
                        }
                    }
                }
            }
        }

        ExprKind::Attribute(ast::ExprAttribute { value, attr, .. })
            if is_sys(checker, value, "version_info") && attr == "minor" =>
        {
            if let (
                [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
                [Attributed {
                    node:
                        ExprKind::Constant(ast::ExprConstant {
                            value: Constant::Int(_),
                            ..
                        }),
                    ..
                }],
            ) = (ops, comparators)
            {
                if checker
                    .settings
                    .rules
                    .enabled(Rule::SysVersionInfoMinorCmpInt)
                {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersionInfoMinorCmpInt, left.range()));
                }
            }
        }

        _ => {}
    }

    if is_sys(checker, left, "version") {
        if let (
            [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
            [Attributed {
                node:
                    ExprKind::Constant(ast::ExprConstant {
                        value: Constant::Str(s),
                        ..
                    }),
                ..
            }],
        ) = (ops, comparators)
        {
            if s.len() == 1 {
                if checker.settings.rules.enabled(Rule::SysVersionCmpStr10) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersionCmpStr10, left.range()));
                }
            } else if checker.settings.rules.enabled(Rule::SysVersionCmpStr3) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(SysVersionCmpStr3, left.range()));
            }
        }
    }
}

/// YTT202
pub(crate) fn name_or_attribute(checker: &mut Checker, expr: &Expr) {
    if checker
        .ctx
        .resolve_call_path(expr)
        .map_or(false, |call_path| call_path.as_slice() == ["six", "PY3"])
    {
        checker
            .diagnostics
            .push(Diagnostic::new(SixPY3, expr.range()));
    }
}
