use rustpython_parser::ast::{self, Expr, Ranged};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::{test_expression, Resolution};

#[violation]
pub struct PandasUseOfDotIx;

impl Violation for PandasUseOfDotIx {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.ix` is deprecated; use more explicit `.loc` or `.iloc`")
    }
}

#[violation]
pub struct PandasUseOfDotAt;

impl Violation for PandasUseOfDotAt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.loc` instead of `.at`. If speed is important, use NumPy.")
    }
}

#[violation]
pub struct PandasUseOfDotIat;

impl Violation for PandasUseOfDotIat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.iloc` instead of `.iat`. If speed is important, use NumPy.")
    }
}

pub(crate) fn subscript(checker: &mut Checker, value: &Expr, expr: &Expr) {
    let Expr::Attribute(ast::ExprAttribute { attr, value, .. }) = value else {
        return;
    };

    let rules = &checker.settings.rules;
    let violation: DiagnosticKind = match attr.as_str() {
        "ix" if rules.enabled(Rule::PandasUseOfDotIx) => PandasUseOfDotIx.into(),
        "at" if rules.enabled(Rule::PandasUseOfDotAt) => PandasUseOfDotAt.into(),
        "iat" if rules.enabled(Rule::PandasUseOfDotIat) => PandasUseOfDotIat.into(),
        _ => return,
    };

    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.at[0]`), and on irrelevant bindings
    // (like imports).
    if !matches!(
        test_expression(value, checker.semantic()),
        Resolution::RelevantLocal
    ) {
        return;
    }

    checker
        .diagnostics
        .push(Diagnostic::new(violation, expr.range()));
}
