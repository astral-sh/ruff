use rustpython_parser::ast::{Expr, ExprKind, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for unnecessary list comprehensions.
///
/// ## Why is it bad?
/// It's unnecessary to use a list comprehension inside a call to `set`,
/// since there is an equivalent comprehension for this type.
///
/// ## Examples
/// ```python
/// set([f(x) for x in foo])
/// ```
///
/// Use instead:
/// ```python
/// {f(x) for x in foo}
/// ```
#[violation]
pub struct UnnecessaryListComprehensionSet;

impl AlwaysAutofixableViolation for UnnecessaryListComprehensionSet {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Unnecessary `list` comprehension (rewrite as a `set` comprehension)")
    }

    fn autofix_title(&self) -> String {
        "Rewrite as a `set` comprehension".to_string()
    }
}

/// C403 (`set([...])`)
pub fn unnecessary_list_comprehension_set(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    let Some(argument) = helpers::exactly_one_argument_with_matching_function("set", func, args, keywords) else {
        return;
    };
    if !checker.ctx.is_builtin("set") {
        return;
    }
    if let ExprKind::ListComp { .. } = &argument {
        let mut diagnostic = Diagnostic::new(UnnecessaryListComprehensionSet, Range::from(expr));
        if checker.patch(diagnostic.kind.rule()) {
            diagnostic.try_set_fix(|| {
                fixes::fix_unnecessary_list_comprehension_set(
                    checker.locator,
                    checker.stylist,
                    expr,
                )
            });
        }
        checker.diagnostics.push(diagnostic);
    }
}
