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

    let Some(iterable) = arguments.find_argument_value("iterable", 0) else {
        return;
    };

    let maxlen = arguments.find_argument_value("maxlen", 1);

    let is_empty_literal = match iterable {
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

    diagnostic
        .try_set_fix(|| fix_unnecessary_literal_in_deque(checker, iterable, &deque.arguments));
}

/// Fix the unnecessary `argument` by removing it from `arguments`.
fn fix_unnecessary_literal_in_deque(
    checker: &Checker,
    argument: &Expr,
    arguments: &ast::Arguments,
) -> anyhow::Result<Fix> {
    let range = parenthesized_range(
        argument.into(),
        arguments.into(),
        checker.comment_ranges(),
        checker.source(),
    )
    .unwrap_or(argument.range());
    let edit = remove_argument(&range, arguments, Parentheses::Preserve, checker.source())?;
    Ok(Fix::safe_edit(edit))
}
