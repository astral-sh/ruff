use num_bigint::BigInt;
use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::flake8_2020::helpers::is_sys;

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
pub struct SysVersion0;

impl Violation for SysVersion0 {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`sys.version[0]` referenced (python10), use `sys.version_info`")
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

/// YTT101, YTT102, YTT301, YTT303
pub(crate) fn subscript(checker: &mut Checker, value: &Expr, slice: &Expr) {
    if is_sys(checker.semantic_model(), value, "version") {
        match slice {
            Expr::Slice(ast::ExprSlice {
                lower: None,
                upper: Some(upper),
                step: None,
                range: _,
            }) => {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(i),
                    ..
                }) = upper.as_ref()
                {
                    if *i == BigInt::from(1) && checker.enabled(Rule::SysVersionSlice1) {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(SysVersionSlice1, value.range()));
                    } else if *i == BigInt::from(3) && checker.enabled(Rule::SysVersionSlice3) {
                        checker
                            .diagnostics
                            .push(Diagnostic::new(SysVersionSlice3, value.range()));
                    }
                }
            }

            Expr::Constant(ast::ExprConstant {
                value: Constant::Int(i),
                ..
            }) => {
                if *i == BigInt::from(2) && checker.enabled(Rule::SysVersion2) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersion2, value.range()));
                } else if *i == BigInt::from(0) && checker.enabled(Rule::SysVersion0) {
                    checker
                        .diagnostics
                        .push(Diagnostic::new(SysVersion0, value.range()));
                }
            }

            _ => {}
        }
    }
}
