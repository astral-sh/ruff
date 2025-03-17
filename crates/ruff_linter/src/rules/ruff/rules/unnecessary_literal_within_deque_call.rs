use crate::checkers::ast::Checker;
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

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
/// ## References
/// - [Python documentation: `collections.deque`](https://docs.python.org/3/library/collections.html#collections.deque)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryEmptyIterableWithinDequeCall {
    has_maxlen: bool,
}

impl AlwaysFixableViolation for UnnecessaryEmptyIterableWithinDequeCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary empty iterable within a deque call".to_string()
    }

    fn fix_title(&self) -> String {
        let title = if self.has_maxlen {
            "Replace with `deque(maxlen=...)`"
        } else {
            "Replace with `deque()`"
        };
        title.to_string()
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

    let mut diagnostic = Diagnostic::new(
        UnnecessaryEmptyIterableWithinDequeCall {
            has_maxlen: maxlen.is_some(),
        },
        deque.range,
    );

    diagnostic.set_fix(fix_unnecessary_literal_in_deque(checker, deque, maxlen));

    checker.report_diagnostic(diagnostic);
}

fn fix_unnecessary_literal_in_deque(
    checker: &Checker,
    deque: &ast::ExprCall,
    maxlen: Option<&Expr>,
) -> Fix {
    let deque_name = checker.locator().slice(deque.func.range());
    let deque_str = match maxlen {
        Some(maxlen) => {
            let len_str = checker.locator().slice(maxlen);
            format!("{deque_name}(maxlen={len_str})")
        }
        None => format!("{deque_name}()"),
    };
    Fix::safe_edit(Edit::range_replacement(deque_str, deque.range))
}
