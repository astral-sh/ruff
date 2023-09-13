use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Expr, Keyword};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as `dict`
/// comprehensions.
///
/// ## Why is this bad?
/// It is unnecessary to use `dict` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// ## Examples
/// ```python
/// dict((x, f(x)) for x in foo)
/// ```
///
/// Use instead:
/// ```python
/// {x: f(x) for x in foo}
/// ```
#[violation]
pub struct UnnecessaryGeneratorDict;

impl AlwaysAutofixableViolation for UnnecessaryGeneratorDict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `dict` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `dict` comprehension".to_string()
    }
}

/// C402 (`dict((x, y) for x, y in iterable)`)
pub(crate) fn unnecessary_generator_dict(
    checker: &mut Checker,
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
    if let Expr::GeneratorExp(ast::ExprGeneratorExp { elt, .. }) = argument {
        match elt.as_ref() {
            Expr::Tuple(ast::ExprTuple { elts, .. }) if elts.len() == 2 => {
                let mut diagnostic = Diagnostic::new(UnnecessaryGeneratorDict, expr.range());
                if checker.patch(diagnostic.kind.rule()) {
                    diagnostic.try_set_fix(|| {
                        fixes::fix_unnecessary_generator_dict(expr, checker).map(Fix::suggested)
                    });
                }
                checker.diagnostics.push(diagnostic);
            }
            _ => {}
        }
    }
}
