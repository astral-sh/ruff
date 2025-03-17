use std::cmp::Ordering;

use ruff_diagnostics::{Applicability, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{
    str::TripleQuotes, Expr, ExprCall, ExprContext, ExprList, ExprUnaryOp, StringLiteral,
    StringLiteralFlags, StringLiteralValue, UnaryOp,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for static `str.split` calls that can be replaced with list literals.
///
/// ## Why is this bad?
/// List literals are more readable and do not require the overhead of calling `str.split`.
///
/// ## Example
/// ```python
/// "a,b,c,d".split(",")
/// ```
///
/// Use instead:
/// ```python
/// ["a", "b", "c", "d"]
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe for implicit string concatenations with comments interleaved
/// between segments, as comments may be removed.
///
/// For example, the fix would be marked as unsafe in the following case:
/// ```python
/// (
///     "a"  # comment
///     ","  # comment
///     "b"  # comment
/// ).split(",")
/// ```
///
/// as this is converted to `["a", "b"]` without any of the comments.
///
/// ## References
/// - [Python documentation: `str.split`](https://docs.python.org/3/library/stdtypes.html#str.split)
#[derive(ViolationMetadata)]
pub(crate) struct SplitStaticString;

impl Violation for SplitStaticString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Consider using a list literal instead of `str.split`".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with list literal".to_string())
    }
}

/// SIM905
pub(crate) fn split_static_string(
    checker: &Checker,
    attr: &str,
    call: &ExprCall,
    str_value: &StringLiteralValue,
) {
    let ExprCall { arguments, .. } = call;

    let maxsplit_arg = arguments.find_argument_value("maxsplit", 1);
    let Some(maxsplit_value) = get_maxsplit_value(maxsplit_arg) else {
        return;
    };

    // `split` vs `rsplit`.
    let direction = if attr == "split" {
        Direction::Left
    } else {
        Direction::Right
    };

    let sep_arg = arguments.find_argument_value("sep", 0);
    let split_replacement = if let Some(sep) = sep_arg {
        match sep {
            Expr::NoneLiteral(_) => split_default(str_value, maxsplit_value),
            Expr::StringLiteral(sep_value) => {
                let sep_value_str = sep_value.value.to_str();
                Some(split_sep(
                    str_value,
                    sep_value_str,
                    maxsplit_value,
                    direction,
                ))
            }
            // Ignore names until type inference is available.
            _ => {
                return;
            }
        }
    } else {
        split_default(str_value, maxsplit_value)
    };

    let mut diagnostic = Diagnostic::new(SplitStaticString, call.range());
    if let Some(ref replacement_expr) = split_replacement {
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(checker.generator().expr(replacement_expr), call.range()),
            // The fix does not preserve comments within implicit string concatenations.
            if checker.comment_ranges().intersects(call.range()) {
                Applicability::Unsafe
            } else {
                Applicability::Safe
            },
        ));
    }
    checker.report_diagnostic(diagnostic);
}

fn construct_replacement(elts: &[&str], flags: StringLiteralFlags) -> Expr {
    Expr::List(ExprList {
        elts: elts
            .iter()
            .map(|elt| {
                Expr::from(StringLiteral {
                    value: Box::from(*elt),
                    range: TextRange::default(),
                    // intentionally omit the triple quote flag, if set, to avoid strange
                    // replacements like
                    //
                    // ```python
                    // """
                    // itemA
                    // itemB
                    // itemC
                    // """.split() # -> ["""itemA""", """itemB""", """itemC"""]
                    // ```
                    flags: flags.with_triple_quotes(TripleQuotes::No),
                })
            })
            .collect(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    })
}

fn split_default(str_value: &StringLiteralValue, max_split: i32) -> Option<Expr> {
    // From the Python documentation:
    // > If sep is not specified or is None, a different splitting algorithm is applied: runs of
    // > consecutive whitespace are regarded as a single separator, and the result will contain
    // > no empty strings at the start or end if the string has leading or trailing whitespace.
    // > Consequently, splitting an empty string or a string consisting of just whitespace with
    // > a None separator returns [].
    // https://docs.python.org/3/library/stdtypes.html#str.split
    match max_split.cmp(&0) {
        Ordering::Greater => {
            // Autofix for `maxsplit` without separator not yet implemented, as
            // `split_whitespace().remainder()` is not stable:
            // https://doc.rust-lang.org/std/str/struct.SplitWhitespace.html#method.remainder
            None
        }
        Ordering::Equal => {
            let list_items: Vec<&str> = vec![str_value.to_str()];
            Some(construct_replacement(
                &list_items,
                str_value.first_literal_flags(),
            ))
        }
        Ordering::Less => {
            let list_items: Vec<&str> = str_value.to_str().split_whitespace().collect();
            Some(construct_replacement(
                &list_items,
                str_value.first_literal_flags(),
            ))
        }
    }
}

fn split_sep(
    str_value: &StringLiteralValue,
    sep_value: &str,
    max_split: i32,
    direction: Direction,
) -> Expr {
    let value = str_value.to_str();
    let list_items: Vec<&str> = if let Ok(split_n) = usize::try_from(max_split) {
        match direction {
            Direction::Left => value.splitn(split_n + 1, sep_value).collect(),
            Direction::Right => value.rsplitn(split_n + 1, sep_value).collect(),
        }
    } else {
        match direction {
            Direction::Left => value.split(sep_value).collect(),
            Direction::Right => value.rsplit(sep_value).collect(),
        }
    };

    construct_replacement(&list_items, str_value.first_literal_flags())
}

/// Returns the value of the `maxsplit` argument as an `i32`, if it is a numeric value.
fn get_maxsplit_value(arg: Option<&Expr>) -> Option<i32> {
    if let Some(maxsplit) = arg {
        match maxsplit {
            // Negative number.
            Expr::UnaryOp(ExprUnaryOp {
                op: UnaryOp::USub,
                operand,
                ..
            }) => {
                match &**operand {
                    Expr::NumberLiteral(maxsplit_val) => maxsplit_val
                        .value
                        .as_int()
                        .and_then(ruff_python_ast::Int::as_i32)
                        .map(|f| -f),
                    // Ignore when `maxsplit` is not a numeric value.
                    _ => None,
                }
            }
            // Positive number
            Expr::NumberLiteral(maxsplit_val) => maxsplit_val
                .value
                .as_int()
                .and_then(ruff_python_ast::Int::as_i32),
            // Ignore when `maxsplit` is not a numeric value.
            _ => None,
        }
    } else {
        // Default value is -1 (no splits).
        Some(-1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Left,
    Right,
}
