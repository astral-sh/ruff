use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::comparable::ComparableKeyword;
use ruff_python_ast::{self as ast, Arguments, Expr, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

/// ## What it does
/// Checks for unnecessary `list()`, `reversed()`, `set()`, `sorted()`, and
/// `tuple()` call within `list()`, `set()`, `sorted()`, and `tuple()` calls.
///
/// ## Why is this bad?
/// It's unnecessary to double-cast or double-process iterables by wrapping
/// the listed functions within an additional `list()`, `set()`, `sorted()`, or
/// `tuple()` call. Doing so is redundant and can be confusing for readers.
///
/// ## Example
/// ```python
/// list(tuple(iterable))
/// ```
///
/// Use instead:
/// ```python
/// list(iterable)
/// ```
///
/// This rule applies to a variety of functions, including `list()`, `reversed()`,
/// `set()`, `sorted()`, and `tuple()`. For example:
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
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryDoubleCastOrProcess {
    inner: String,
    outer: String,
}

impl AlwaysFixableViolation for UnnecessaryDoubleCastOrProcess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDoubleCastOrProcess { inner, outer } = self;
        format!("Unnecessary `{inner}()` call within `{outer}()`")
    }

    fn fix_title(&self) -> String {
        let UnnecessaryDoubleCastOrProcess { inner, .. } = self;
        format!("Remove the inner `{inner}()` call")
    }
}

/// C414
pub(crate) fn unnecessary_double_cast_or_process(
    checker: &Checker,
    expr: &Expr,
    outer_func: &Expr,
    args: &[Expr],
    outer_kw: &[Keyword],
) {
    let Some(arg) = args.first() else {
        return;
    };
    let Expr::Call(ast::ExprCall {
        func: inner_func,
        arguments: Arguments {
            keywords: inner_kw, ..
        },
        ..
    }) = arg
    else {
        return;
    };
    let semantic = checker.semantic();
    let Some(outer_func_name) = semantic.resolve_builtin_symbol(outer_func) else {
        return;
    };
    if !matches!(
        outer_func_name,
        "list" | "tuple" | "set" | "reversed" | "sorted"
    ) {
        return;
    }
    let Some(inner_func_name) = semantic.resolve_builtin_symbol(inner_func) else {
        return;
    };

    // Avoid collapsing nested `sorted` calls with non-identical keyword arguments
    // (i.e., `key`, `reverse`).
    if inner_func_name == "sorted" && outer_func_name == "sorted" {
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
        (outer_func_name, inner_func_name),
        ("set" | "sorted", "list" | "tuple" | "reversed" | "sorted")
            | ("set", "set")
            | ("list" | "tuple", "list" | "tuple")
    ) {
        let mut diagnostic = Diagnostic::new(
            UnnecessaryDoubleCastOrProcess {
                inner: inner_func_name.to_string(),
                outer: outer_func_name.to_string(),
            },
            expr.range(),
        );
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_double_cast_or_process(
                expr,
                checker.locator(),
                checker.stylist(),
            )
            .map(Fix::unsafe_edit)
        });
        checker.report_diagnostic(diagnostic);
    }
}
