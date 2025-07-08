use ruff_diagnostics::{Applicability, Edit};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::parenthesize::parenthesized_range;
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{Parentheses, remove_argument};
use crate::{Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for usages of `collections.deque` that have an empty iterable as the first argument.
///
/// ## Why is this bad?
/// It's unnecessary to use an empty literal as a deque's iterable, since this is already the default behavior.
///
/// ## Example
///
/// ```python
/// from collections import deque
///
/// queue = deque(set())
/// queue = deque([], 10)
/// ```
///
/// Use instead:
///
/// ```python
/// from collections import deque
///
/// queue = deque()
/// queue = deque(maxlen=10)
/// ```
///
/// ## Fix safety
///
/// The fix is marked as unsafe whenever it would delete comments present in the `deque` call or if
/// there are unrecognized arguments other than `iterable` and `maxlen`.
///
/// ## Fix availability
///
/// This rule's fix is unavailable if any starred arguments are present after the initial iterable.
///
/// ## References
/// - [Python documentation: `collections.deque`](https://docs.python.org/3/library/collections.html#collections.deque)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryEmptyIterableWithinDequeCall {
    has_maxlen: bool,
}

impl Violation for UnnecessaryEmptyIterableWithinDequeCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary empty iterable within a deque call".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        let title = if self.has_maxlen {
            "Replace with `deque(maxlen=...)`"
        } else {
            "Replace with `deque()`"
        };
        Some(title.to_string())
    }
}

/// RUF037
pub(crate) fn unnecessary_literal_within_deque_call(checker: &Checker, deque: &ast::ExprCall) {
    let ast::ExprCall {
        func, arguments, ..
    } = deque;

    let Some(qualified) = checker.semantic().resolve_qualified_name(func) else {
        return;
    };
    if !matches!(qualified.segments(), ["collections", "deque"]) || arguments.len() > 2 {
        return;
    }

    let Some(iterable) = arguments.find_argument("iterable", 0) else {
        return;
    };

    let maxlen = arguments.find_argument_value("maxlen", 1);

    let is_empty_literal = match iterable.value() {
        Expr::Dict(dict) => dict.is_empty(),
        Expr::List(list) => list.is_empty(),
        Expr::Tuple(tuple) => tuple.is_empty(),
        Expr::Call(call) => {
            checker
                .semantic()
                .resolve_builtin_symbol(&call.func)
                // other lints should handle empty list/dict/tuple calls,
                // but this means that the lint still applies before those are fixed
                .is_some_and(|name| {
                    name == "set" || name == "list" || name == "dict" || name == "tuple"
                })
                && call.arguments.is_empty()
        }
        Expr::StringLiteral(string) => string.value.is_empty(),
        Expr::BytesLiteral(bytes) => bytes.value.is_empty(),
        Expr::FString(fstring) => fstring.value.is_empty_literal(),
        _ => false,
    };
    if !is_empty_literal {
        return;
    }

    let mut diagnostic = checker.report_diagnostic(
        UnnecessaryEmptyIterableWithinDequeCall {
            has_maxlen: maxlen.is_some(),
        },
        deque.range,
    );

    // Return without a fix in the presence of a starred argument because we can't accurately
    // generate the fix. If all of the arguments are unpacked (e.g. `deque(*([], 10))`), we will
    // have already returned after the first `find_argument_value` call.
    if deque.arguments.args.iter().any(Expr::is_starred_expr) {
        return;
    }

    diagnostic.try_set_fix(|| fix_unnecessary_literal_in_deque(checker, iterable, deque, maxlen));
}

fn fix_unnecessary_literal_in_deque(
    checker: &Checker,
    iterable: ast::ArgOrKeyword,
    deque: &ast::ExprCall,
    maxlen: Option<&Expr>,
) -> anyhow::Result<Fix> {
    // if `maxlen` is `Some`, we know there were exactly two arguments, and we can replace the whole
    // call. otherwise, we only delete the `iterable` argument and leave the others untouched.
    let edit = if let Some(maxlen) = maxlen {
        let deque_name = checker.locator().slice(
            parenthesized_range(
                deque.func.as_ref().into(),
                deque.into(),
                checker.comment_ranges(),
                checker.source(),
            )
            .unwrap_or(deque.func.range()),
        );
        let len_str = checker.locator().slice(maxlen);
        let deque_str = format!("{deque_name}(maxlen={len_str})");
        Edit::range_replacement(deque_str, deque.range)
    } else {
        let range = parenthesized_range(
            iterable.value().into(),
            (&deque.arguments).into(),
            checker.comment_ranges(),
            checker.source(),
        )
        .unwrap_or(iterable.range());
        remove_argument(
            &range,
            &deque.arguments,
            Parentheses::Preserve,
            checker.source(),
            checker.comment_ranges(),
        )?
    };
    let has_comments = checker.comment_ranges().intersects(edit.range());
    // we've already checked maxlen.is_some() && args != 2 above, so this is the only problematic
    // case left
    let unknown_arguments = maxlen.is_none() && deque.arguments.len() != 1;
    let applicability = if has_comments || unknown_arguments {
        Applicability::Unsafe
    } else {
        Applicability::Safe
    };

    Ok(Fix::applicable_edit(edit, applicability))
}
