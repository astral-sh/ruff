use itertools::Itertools;
use ruff_python_ast::{self as ast, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for uses of multi-character strings in `.strip()`, `.lstrip()`, and
/// `.rstrip()` calls.
///
/// ## Why is this bad?
/// All characters in the call to `.strip()`, `.lstrip()`, or `.rstrip()` are
/// removed from the leading and trailing ends of the string. If the string
/// contains multiple characters, the reader may be misled into thinking that
/// a prefix or suffix is being removed, rather than a set of characters.
///
/// In Python 3.9 and later, you can use `str.removeprefix` and
/// `str.removesuffix` to remove an exact prefix or suffix from a string,
/// respectively, which should be preferred when possible.
///
/// ## Known problems
/// As a heuristic, this rule only flags multi-character strings that contain
/// duplicate characters. This allows usages like `.strip("xyz")`, which
/// removes all occurrences of the characters `x`, `y`, and `z` from the
/// leading and trailing ends of the string, but not `.strip("foo")`.
///
/// The use of unique, multi-character strings may be intentional and
/// consistent with the intent of `.strip()`, `.lstrip()`, or `.rstrip()`,
/// while the use of duplicate-character strings is very likely to be a
/// mistake.
///
/// ## Example
/// ```python
/// "text.txt".strip(".txt")  # "e"
/// ```
///
/// Use instead:
/// ```python
/// "text.txt".removesuffix(".txt")  # "text"
/// ```
///
/// ## References
/// - [Python documentation: `str.strip`](https://docs.python.org/3/library/stdtypes.html#str.strip)
#[derive(ViolationMetadata)]
pub(crate) struct StripWithMultiCharacters;

impl Violation for StripWithMultiCharacters {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Using `.strip()` with multi-character strings is misleading".to_string()
    }
}

/// B005
pub(crate) fn strip_with_multi_characters(
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Expr::Attribute(ast::ExprAttribute { attr, .. }) = func else {
        return;
    };
    if !matches!(attr.as_str(), "strip" | "lstrip" | "rstrip") {
        return;
    }

    let [Expr::StringLiteral(ast::ExprStringLiteral { value, .. })] = args else {
        return;
    };

    if value.chars().count() > 1 && !value.chars().all_unique() {
        checker.report_diagnostic(Diagnostic::new(StripWithMultiCharacters, expr.range()));
    }
}
