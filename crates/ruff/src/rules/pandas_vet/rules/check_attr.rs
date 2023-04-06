use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::Violation;
use ruff_diagnostics::{Diagnostic, DiagnosticKind};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;
use ruff_python_semantic::binding::BindingKind;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::pandas_vet::helpers::is_dataframe_candidate;

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
        format!("Use `.loc` instead of `.at`.  If speed is important, use numpy.")
    }
}

#[violation]
pub struct PandasUseOfDotIat;

impl Violation for PandasUseOfDotIat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.iloc` instead of `.iat`.  If speed is important, use numpy.")
    }
}

#[violation]
pub struct PandasUseOfDotValues;

impl Violation for PandasUseOfDotValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.to_numpy()` instead of `.values`")
    }
}

pub fn check_attr(checker: &mut Checker, attr: &str, value: &Expr, attr_expr: &Expr) {
    let rules = &checker.settings.rules;
    let violation: DiagnosticKind = match attr {
        "ix" if rules.enabled(Rule::PandasUseOfDotIx) => PandasUseOfDotIx.into(),
        "at" if rules.enabled(Rule::PandasUseOfDotAt) => PandasUseOfDotAt.into(),
        "iat" if rules.enabled(Rule::PandasUseOfDotIat) => PandasUseOfDotIat.into(),
        "values" if rules.enabled(Rule::PandasUseOfDotValues) => PandasUseOfDotValues.into(),
        _ => return,
    };

    // Avoid flagging on function calls (e.g., `df.values()`).
    if let Some(parent) = checker.ctx.current_expr_parent() {
        if matches!(parent.node, ExprKind::Call { .. }) {
            return;
        }
    }
    // Avoid flagging on non-DataFrames (e.g., `{"a": 1}.values`).
    if !is_dataframe_candidate(value) {
        return;
    }

    // If the target is a named variable, avoid triggering on
    // irrelevant bindings (like imports).
    if let ExprKind::Name { id, .. } = &value.node {
        if checker.ctx.find_binding(id).map_or(true, |binding| {
            matches!(
                binding.kind,
                BindingKind::Builtin
                    | BindingKind::ClassDefinition
                    | BindingKind::FunctionDefinition
                    | BindingKind::Export(..)
                    | BindingKind::FutureImportation
                    | BindingKind::Importation(..)
                    | BindingKind::FromImportation(..)
                    | BindingKind::SubmoduleImportation(..)
            )
        }) {
            return;
        }
    }

    checker
        .diagnostics
        .push(Diagnostic::new(violation, Range::from(attr_expr)));
}
