use crate::checkers::ast::Checker;
use crate::rules::airflow::helpers::Replacement;
use crate::{FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{Expr, ExprAttribute, ExprName};
use ruff_python_semantic::Modules;
use ruff_text_size::TextRange;

/// ## What it does
/// Checks for uses of Airflow functions and values that have been removed in Airflow 3.2.
///
/// ## Why is this bad?
/// Airflow 3.2 removed the `airflow.traces` module. Any imports from this
/// module will fail at runtime.
///
/// ## Example
/// ```python
/// from airflow.traces.tracer import Trace
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "0.15.1")]
pub(crate) struct Airflow32Removal {
    deprecated: String,
    replacement: Replacement,
}

impl Violation for Airflow32Removal {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Airflow32Removal { deprecated, .. } = self;
        format!("`{deprecated}` is removed in Airflow 3.2")
    }

    fn fix_title(&self) -> Option<String> {
        let Airflow32Removal { replacement, .. } = self;
        match replacement {
            Replacement::None => None,
            Replacement::Message(message) => Some((*message).to_string()),
            _ => None,
        }
    }
}

/// AIR331
pub(crate) fn airflow_3_2_removal_expr(checker: &Checker, expr: &Expr) {
    if !checker.semantic().seen_module(Modules::AIRFLOW) {
        return;
    }

    match expr {
        Expr::Attribute(ExprAttribute { range, .. }) | Expr::Name(ExprName { range, .. }) => {
            check_name(checker, expr, *range);
        }
        _ => {}
    }
}

fn check_name(checker: &Checker, expr: &Expr, range: TextRange) {
    let semantic = checker.semantic();

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let replacement = match qualified_name.segments() {
        ["airflow", "traces", ..] => Replacement::None,
        _ => return,
    };

    checker.report_diagnostic(
        Airflow32Removal {
            deprecated: qualified_name.to_string(),
            replacement,
        },
        range,
    );
}
