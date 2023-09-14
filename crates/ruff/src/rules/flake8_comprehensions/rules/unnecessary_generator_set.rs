use ruff_python_ast::{Expr, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as `set`
/// comprehensions.
///
/// ## Why is this bad?
/// It is unnecessary to use `set` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// ## Examples
/// ```python
/// set(f(x) for x in foo)
/// ```
///
/// Use instead:
/// ```python
/// {f(x) for x in foo}
/// ```
#[violation]
pub struct UnnecessaryGeneratorSet;

impl AlwaysAutofixableViolation for UnnecessaryGeneratorSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `set` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }
}

/// C401 (`set(generator)`)
pub(crate) fn unnecessary_generator_set(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) =
        helpers::exactly_one_argument_with_matching_function("set", func, args, keywords)
    else {
        return;
    };
    if !checker.semantic().is_builtin("set") {
        return;
    }
    if let Expr::GeneratorExp(_) = argument {
        let mut diagnostic = Diagnostic::new(UnnecessaryGeneratorSet, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                fixes::fix_unnecessary_generator_set(expr, checker).map(Fix::suggested)
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
