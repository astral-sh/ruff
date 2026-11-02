use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::str::TripleQuotes;
use ruff_python_ast::{
    self as ast, BuildId, Builder, Expr, ExprKind, StringFlags, StringLiteralFlags, StringValue,
    UnaryOp,
};
use std::cmp::Ordering;
use std::fmt::{Display, Formatter};

use crate::checkers::ast::Checker;
use crate::codes::Category;
use crate::fix::syntax;
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
#[violation_metadata(stable_since = "0.10.0", category = Category::Complexity)]
pub(crate) struct SplitStaticString {
    method: Method,
}

#[derive(Copy, Clone, Debug)]
enum Method {
    Split,
    RSplit,
}

impl Method {
    fn is_rsplit(self) -> bool {
        matches!(self, Method::RSplit)
    }
}

impl Display for Method {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Method::Split => f.write_str("split"),
            Method::RSplit => f.write_str("rsplit"),
        }
    }
}

impl Violation for SplitStaticString {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "Consider using a list literal instead of `str.{}`",
            self.method
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Replace with list literal".to_string())
    }
}

/// SIM905
pub(crate) fn split_static_string(
    checker: &Checker,
    attr: &str,
    call: &ast::ExprCallFields<'_>,
    str_value: StringValue<'_>,
) {
    let arguments = call.arguments;

    let maxsplit_arg = arguments.find_argument_value("maxsplit", 1);
    let Some(maxsplit_value) = get_maxsplit_value(maxsplit_arg) else {
        return;
    };

    // `split` vs `rsplit`.
    let method = if attr == "split" {
        Method::Split
    } else {
        Method::RSplit
    };

    let separator = match arguments.find_argument_value("sep", 0).map(Expr::classify) {
        None | Some(ExprKind::NoneLiteral(_)) => None,
        Some(ExprKind::StringLiteral(sep)) => Some(sep.value().to_str()),
        // Ignore names until type inference is available.
        _ => return,
    };
    let mut builder = Builder::default();
    let split_replacement = if let Some(separator) = separator {
        Some(split_sep(
            &mut builder,
            str_value,
            separator,
            maxsplit_value,
            method,
        ))
    } else {
        split_default(&mut builder, str_value, maxsplit_value, method)
    };

    let mut diagnostic = checker.report_diagnostic(SplitStaticString { method }, call.range);
    if let Some(replacement_expr) = split_replacement {
        diagnostic.set_fix(Fix::applicable_edit(
            Edit::range_replacement(
                checker.generator().expr(builder.get(replacement_expr)),
                call.range,
            ),
            // The fix does not preserve comments within implicit string concatenations.
            if checker.comment_ranges().intersects(call.range) {
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

fn construct_replacement(
    builder: &mut Builder,
    elts: &[&str],
    flags: StringLiteralFlags,
) -> BuildId<ruff_python_ast::kind::Expr> {
    let elts = builder.map_list(elts, |builder, elt| {
        syntax::string_literal(builder, elt, replace_flags(elt, flags))
    });
    syntax::list(builder, elts)
}

fn split_default(
    builder: &mut Builder,
    str_value: StringValue<'_>,
    max_split: i32,
    method: Method,
) -> Option<BuildId<ruff_python_ast::kind::Expr>> {
    let string_val = str_value.to_str();
    match max_split.cmp(&0) {
        Ordering::Greater | Ordering::Equal => {
            let Ok(max_split) = usize::try_from(max_split) else {
                return None;
            };
            let list_items = split_whitespace_with_maxsplit(string_val, max_split, method);
            Some(construct_replacement(
                builder,
                &list_items,
                str_value.first_literal_flags(),
            ))
        }
        Ordering::Less => {
            let list_items: Vec<&str> = string_val
                .split(py_unicode_is_whitespace)
                .filter(|s| !s.is_empty())
                .collect();
            Some(construct_replacement(
                builder,
                &list_items,
                str_value.first_literal_flags(),
            ))
        }
    }
}

fn split_sep(
    builder: &mut Builder,
    str_value: StringValue<'_>,
    sep_value: &str,
    max_split: i32,
    method: Method,
) -> BuildId<ruff_python_ast::kind::Expr> {
    let value = str_value.to_str();
    let list_items: Vec<&str> = if let Ok(split_n) = usize::try_from(max_split) {
        match method {
            Method::Split => value.splitn(split_n + 1, sep_value).collect(),
            Method::RSplit => {
                let mut items: Vec<&str> = value.rsplitn(split_n + 1, sep_value).collect();
                items.reverse();
                items
            }
        }
    } else {
        match method {
            Method::Split => value.split(sep_value).collect(),
            Method::RSplit => {
                let mut items: Vec<&str> = value.rsplit(sep_value).collect();
                items.reverse();
                items
            }
        }
    };

    construct_replacement(builder, &list_items, str_value.first_literal_flags())
}

/// Returns the value of the `maxsplit` argument as an `i32`, if it is a numeric value.
fn get_maxsplit_value(arg: Option<Expr<'_>>) -> Option<i32> {
    if let Some(maxsplit) = arg {
        match maxsplit.classify() {
            // Negative number.
            ExprKind::UnaryOp(expr_unary_op) if expr_unary_op.op() == UnaryOp::USub => {
                let operand = expr_unary_op.operand();

                match operand.classify() {
                    ExprKind::NumberLiteral(maxsplit_val) => maxsplit_val
                        .value()
                        .as_int()
                        .and_then(ruff_python_ast::Int::as_i32)
                        .map(|f| -f),
                    // Ignore when `maxsplit` is not a numeric value.
                    _ => None,
                }
            }
            // Positive number
            ExprKind::NumberLiteral(maxsplit_val) => maxsplit_val
                .value()
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

/// Like [`char::is_whitespace`] but with Python's notion of whitespace.
///
/// <https://github.com/astral-sh/ruff/issues/19845>
/// <https://github.com/python/cpython/blob/v3.14.0rc1/Objects/unicodetype_db.h#L6673-L6711>
#[rustfmt::skip]
#[inline]
const fn py_unicode_is_whitespace(ch: char) -> bool {
    matches!(
        ch,
        | '\u{0009}'
        | '\u{000A}'
        | '\u{000B}'
        | '\u{000C}'
        | '\u{000D}'
        | '\u{001C}'
        | '\u{001D}'
        | '\u{001E}'
        | '\u{001F}'
        | '\u{0020}'
        | '\u{0085}'
        | '\u{00A0}'
        | '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{2028}'
        | '\u{2029}'
        | '\u{202F}'
        | '\u{205F}'
        | '\u{3000}'
    )
}

struct WhitespaceMaxSplitIterator<'a> {
    remaining: &'a str,
    max_split: usize,
    splits: usize,
    method: Method,
}

impl<'a> WhitespaceMaxSplitIterator<'a> {
    fn new(s: &'a str, max_split: usize, method: Method) -> Self {
        let remaining = match method {
            Method::Split => s.trim_start_matches(py_unicode_is_whitespace),
            Method::RSplit => s.trim_end_matches(py_unicode_is_whitespace),
        };

        Self {
            remaining,
            max_split,
            splits: 0,
            method,
        }
    }
}

impl<'a> Iterator for WhitespaceMaxSplitIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }

        if self.splits >= self.max_split {
            let result = self.remaining;
            self.remaining = "";
            return Some(result);
        }

        self.splits += 1;
        match self.method {
            Method::Split => match self.remaining.split_once(py_unicode_is_whitespace) {
                Some((s, remaining)) => {
                    self.remaining = remaining.trim_start_matches(py_unicode_is_whitespace);
                    Some(s)
                }
                None => Some(std::mem::take(&mut self.remaining)),
            },
            Method::RSplit => match self.remaining.rsplit_once(py_unicode_is_whitespace) {
                Some((remaining, s)) => {
                    self.remaining = remaining.trim_end_matches(py_unicode_is_whitespace);
                    Some(s)
                }
                None => Some(std::mem::take(&mut self.remaining)),
            },
        }
    }
}

// From the Python documentation:
// > If sep is not specified or is None, a different splitting algorithm is applied: runs of
// > consecutive whitespace are regarded as a single separator, and the result will contain
// > no empty strings at the start or end if the string has leading or trailing whitespace.
// > Consequently, splitting an empty string or a string consisting of just whitespace with
// > a None separator returns [].
// https://docs.python.org/3/library/stdtypes.html#str.split
fn split_whitespace_with_maxsplit(s: &str, max_split: usize, method: Method) -> Vec<&str> {
    let mut result: Vec<_> = WhitespaceMaxSplitIterator::new(s, max_split, method).collect();
    if method.is_rsplit() {
        result.reverse();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{Method, split_whitespace_with_maxsplit};
    use test_case::test_case;

    #[test_case("  ", 1, &[])]
    #[test_case("a  b", 1, &["a", "b"])]
    #[test_case("a  b", 2, &["a", "b"])]
    #[test_case(" a b c d ", 2, &["a", "b", "c d "])]
    #[test_case("  a  b  c  ", 1, &["a", "b  c  "])]
    #[test_case(" x ", 0, &["x "])]
    #[test_case(" ", 0, &[])]
    #[test_case("a\u{3000}b", 1, &["a", "b"])]
    fn test_split_whitespace_with_maxsplit(s: &str, max_split: usize, expected: &[&str]) {
        let parts = split_whitespace_with_maxsplit(s, max_split, Method::Split);
        assert_eq!(parts, expected);
    }

    #[test_case("  ", 1, &[])]
    #[test_case("a  b", 1, &["a", "b"])]
    #[test_case("a  b", 2, &["a", "b"])]
    #[test_case(" a b c d ", 2, &[" a b", "c", "d"])]
    #[test_case("  a  b  c  ", 1, &["  a  b", "c"])]
    #[test_case(" x ", 0, &[" x"])]
    #[test_case(" ", 0, &[])]
    #[test_case("a\u{3000}b", 1, &["a", "b"])]
    fn test_rsplit_whitespace_with_maxsplit(s: &str, max_split: usize, expected: &[&str]) {
        let parts = split_whitespace_with_maxsplit(s, max_split, Method::RSplit);
        assert_eq!(parts, expected);
    }
}
