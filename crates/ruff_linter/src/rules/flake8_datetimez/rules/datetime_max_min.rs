use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{Expr, ExprAttribute, ExprCall};
use ruff_python_semantic::Modules;
use ruff_text_size::Ranged;
use std::fmt::{Display, Formatter};

#[derive(Debug, Eq, PartialEq)]
enum MaxMin {
    /// `datetime.datetime.max`
    Max,
    /// `datetime.datetime.min`
    Min,
}

impl MaxMin {
    fn from(attr: &str) -> Self {
        match attr {
            "max" => Self::Max,
            "min" => Self::Min,
            _ => panic!("Unexpected argument for MaxMin"),
        }
    }
}

impl Display for MaxMin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            MaxMin::Max => "max",
            MaxMin::Min => "min",
        };

        write!(f, "{name}")
    }
}

/// ## What it does
/// Checks for usages of `datetime.datetime.max` and `datetime.datetime.min`.
///
/// ## Why is this bad?
/// `datetime.max` and `datetime.min` are constants with no timezone information.
/// Therefore, operations on them might fail unexpectedly:
///
/// ```python
/// # Timezone: UTC-14
/// datetime.max.timestamp()  # ValueError: year 10000 is out of range
/// datetime.min.timestamp()  # ValueError: year 0 is out of range
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
#[violation]
pub struct DatetimeMaxMin(MaxMin);

impl Violation for DatetimeMaxMin {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("`datetime.datetime.{}` used", self.0)
    }
}

/// DTZ901
pub(crate) fn datetime_max_min(checker: &mut Checker, expr: &Expr) {
    let semantic = checker.semantic();

    if !semantic.seen_module(Modules::DATETIME) {
        return;
    }

    let Some(qualified_name) = semantic.resolve_qualified_name(expr) else {
        return;
    };

    let maxmin = match qualified_name.segments() {
        ["datetime", "datetime", attr @ ("max" | "min")] => MaxMin::from(attr),
        _ => return,
    };

    if followed_by_replace_tzinfo(checker) {
        return;
    }

    let diagnostic = Diagnostic::new(DatetimeMaxMin(maxmin), expr.range());

    checker.diagnostics.push(diagnostic);
}

/// Check if the current expression has the pattern `foo.replace(tzinfo=bar)`.
pub(super) fn followed_by_replace_tzinfo(checker: &Checker) -> bool {
    let semantic = checker.semantic();

    let Some(parent) = semantic.current_expression_parent() else {
        return false;
    };
    let Some(grandparent) = semantic.current_expression_grandparent() else {
        return false;
    };

    match (parent, grandparent) {
        (Expr::Attribute(ExprAttribute { attr, .. }), Expr::Call(ExprCall { arguments, .. })) => {
            attr.as_str() == "replace" && matches!(arguments.find_keyword("tzinfo"), Some(..))
        }
        _ => false,
    }
}
