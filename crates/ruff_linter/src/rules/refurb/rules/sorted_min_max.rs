use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_diagnostics::FixAvailability;
use ruff_diagnostics::Violation;
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr};

use crate::checkers::ast::Checker;
use ruff_python_ast::Number;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for uses of `sorted()` to get the min and max values of a list.
///
/// ## Why is this bad?
/// Using `sorted()` to get the min and max values of a list is inefficient.
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
/// ## References
/// - [Python documentation: `min`](https://docs.python.org/3/library/functions.html#min)
/// - [Python documentation: `max`](https://docs.python.org/3/library/functions.html#max)

#[violation]
pub struct SortedMinMax;

impl Violation for SortedMinMax {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Prefer `min` and `max` over `sorted()` to get the min and max values of a list")
    }

    fn fix_title(&self) -> Option<String> {
        Some(format!("Replace with `min` and `max`"))
    }
}

/// FURB192
pub(crate) fn sorted_min_max(checker: &mut Checker, subscript: &ast::ExprSubscript) {
    if subscript.ctx.is_store() || subscript.ctx.is_del() {
        return;
    }

    let ast::ExprSubscript { slice, value, .. } = &subscript;

    // Early return if index is not unary or a number literal
    if !(slice.is_number_literal_expr() || slice.is_unary_op_expr()) {
        return;
    }

    let Expr::Call(ast::ExprCall {
        func,
        arguments,
        range,
    }) = value.as_ref()
    else {
        return;
    };
    // Check if the value is a call to `sorted()`
    if !matches!(func.as_ref(), Expr::Name(name) if name.id == "sorted" && checker.semantic().is_builtin(name.id.as_str()))
    {
        return;
    };

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

    let reversed = arguments.keywords.iter().any(|keyword| {
        // Is the keyword "reverse" and is the value `true`?
        keyword
            .arg
            .as_ref()
            .map_or(false, |arg| arg.as_str() == "reverse")
            && keyword
                .value
                .as_boolean_literal_expr()
                .map_or(false, |b| b.value)
    });

    let keys = arguments.keywords.iter().find(|keyword| {
        keyword
            .arg
            .as_ref()
            .map_or(false, |arg| arg.as_str() == "keys")
    });

    let min_max = match (index, reversed) {
        (Index::First, false) => MinMax::Min,
        (Index::First, true) => MinMax::Max,
        (Index::Last, false) => MinMax::Max,
        (Index::Last, true) => MinMax::Min,
    };

    let Some(Expr::Name(list_expr)) = arguments.args.get(0) else {
        return;
    };

    // let replacement = format!("{}({})", min_max.as_str(), keys);
    let replacement = format!("{}({})", min_max.as_str(), list_expr.id);

    let mut diagnostic = Diagnostic::new(SortedMinMax, subscript.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::replacement(
        replacement,
        subscript.start(),
        subscript.end(),
    )));
    checker.diagnostics.push(diagnostic);
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum MinMax {
    Min,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Index {
    // 0
    First,
    // -1
    Last,
}

// TODO:
// - Caveat reverse=True with -1 as unsafe
