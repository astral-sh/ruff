use rustpython_parser::ast::{Expr, Keyword, Ranged};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary generators that can be rewritten as `list`
/// comprehensions.
///
/// ## Why is this bad?
/// It is unnecessary to use `list` around a generator expression, since
/// there are equivalent comprehensions for these types. Using a
/// comprehension is clearer and more idiomatic.
///
/// ## Examples
/// ```python
/// list(f(x) for x in foo)
/// ```
///
/// Use instead:
/// ```python
/// [f(x) for x in foo]
/// ```
#[violation]
pub struct UnnecessaryGeneratorList;

impl AlwaysAutofixableViolation for UnnecessaryGeneratorList {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary generator (rewrite as a `list` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `list` comprehension".to_string()
    }
}

/// C400 (`list(generator)`)
pub(crate) fn unnecessary_generator_list(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) =
        helpers::exactly_one_argument_with_matching_function("list", func, args, keywords)
    else {
        return;
    };
    if !checker.semantic().is_builtin("list") {
        return;
    }
    if let Expr::GeneratorExp(_) = argument {
        let mut diagnostic = Diagnostic::new(UnnecessaryGeneratorList, expr.range());
        if checker.patch(diagnostic.kind.rule()) {
            #[allow(deprecated)]
            diagnostic.try_set_fix_from_edit(|| {
                fixes::fix_unnecessary_generator_list(checker.locator, checker.stylist, expr)
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
