use std::fmt;

use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};
use serde::{Deserialize, Serialize};

use crate::ast::types::{Range, ScopeKind};
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
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

define_violation!(
    pub struct YieldOutsideFunction {
        pub keyword: DeferralKeyword,
    }
);
impl Violation for YieldOutsideFunction {
    #[derive_message_formats]
    fn message(&self) -> String {
        let YieldOutsideFunction { keyword } = self;
        format!("`{keyword}` statement outside of a function")
    }
}

pub fn yield_outside_function(checker: &mut Checker, expr: &Expr) {
    if matches!(
        checker.current_scope().kind,
        ScopeKind::Class(_) | ScopeKind::Module
    ) {
        let keyword = match expr.node {
            ExprKind::Yield { .. } => DeferralKeyword::Yield,
            ExprKind::YieldFrom { .. } => DeferralKeyword::YieldFrom,
            ExprKind::Await { .. } => DeferralKeyword::Await,
            _ => unreachable!("Expected ExprKind::Yield | ExprKind::YieldFrom | ExprKind::Await"),
        };
        checker.diagnostics.push(Diagnostic::new(
            YieldOutsideFunction { keyword },
            Range::from_located(expr),
        ));
    }
}
