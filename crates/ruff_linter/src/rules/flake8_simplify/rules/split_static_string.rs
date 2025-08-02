use std::cmp::Ordering;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StringFlags;
use ruff_python_ast::str::TripleQuotes;
use ruff_python_ast::str_prefix::StringLiteralPrefix;
use ruff_python_ast::{
    Expr, ExprCall, ExprContext, ExprList, ExprUnaryOp, StringLiteral, StringLiteralFlags,
    StringLiteralValue, UnaryOp,
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

/// Check if a string contains characters that would be unescapable in an r-string.
///
/// In r-strings, backslashes are treated literally, so sequences like `\n`, `\t`, etc.
/// are not valid escape sequences and will cause syntax errors.
/// This function now checks for all cases where a string cannot be safely represented as a raw string.
fn contains_unescapable_for_rstring(s: &str, quote: char, triple_quoted: bool) -> bool {
    if s.ends_with('\\') {
        return true;
    }
    if s.contains(quote) {
        if triple_quoted {
            let triple = std::iter::repeat_n(quote, 3).collect::<String>();
            if s.contains(&triple) {
                return true;
            }
        } else {
            return true;
        }
    }
    if !triple_quoted && s.contains('\n') {
        return true;
    }
    false
}

fn construct_replacement(elts: &[&str], flags: StringLiteralFlags) -> Expr {
    let quote = flags.quote_style().as_char();
    let triple_quoted = flags.triple_quotes() == TripleQuotes::Yes;
    Expr::List(ExprList {
        elts: elts
            .iter()
            .map(|elt| {
                let should_use_r_string = matches!(flags.prefix(), StringLiteralPrefix::Raw { .. })
                    && !contains_unescapable_for_rstring(elt, quote, triple_quoted);
                Expr::from(StringLiteral {
                    value: Box::from(*elt),
                    range: TextRange::default(),
                    node_index: ruff_python_ast::AtomicNodeIndex::dummy(),
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
                    flags: if should_use_r_string {
                        flags.with_triple_quotes(TripleQuotes::No)
                    } else {
                        let new_prefix = match flags.prefix() {
                            StringLiteralPrefix::Raw { .. } => StringLiteralPrefix::Empty,
                            StringLiteralPrefix::Unicode => StringLiteralPrefix::Unicode,
                            StringLiteralPrefix::Empty => StringLiteralPrefix::Empty,
                        };
                        flags
                            .with_triple_quotes(TripleQuotes::No)
                            .with_prefix(new_prefix)
                    },
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
