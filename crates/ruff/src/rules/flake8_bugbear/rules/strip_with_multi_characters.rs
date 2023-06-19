use itertools::Itertools;
use rustpython_parser::ast::{self, Constant, Expr, Ranged};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for multi-character strings in `.strip()`, `.lstrip()`, and
/// `.rstrip()`.
///
/// ## Why is this bad?
/// All characters in the call to `.strip()`, `.lstrip()`, or `.rstrip()` are
/// removed from the leading and trailing ends of the string. If the string
/// contains multiple characters, the reader may be misled into thinking that
/// substring is removed from the leading and trailing ends of the string.
///
/// ## Example
/// ```python
/// "abcba".strip("ab")  # "c"
/// ```
///
/// Use instead:
/// ```python
/// "abcba".strip("a").strip("b")  # "c"
/// ```
///
/// ## References
/// - [Python documentation: `str.strip`](https://docs.python.org/3/library/stdtypes.html#str.strip)
#[violation]
pub struct StripWithMultiCharacters;

impl Violation for StripWithMultiCharacters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Using `.strip()` with multi-character strings is misleading the reader")
    }
}

/// B005
pub(crate) fn strip_with_multi_characters(
    checker: &mut Checker,
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
    if args.len() != 1 {
        return;
    }

    let Expr::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    } )= &args[0] else {
        return;
    };

    let num_chars = value.chars().count();
    if num_chars > 1 && num_chars != value.chars().unique().count() {
        checker
            .diagnostics
            .push(Diagnostic::new(StripWithMultiCharacters, expr.range()));
    }
}
