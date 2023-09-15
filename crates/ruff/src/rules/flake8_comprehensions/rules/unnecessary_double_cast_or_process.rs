use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::comparable::ComparableKeyword;
use ruff_python_ast::{self as ast, Arguments, Expr, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary `list`, `reversed`, `set`, `sorted`, and `tuple`
/// call within `list`, `set`, `sorted`, and `tuple` calls.
///
/// ## Why is this bad?
/// It's unnecessary to double-cast or double-process iterables by wrapping
/// the listed functions within an additional `list`, `set`, `sorted`, or
/// `tuple` call. Doing so is redundant and can be confusing for readers.
///
/// ## Examples
/// ```python
/// list(tuple(iterable))
/// ```
///
/// Use instead:
/// ```python
/// list(iterable)
/// ```
///
/// This rule applies to a variety of functions, including `list`, `reversed`,
/// `set`, `sorted`, and `tuple`. For example:
///
/// - Instead of `list(list(iterable))`, use `list(iterable)`.
/// - Instead of `list(tuple(iterable))`, use `list(iterable)`.
/// - Instead of `tuple(list(iterable))`, use `tuple(iterable)`.
/// - Instead of `tuple(tuple(iterable))`, use `tuple(iterable)`.
/// - Instead of `set(set(iterable))`, use `set(iterable)`.
/// - Instead of `set(list(iterable))`, use `set(iterable)`.
/// - Instead of `set(tuple(iterable))`, use `set(iterable)`.
/// - Instead of `set(sorted(iterable))`, use `set(iterable)`.
/// - Instead of `set(reversed(iterable))`, use `set(iterable)`.
/// - Instead of `sorted(list(iterable))`, use `sorted(iterable)`.
/// - Instead of `sorted(tuple(iterable))`, use `sorted(iterable)`.
/// - Instead of `sorted(sorted(iterable))`, use `sorted(iterable)`.
/// - Instead of `sorted(reversed(iterable))`, use `sorted(iterable)`.
#[violation]
pub struct UnnecessaryDoubleCastOrProcess {
    inner: String,
    outer: String,
}

impl AlwaysAutofixableViolation for UnnecessaryDoubleCastOrProcess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDoubleCastOrProcess { inner, outer } = self;
        format!("Unnecessary `{inner}` call within `{outer}()`")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryDoubleCastOrProcess { inner, .. } = self;
        format!("Remove the inner `{inner}` call")
    }
}

/// C414
pub(crate) fn unnecessary_double_cast_or_process(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    outer_kw: &[Keyword],
) {
    let Some(outer) = func.as_name_expr() else {
        return;
    };
    if !matches!(
        outer.id.as_str(),
        "list" | "tuple" | "set" | "reversed" | "sorted"
    ) {
        return;
    }
    let Some(arg) = args.first() else {
        return;
    };
    let Expr::Call(ast::ExprCall {
        func,
        arguments: Arguments {
            keywords: inner_kw, ..
        },
        ..
    }) = arg
    else {
        return;
    };
    let Some(inner) = func.as_name_expr() else {
        return;
    };
    if !checker.semantic().is_builtin(&inner.id) || !checker.semantic().is_builtin(&outer.id) {
        return;
    }

    // Avoid collapsing nested `sorted` calls with non-identical keyword arguments
    // (i.e., `key`, `reverse`).
    if inner.id == "sorted" && outer.id == "sorted" {
        if inner_kw.len() != outer_kw.len() {
            return;
        }
        if !inner_kw.iter().all(|inner| {
            outer_kw
                .iter()
                .any(|outer| ComparableKeyword::from(inner) == ComparableKeyword::from(outer))
        }) {
            return;
        }
    }

    // Ex) `set(tuple(...))`
    // Ex) `list(tuple(...))`
    // Ex) `set(set(...))`
    if matches!(
        (outer.id.as_str(), inner.id.as_str()),
        ("set" | "sorted", "list" | "tuple" | "reversed" | "sorted")
            | ("set", "set")
            | ("list" | "tuple", "list" | "tuple")
    ) {
        let mut diagnostic = Diagnostic::new(
            UnnecessaryDoubleCastOrProcess {
                inner: inner.id.to_string(),
                outer: outer.id.to_string(),
            },
            expr.range(),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                fixes::fix_unnecessary_double_cast_or_process(
                    expr,
                    checker.locator(),
                    checker.stylist(),
                )
                .map(Fix::suggested)
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
