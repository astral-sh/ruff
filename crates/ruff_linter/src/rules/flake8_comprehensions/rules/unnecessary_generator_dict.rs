use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as dict
/// comprehensions.
///
/// ## Why is this bad?
/// It is unnecessary to use `dict()` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// ## Example
/// ```python
/// dict((x, f(x)) for x in foo)
/// ```
///
/// Use instead:
/// ```python
/// {x: f(x) for x in foo}
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as it may occasionally drop comments
/// when rewriting the call. In most cases, though, comments will be preserved.
#[derive(ViolationMetadata)]
pub(crate) struct UnnecessaryGeneratorDict;

impl AlwaysFixableViolation for UnnecessaryGeneratorDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Unnecessary generator (rewrite as a dict comprehension)".to_string()
    }

    fn fix_title(&self) -> String {
        "Rewrite as a dict comprehension".to_string()
    }
}

/// C402 (`dict((x, y) for x, y in iterable)`)
pub(crate) fn unnecessary_generator_dict(
    checker: &Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) =
        helpers::exactly_one_argument_with_matching_function("dict", func, args, keywords)
    else {
        return;
    };
    let Expr::Generator(ast::ExprGenerator { elt, .. }) = argument else {
        return;
    };
    let Expr::Tuple(tuple) = &**elt else {
        return;
    };
    if tuple.len() != 2 {
        return;
    }
    if tuple.iter().any(Expr::is_starred_expr) {
        return;
    }
    let mut diagnostic = Diagnostic::new(UnnecessaryGeneratorDict, expr.range());
    diagnostic
        .try_set_fix(|| fixes::fix_unnecessary_generator_dict(expr, checker).map(Fix::unsafe_edit));
    checker.report_diagnostic(diagnostic);
}
