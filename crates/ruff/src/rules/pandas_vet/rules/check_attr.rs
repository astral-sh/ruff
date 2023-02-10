use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{ExprKind, Located};

use crate::ast::types::{BindingKind, Range};
use crate::checkers::ast::Checker;
use crate::fix::Fix;
use crate::registry::{Diagnostic, DiagnosticKind, Rule};
use crate::rules::pandas_vet::fixes::fix_attr;
use crate::rules::pandas_vet::helpers::is_dataframe_candidate;
use crate::violation::{AlwaysAutofixableViolation, Violation};

define_violation!(
    pub struct UseOfDotIx;
);
impl Violation for UseOfDotIx {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`.ix` is deprecated; use more explicit `.loc` or `.iloc`")
    }
}

define_violation!(
    pub struct UseOfDotAt;
);
impl AlwaysAutofixableViolation for UseOfDotAt {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.loc` instead of `.at`.  If speed is important, use numpy.")
    }

    fn autofix_title(&self) -> String {
        format!("Replace `.at` with `.loc`")
    }
}

define_violation!(
    pub struct UseOfDotIat;
);
impl AlwaysAutofixableViolation for UseOfDotIat {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.iloc` instead of `.iat`.  If speed is important, use numpy.")
    }

    fn autofix_title(&self) -> String {
        format!("Replace `.iat` with `.iloc`")
    }
}

define_violation!(
    pub struct UseOfDotValues;
);
impl AlwaysAutofixableViolation for UseOfDotValues {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use `.to_numpy()` instead of `.values`")
    }

    fn autofix_title(&self) -> String {
        format!("Replace `.values` with `.to_numpy()`")
    }
}

pub fn check_attr(
    checker: &mut Checker,
    attr: &str,
    value: &Located<ExprKind>,
    attr_expr: &Located<ExprKind>,
) {
    let rules = &checker.settings.rules;
    let violation: DiagnosticKind = match attr {
        "ix" if rules.enabled(&Rule::UseOfDotIx) => UseOfDotIx.into(),
        "at" if rules.enabled(&Rule::UseOfDotAt) => UseOfDotAt.into(),
        "iat" if rules.enabled(&Rule::UseOfDotIat) => UseOfDotIat.into(),
        "values" if rules.enabled(&Rule::UseOfDotValues) => UseOfDotValues.into(),
        _ => return,
    };

    // Avoid flagging on function calls (e.g., `df.values()`).
    if let Some(parent) = checker.current_expr_parent() {
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
        if checker.find_binding(id).map_or(true, |binding| {
            matches!(
                binding.kind,
                BindingKind::Builtin
                    | BindingKind::ClassDefinition
                    | BindingKind::FunctionDefinition
                    | BindingKind::Export(..)
                    | BindingKind::FutureImportation
                    | BindingKind::StarImportation(..)
                    | BindingKind::Importation(..)
                    | BindingKind::FromImportation(..)
                    | BindingKind::SubmoduleImportation(..)
            )
        }) {
            return;
        }
    }

    let mut diagnostic = Diagnostic::new(violation, Range::from_located(attr_expr));
    if checker.patch(diagnostic.kind.rule()) {
        let replacement = match *diagnostic.kind.rule() {
            Rule::UseOfDotAt => Some("loc"),
            Rule::UseOfDotIat => Some("iloc"),
            Rule::UseOfDotValues => Some("to_numpy()"),
            _ => None,
        };
        if let Some(replacement) = replacement {
            diagnostic.amend(Fix::replacement(
                fix_attr(replacement, value, checker.stylist),
                attr_expr.location,
                attr_expr.end_location.unwrap(),
            ));
        }
    }

    checker.diagnostics.push(diagnostic);
}
