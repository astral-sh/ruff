use crate::checkers::ast::Checker;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr};
use ruff_text_size::Ranged;

/// TODO: CHANGE THIS
/// ## What it does
/// TODO
///
/// ## Why is this bad?
/// TODO
///
/// ## Examples
/// ```python
/// from collections import deque
/// queue = deque(set())
/// queue = deque([], maxlen=10)
/// ```
///
/// Use instead:
/// ```python
/// from collections import deque
/// queue = deque()
/// queue = deque(maxlen=10)
/// ```
///
/// ## References
/// - [Python documentation: `collections.deque`](https://docs.python.org/3/library/collections.html#collections.deque)
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryLiteralInDeque {
    has_maxlen: bool,
}

impl Violation for UnnecessaryLiteralInDeque {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary literal inside a deque expression; remove the literal".to_string()
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

/// C421
pub(crate) fn unnecessary_literal_in_deque(checker: &mut Checker, deque: &ast::ExprCall) {
    let semantic = checker.semantic();
    let ast::ExprCall {
        func, arguments, ..
    } = deque;
    let func = func.as_ref();
    let Some(qualified) = semantic.resolve_qualified_name(func) else {
        return;
    };
    if !matches!(qualified.segments(), ["collections", "deque"]) {
        return;
    }
    let Some(iterable) = arguments
        .find_positional(0)
        .or_else(|| arguments.find_keyword("iterable").map(|kw| &kw.value))
    else {
        return;
    };
    let maxlen = arguments
        .find_positional(1)
        .or_else(|| arguments.find_keyword("maxlen").map(|kw| &kw.value));
    let is_empty_literal = match iterable {
        Expr::Dict(dict) => dict.is_empty(),
        Expr::List(list) => list.is_empty(),
        Expr::Tuple(tuple) => tuple.is_empty(),
        Expr::Call(call) => {
            semantic
                .resolve_builtin_symbol(&call.func)
                .is_some_and(|name| name == "set")
                && call.arguments.is_empty()
        }
        _ => false,
    };
    if !is_empty_literal {
        return;
    }

    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralInDeque {
            has_maxlen: maxlen.is_some(), // TODO: fix this
        },
        deque.range,
    );

    diagnostic.set_fix(fix_unnecessary_literal_in_deque(checker, deque, maxlen));

    checker.diagnostics.push(diagnostic);
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
