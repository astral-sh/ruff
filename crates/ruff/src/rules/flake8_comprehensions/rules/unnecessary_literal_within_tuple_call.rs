use ruff_python_ast::{Expr, Keyword};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::AsRule;
use crate::rules::flake8_comprehensions::fixes;

use super::helpers;

/// ## What it does
/// Checks for `tuple` calls that take unnecessary list or tuple literals as
/// arguments.
///
/// ## Why is this bad?
/// It's unnecessary to use a list or tuple literal within a `tuple()` call,
/// since there is a literal syntax for these types.
///
/// If a list literal was passed, then it should be rewritten as a `tuple`
/// literal. Otherwise, if a tuple literal was passed, then the outer call
/// to `list()` should be removed.
///
/// ## Examples
/// ```python
/// tuple([1, 2])
/// tuple((1, 2))
/// ```
///
/// Use instead:
/// ```python
/// (1, 2)
/// (1, 2)
/// ```
#[violation]
pub struct UnnecessaryLiteralWithinTupleCall {
    literal: String,
}

impl AlwaysAutofixableViolation for UnnecessaryLiteralWithinTupleCall {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall { literal } = self;
        if literal == "list" {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (rewrite as a `tuple` \
                 literal)"
            )
        } else {
            format!(
                "Unnecessary `{literal}` literal passed to `tuple()` (remove the outer call to \
                 `tuple()`)"
            )
        }
    }

    fn autofix_title(&self) -> String {
        let UnnecessaryLiteralWithinTupleCall { literal } = self;
        {
            if literal == "list" {
                "Rewrite as a `tuple` literal".to_string()
            } else {
                "Remove outer `tuple` call".to_string()
            }
        }
    }
}

/// C409
pub(crate) fn unnecessary_literal_within_tuple_call(
    checker: &mut Checker,
    expr: &Expr,
    func: &Expr,
    args: &[Expr],
    keywords: &[Keyword],
) {
    if !keywords.is_empty() {
        return;
    }
    let Some(argument) = helpers::first_argument_with_matching_function("tuple", func, args) else {
        return;
    };
    if !checker.semantic().is_builtin("tuple") {
        return;
    }
    let argument_kind = match argument {
        Expr::Tuple(_) => "tuple",
        Expr::List(_) => "list",
        _ => return,
    };
    let mut diagnostic = Diagnostic::new(
        UnnecessaryLiteralWithinTupleCall {
            literal: argument_kind.to_string(),
        },
        expr.range(),
    );
    if checker.patch(diagnostic.kind.rule()) {
        diagnostic.try_set_fix(|| {
            fixes::fix_unnecessary_literal_within_tuple_call(
                expr,
                checker.locator(),
                checker.stylist(),
            )
            .map(Fix::suggested)
        });
    }
    checker.diagnostics.push(diagnostic);
}
