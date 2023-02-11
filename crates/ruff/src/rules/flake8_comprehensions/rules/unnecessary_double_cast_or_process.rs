use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::flake8_comprehensions::fixes;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    /// ## What it does
    /// Checks for unnecessary `list/reversed/set/sorted/tuple` call within `list/set/sorted/tuple`.
    ///
    /// ## Why is this bad?
    /// It's unnecessary to double-cast or double-process iterables by wrapping the listed functions within `list/set/sorted/tuple`.
    ///
    /// ## Examples
    /// Rewrite `list(list(iterable))` as `list(iterable)`
    /// Rewrite `list(tuple(iterable))` as `list(iterable)`
    /// Rewrite `tuple(list(iterable))` as `tuple(iterable)`
    /// Rewrite `tuple(tuple(iterable))` as `tuple(iterable)`
    /// Rewrite `set(set(iterable))` as `set(iterable)`
    /// Rewrite `set(list(iterable))` as `set(iterable)`
    /// Rewrite `set(tuple(iterable))` as `set(iterable)`
    /// Rewrite `set(sorted(iterable))` as `set(iterable)`
    /// Rewrite `set(reversed(iterable))` as `set(iterable)`
    /// Rewrite `sorted(list(iterable))` as `sorted(iterable)`
    /// Rewrite `sorted(tuple(iterable))` as `sorted(iterable)`
    /// Rewrite `sorted(sorted(iterable))` as `sorted(iterable)`
    /// Rewrite `sorted(reversed(iterable))` as `sorted(iterable)`
    pub struct UnnecessaryDoubleCastOrProcess {
        pub inner: String,
        pub outer: String,
    }
);
impl AlwaysAutofixableViolation for UnnecessaryDoubleCastOrProcess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDoubleCastOrProcess { inner, outer } = self;
        format!("Unnecessary `{inner}` call within `{outer}()`")
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryDoubleCastOrProcess { inner, .. } = self;
        format!("Remove the `{inner}()` call")
    }
}

/// C414
pub fn unnecessary_double_cast_or_process(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    fn create_diagnostic(inner: &str, outer: &str, location: Range) -> Diagnostic {
        Diagnostic::new(
            UnnecessaryDoubleCastOrProcess {
                inner: inner.to_string(),
                outer: outer.to_string(),
            },
            location,
        )
    }

    let Some(outer) = helpers::function_name(func) else {
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
    let ExprKind::Call { func, .. } = &arg.node else {
        return;
    };
    let Some(inner) = helpers::function_name(func) else {
        return;
    };
    if !checker.is_builtin(inner) || !checker.is_builtin(outer) {
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
        let mut diagnostic = create_diagnostic(inner, outer, Range::from_located(expr));
        if checker.patch(diagnostic.kind.rule()) {
            if let Ok(fix) = fixes::fix_unnecessary_double_cast_or_process(
                checker.locator,
                checker.stylist,
                expr,
            ) {
                diagnostic.amend(fix);
            }
        }
        checker.diagnostics.push(diagnostic);
    }
}
