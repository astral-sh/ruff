use std::fmt::{Display, Formatter};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::{Modules, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `datetime.datetime.min` and `datetime.datetime.max`.
///
/// ## Why is this bad?
/// `datetime.min` and `datetime.max` are non-timezone-aware datetime objects.
///
/// As such, operations on `datetime.min` and `datetime.max` may behave
/// unexpectedly, as in:
///
/// ```python
/// # Timezone: UTC-14
/// datetime.min.timestamp()  # ValueError: year 0 is out of range
/// datetime.max.timestamp()  # ValueError: year 10000 is out of range
/// ```
///
/// ## Example
/// ```python
/// datetime.max
/// ```
///
/// Use instead:
/// ```python
/// datetime.max.replace(tzinfo=datetime.UTC)
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct DatetimeMinMax {
    min_max: MinMax,
}

impl Violation for DatetimeMinMax {
    #[derive_message_formats]
    fn message(&self) -> String {
        let DatetimeMinMax { min_max } = self;
        format!("Use of `datetime.datetime.{min_max}` without timezone information")
    }

    fn fix_title(&self) -> Option<String> {
        let DatetimeMinMax { min_max } = self;
        Some(format!(
            "Replace with `datetime.datetime.{min_max}.replace(tzinfo=...)`"
        ))
    }
}

/// DTZ901
pub(crate) fn datetime_min_max(checker: &Checker, expr: &Expr) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::DATETIME) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let min_max = match qualified_name.segments() {
        ["datetime", "datetime", "min"] => MinMax::Min,
        ["datetime", "datetime", "max"] => MinMax::Max,
        _ => return,
    };

    if usage_is_safe(checker.semantic()) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(DatetimeMinMax { min_max }, expr.range()));
}

/// Check if the current expression has the pattern `foo.replace(tzinfo=bar)` or `foo.time()`.
fn usage_is_safe(semantic: &SemanticModel) -> bool {
    let Some(parent) = semantic.current_expression_parent() else {
        return false;
    };
    let Some(grandparent) = semantic.current_expression_grandparent() else {
        return false;
    };

    match (parent, grandparent) {
        (Expr::Attribute(ExprAttribute { attr, .. }), Expr::Call(ExprCall { arguments, .. })) => {
            attr == "time" || (attr == "replace" && arguments.find_keyword("tzinfo").is_some())
        }
        _ => false,
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum MinMax {
    /// `datetime.datetime.min`
    Min,
    /// `datetime.datetime.max`
    Max,
}

impl Display for MinMax {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MinMax::Min => write!(f, "min"),
            MinMax::Max => write!(f, "max"),
        }
    }
}
