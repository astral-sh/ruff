use itertools::Itertools;
use ruff_python_ast::{self as ast, Constant, Expr};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
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
/// ## Example
/// ```python
/// "abcba".strip("ab")  # "c"
/// ```
///
/// Use instead:
/// ```python
/// "abcba".removeprefix("ab").removesuffix("ba")  # "c"
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

    let [Expr::Constant(ast::ExprConstant {
        value: Constant::Str(value),
        ..
    })] = args
    else {
        return;
    };

    let num_chars = value.chars().count();
    if num_chars > 1 && num_chars != value.chars().unique().count() {
        checker
            .diagnostics
            .push(Diagnostic::new(StripWithMultiCharacters, expr.range()));
    }
}
