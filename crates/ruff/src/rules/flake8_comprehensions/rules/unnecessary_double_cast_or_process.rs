use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, ExprKind};

use super::helpers;
use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct UnnecessaryDoubleCastOrProcess {
        pub inner: String,
        pub outer: String,
    }
);
impl Violation for UnnecessaryDoubleCastOrProcess {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryDoubleCastOrProcess { inner, outer } = self;
        format!("Unnecessary `{inner}` call within `{outer}()`")
    }
}

/// C414
pub fn unnecessary_double_cast_or_process(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
) {
    fn diagnostic(inner: &str, outer: &str, location: Range) -> Diagnostic {
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
    if (outer == "set" || outer == "sorted")
        && (inner == "list" || inner == "tuple" || inner == "reversed" || inner == "sorted")
    {
        checker
            .diagnostics
            .push(diagnostic(inner, outer, Range::from_located(expr)));
        return;
    }

    // Ex) list(tuple(...))
    if (outer == "list" || outer == "tuple") && (inner == "list" || inner == "tuple") {
        checker
            .diagnostics
            .push(diagnostic(inner, outer, Range::from_located(expr)));
        return;
    }

    // Ex) set(set(...))
    if outer == "set" && inner == "set" {
        checker
            .diagnostics
            .push(diagnostic(inner, outer, Range::from_located(expr)));
    }
}
