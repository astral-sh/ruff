use malachite_bigint::BigInt;
use rustpython_parser::ast::{self, Cmpop, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;

use super::super::helpers::is_sys;

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
pub struct SysVersionCmpStr10;

impl Violation for SysVersionCmpStr10 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version` compared to string (python10), use `sys.version_info`")
    }
}

/// YTT103, YTT201, YTT203, YTT204, YTT302
pub(crate) fn compare(checker: &mut Checker, left: &Expr, ops: &[Cmpop], comparators: &[Expr]) {
    match left {
        Expr::Subscript(ast::ExprSubscript { value, slice, .. })
            if is_sys(checker.semantic_model(), value, "version_info") =>
        {
            if let Expr::Constant(ast::ExprConstant {
                value: Constant::Int(i),
                ..
            }) = slice.as_ref()
            {
                if *i == BigInt::from(0) {
                    if let (
                        [Cmpop::Eq | Cmpop::NotEq],
                        [Expr::Constant(ast::ExprConstant {
                            value: Constant::Int(n),
                            ..
                        })],
                    ) = (ops, comparators)
                    {
                        if *n == BigInt::from(3) && checker.enabled(Rule::SysVersionInfo0Eq3) {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(SysVersionInfo0Eq3, left.range()));
                        }
                    }
                } else if *i == BigInt::from(1) {
                    if let (
                        [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
                        [Expr::Constant(ast::ExprConstant {
                            value: Constant::Int(_),
                            ..
                        })],
                    ) = (ops, comparators)
                    {
                        if checker.enabled(Rule::SysVersionInfo1CmpInt) {
                            checker
                                .diagnostics
                                .push(Diagnostic::new(SysVersionInfo1CmpInt, left.range()));
                        }
                    }
                }
            }
        }

        Expr::Attribute(ast::ExprAttribute { value, attr, .. })
            if is_sys(checker.semantic_model(), value, "version_info") && attr == "minor" =>
        {
            if let (
                [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
                [Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(_),
                    ..
                })],
            ) = (ops, comparators)
            {
                if checker.enabled(Rule::SysVersionInfoMinorCmpInt) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersionInfoMinorCmpInt, left.range()));
                }
            }
        }

        _ => {}
    }

    if is_sys(checker.semantic_model(), left, "version") {
        if let (
            [Cmpop::Lt | Cmpop::LtE | Cmpop::Gt | Cmpop::GtE],
            [Expr::Constant(ast::ExprConstant {
                value: Constant::Str(s),
                ..
            })],
        ) = (ops, comparators)
        {
            if s.len() == 1 {
                if checker.enabled(Rule::SysVersionCmpStr10) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersionCmpStr10, left.range()));
                }
            } else if checker.enabled(Rule::SysVersionCmpStr3) {
                checker
                    .diagnostics
                    .push(Diagnostic::new(SysVersionCmpStr3, left.range()));
            }
        }
    }
}
