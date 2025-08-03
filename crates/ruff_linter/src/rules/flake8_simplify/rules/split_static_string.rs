use std::cmp::Ordering;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StringFlags;
use ruff_python_ast::{
    Expr, ExprCall, ExprContext, ExprList, ExprUnaryOp, StringLiteral, StringLiteralFlags,
    StringLiteralValue, UnaryOp, str::TripleQuotes,
};
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::{Applicability, Edit, Fix, FixAvailability, Violation};

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
            Expr::NoneLiteral(_) => split_default(str_value, maxsplit_value, direction),
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
        split_default(str_value, maxsplit_value, direction)
    };

    let mut diagnostic = checker.report_diagnostic(SplitStaticString, call.range());
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
}

fn replace_flags(elt: &str, flags: StringLiteralFlags) -> StringLiteralFlags {
    // In the ideal case we can wrap the element in _single_ quotes of the same
    // style. For example, both of these are okay:
    //
    // ```python
    // """itemA
    // itemB
    // itemC""".split() # -> ["itemA", "itemB", "itemC"]
    // ```
    //
    // ```python
    // r"""itemA
    // 'single'quoted
    // """.split() # -> [r"itemA",r"'single'quoted'"]
    // ```
    if !flags.prefix().is_raw() || !elt.contains(flags.quote_style().as_char()) {
        flags.with_triple_quotes(TripleQuotes::No)
    }
    // If we have a raw string containing a quotation mark of the same style,
    // then we have to swap the style of quotation marks used
    else if !elt.contains(flags.quote_style().opposite().as_char()) {
        flags
            .with_quote_style(flags.quote_style().opposite())
            .with_triple_quotes(TripleQuotes::No)
    } else
    // If both types of quotes are used in the raw, triple-quoted string, then
    // we are forced to either add escapes or keep the triple quotes. We opt for
    // the latter.
    {
        flags
    }
}

fn construct_replacement(elts: &[&str], flags: StringLiteralFlags) -> Expr {
    Expr::List(ExprList {
        elts: elts
            .iter()
            .map(|elt| {
                let element_flags = replace_flags(elt, flags);
                Expr::from(StringLiteral {
                    value: Box::from(*elt),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
                    flags: element_flags,
                })
            })
            .collect(),
        ctx: ExprContext::Load,
        range: TextRange::default(),
        node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
    })
}

fn split_default(
    str_value: &StringLiteralValue,
    max_split: i32,
    direction: Direction,
) -> Option<Expr> {
    // From the Python documentation:
    // > If sep is not specified or is None, a different splitting algorithm is applied: runs of
    // > consecutive whitespace are regarded as a single separator, and the result will contain
    // > no empty strings at the start or end if the string has leading or trailing whitespace.
    // > Consequently, splitting an empty string or a string consisting of just whitespace with
    // > a None separator returns [].
    // https://docs.python.org/3/library/stdtypes.html#str.split
    let string_val = str_value.to_str();
    match max_split.cmp(&0) {
        Ordering::Greater => {
            // Autofix for `maxsplit` without separator not yet implemented, as
            // `split_whitespace().remainder()` is not stable:
            // https://doc.rust-lang.org/std/str/struct.SplitWhitespace.html#method.remainder
            None
        }
        Ordering::Equal => {
            // Behavior for maxsplit = 0 when sep is None:
            // - If the string is empty or all whitespace, result is [].
            // - Otherwise:
            //   - " x ".split(maxsplit=0)  -> ['x ']
            //   - " x ".rsplit(maxsplit=0) -> [' x']
            //   - "".split(maxsplit=0) -> []
            //   - " ".split(maxsplit=0) -> []
            let processed_str = if direction == Direction::Left {
                string_val.trim_start()
            } else {
                string_val.trim_end()
            };
            let list_items: &[_] = if processed_str.is_empty() {
                &[]
            } else {
                &[processed_str]
            };
            Some(construct_replacement(
                list_items,
                str_value.first_literal_flags(),
            ))
        }
        Ordering::Less => {
            let list_items: Vec<&str> = string_val.split_whitespace().collect();
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
            Direction::Right => {
                let mut items: Vec<&str> = value.rsplitn(split_n + 1, sep_value).collect();
                items.reverse();
                items
            }
        }
    } else {
        match direction {
            Direction::Left => value.split(sep_value).collect(),
            Direction::Right => {
                let mut items: Vec<&str> = value.rsplit(sep_value).collect();
                items.reverse();
                items
            }
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
