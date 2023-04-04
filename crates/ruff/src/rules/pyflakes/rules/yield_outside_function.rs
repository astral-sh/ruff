use std::fmt;

use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::scope::ScopeKind;

use crate::checkers::ast::Checker;

#[derive(Debug, PartialEq, Eq)]
pub enum DeferralKeyword {
    Yield,
    YieldFrom,
    Await,
}

impl fmt::Display for DeferralKeyword {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeferralKeyword::Yield => fmt.write_str("yield"),
            DeferralKeyword::YieldFrom => fmt.write_str("yield from"),
            DeferralKeyword::Await => fmt.write_str("await"),
        }
    }
}

#[violation]
pub struct YieldOutsideFunction {
    pub keyword: DeferralKeyword,
}

impl Violation for YieldOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let YieldOutsideFunction { keyword } = self;
        format!("`{keyword}` statement outside of a function")
    }
}

pub fn yield_outside_function(checker: &mut Checker, expr: &Expr) {
    if matches!(
        checker.ctx.scope().kind,
        ScopeKind::Class(_) | ScopeKind::Module
    ) {
        let keyword = match expr.node {
            ExprKind::Yield { .. } => DeferralKeyword::Yield,
            ExprKind::YieldFrom { .. } => DeferralKeyword::YieldFrom,
            ExprKind::Await { .. } => DeferralKeyword::Await,
            _ => panic!("Expected ExprKind::Yield | ExprKind::YieldFrom | ExprKind::Await"),
        };
        checker.diagnostics.push(Diagnostic::new(
            YieldOutsideFunction { keyword },
            Range::from(expr),
        ));
    }
}
