use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_diagnostics::FixAvailability;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::Number;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of `sorted()` to retrieve the minimum or maximum value in
/// a sequence.
///
/// ## Why is this bad?
/// Using `sorted()` to compute the minimum or maximum value in a sequence is
/// inefficient and less readable than using `min()` or `max()` directly.
///
/// ## Example
/// ```python
/// nums = [3, 1, 4, 1, 5]
/// lowest = sorted(nums)[0]
/// highest = sorted(nums)[-1]
/// highest = sorted(nums, reverse=True)[0]
/// ```
///
/// Use instead:
/// ```python
/// nums = [3, 1, 4, 1, 5]
/// lowest = min(nums)
/// highest = max(nums)
/// ```
///
/// ## Fix safety
/// In some cases, migrating to `min` or `max` can lead to a change in behavior,
/// notably when breaking ties.
///
/// As an example, `sorted(data, key=itemgetter(0), reverse=True)[0]` will return
/// the _last_ "minimum" element in the list, if there are multiple elements with
/// the same key. However, `min(data, key=itemgetter(0))` will return the _first_
/// "minimum" element in the list in the same scenario.
///
/// As such, this rule's fix is marked as unsafe when the `reverse` keyword is used.
///
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3/library/functions.html#max)
#[derive(ViolationMetadata)]
pub(crate) struct SortedMinMax {
    min_max: MinMax,
}

impl Violation for SortedMinMax {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        match self.min_max {
            MinMax::Min => {
                "Prefer `min` over `sorted()` to compute the minimum value in a sequence"
                    .to_string()
            }
            MinMax::Max => {
                "Prefer `max` over `sorted()` to compute the maximum value in a sequence"
                    .to_string()
            }
        }
    }

    fn fix_title(&self) -> Option<String> {
        let title = match self.min_max {
            MinMax::Min => "Replace with `min`",
            MinMax::Max => "Replace with `max`",
        };
        Some(title.to_string())
    }
}

/// FURB192
pub(crate) fn sorted_min_max(checker: &Checker, subscript: &ast::ExprSubscript) {
    if subscript.ctx.is_store() || subscript.ctx.is_del() {
        return;
    }

    let ast::ExprSubscript { slice, value, .. } = &subscript;

    // Early return if index is not unary or a number literal.
    if !(slice.is_number_literal_expr() || slice.is_unary_op_expr()) {
        return;
    }

    // Early return if the value is not a call expression.
    let Expr::Call(ast::ExprCall {
        func, arguments, ..
    }) = value.as_ref()
    else {
        return;
    };

    // Check if the index is either 0 or -1.
    let index = match slice.as_ref() {
        // [0]
        Expr::NumberLiteral(ast::ExprNumberLiteral {
            value: Number::Int(index),
            ..
        }) if *index == 0 => Index::First,

        // [-1]
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: ast::UnaryOp::USub,
            operand,
            ..
        }) => {
            match operand.as_ref() {
                // [-1]
                Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: Number::Int(index),
                    ..
                }) if *index == 1 => Index::Last,
                _ => return,
            }
        }
        _ => return,
    };

    // Check if the value is a call to `sorted()`.
    if !checker.semantic().match_builtin_expr(func, "sorted") {
        return;
    }

    // Check if the call to `sorted()` has a single argument.
    let [list_expr] = arguments.args.as_ref() else {
        return;
    };

    let mut reverse_keyword = None;
    let mut key_keyword_expr = None;

    // Check if the call to `sorted()` has the `reverse` and `key` keywords.
    for keyword in &*arguments.keywords {
        // If the call contains `**kwargs`, return.
        let Some(arg) = keyword.arg.as_ref() else {
            return;
        };

        match arg.as_str() {
            "reverse" => {
                reverse_keyword = Some(keyword);
            }
            "key" => {
                key_keyword_expr = Some(keyword);
            }
            _ => {
                // If unexpected keyword is found, return.
                return;
            }
        }
    }

    let is_reversed = if let Some(reverse_keyword) = reverse_keyword {
        match reverse_keyword.value.as_boolean_literal_expr() {
            Some(ast::ExprBooleanLiteral { value, .. }) => *value,
            // If the value is not a boolean literal, we can't determine if it is reversed.
            _ => return,
        }
    } else {
        // No reverse keyword, so it is not reversed.
        false
    };

    // Determine whether the operation is computing a minimum or maximum value.
    let min_max = match (index, is_reversed) {
        (Index::First, false) => MinMax::Min,
        (Index::First, true) => MinMax::Max,
        (Index::Last, false) => MinMax::Max,
        (Index::Last, true) => MinMax::Min,
    };

    let mut diagnostic = Diagnostic::new(SortedMinMax { min_max }, subscript.range());

    if checker.semantic().has_builtin_binding(min_max.as_str()) {
        diagnostic.set_fix({
            let replacement = if let Some(key) = key_keyword_expr {
                format!(
                    "{min_max}({}, {})",
                    checker.locator().slice(list_expr),
                    checker.locator().slice(key),
                )
            } else {
                format!("{min_max}({})", checker.locator().slice(list_expr))
            };

            let replacement = Edit::range_replacement(replacement, subscript.range());
            if is_reversed {
                Fix::unsafe_edit(replacement)
            } else {
                Fix::safe_edit(replacement)
            }
        });
    }

    checker.report_diagnostic(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MinMax {
    /// E.g., `min(nums)`
    Min,
    /// E.g., `max(nums)`
    Max,
}

impl MinMax {
    fn as_str(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

impl std::fmt::Display for MinMax {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Min => write!(f, "min"),
            Self::Max => write!(f, "max"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Index {
    /// E.g., `sorted(nums)[0]`
    First,
    /// E.g., `sorted(nums)[-1]`
    Last,
}
