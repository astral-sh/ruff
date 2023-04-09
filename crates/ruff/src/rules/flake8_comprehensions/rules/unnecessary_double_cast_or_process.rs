use rustpython_parser::ast::{Expr, ExprKind};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

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
    pub inner: String,
    pub outer: String,
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
pub fn unnecessary_double_cast_or_process(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    let Some(outer) = helpers::expr_name(func) else {
        return;
    };
    if !(outer == "list"
        || outer == "tuple"
        || outer == "set"
        || outer == "reversed"
        || outer == "sorted")
    {
        return;
    }
    let Some(arg) = args.first() else {
        return;
    };
    let ExprKind::Call { func, ..} = &arg.node else {
        return;
    };
    let Some(inner) = helpers::expr_name(func) else {
        return;
    };
    if !checker.ctx.is_builtin(inner) || !checker.ctx.is_builtin(outer) {
        return;
    }

    // Ex) set(tuple(...))
    // Ex) list(tuple(...))
    // Ex) set(set(...))
    if ((outer == "set" || outer == "sorted")
        && (inner == "list" || inner == "tuple" || inner == "reversed" || inner == "sorted"))
        || (outer == "set" && inner == "set")
        || ((outer == "list" || outer == "tuple") && (inner == "list" || inner == "tuple"))
    {
        let mut diagnostic = Diagnostic::new(
            UnnecessaryDoubleCastOrProcess {
                inner: inner.to_string(),
                outer: outer.to_string(),
            },
            Range::from(expr),
        );
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                fixes::fix_unnecessary_double_cast_or_process(
                    checker.locator,
                    checker.stylist,
                    expr,
                )
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
