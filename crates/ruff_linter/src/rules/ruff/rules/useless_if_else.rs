use crate::checkers::ast::Checker;
use crate::fix::snippet::SourceCodeSnippet;
use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast as ast;
use ruff_python_ast::comparable::ComparableExpr;
use ruff_python_ast::helpers::Truthiness;
use ruff_text_size::Ranged;

/// ## What it does
/// Checks for useless if-else conditions with identical arms.
///
/// ## Why is this bad?
/// Useless if-else conditions add unnecessary complexity to the code without
/// providing any logical benefit.
///
/// Assigning the value directly is clearer and more explicit, and
/// should be preferred.
///
/// ## Example
/// ```python
/// # Bad
/// foo = x if y else x
/// ```
///
/// Use instead:
/// ```python
/// # Good
/// foo = x
/// ```
#[violation]
pub struct UselessIfElse {
    body: Option<SourceCodeSnippet>,
}

impl Violation for UselessIfElse {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Useless if-else condition")
    }

    fn fix_title(&self) -> Option<String> {
        if let Some(body) = &self.body.as_ref().and_then(SourceCodeSnippet::full_display) {
            Some(format!("Assign `{body}` directly"))
        } else {
            Some("Remove useless if-else condition".to_string())
        }
    }
}

/// RUF031
pub(crate) fn useless_if_else(checker: &mut Checker, if_expr: &ast::ExprIf) {
    let ast::ExprIf {
        body,
        test,
        orelse,
        range,
    } = if_expr;

    // Skip if the body and orelse are not the same
    if ComparableExpr::from(body) != ComparableExpr::from(orelse) {
        return;
    }

    let truthiness = Truthiness::from_expr(test, |id| checker.semantic().has_builtin_binding(id));

    let (body, fix) = if matches!(truthiness, Truthiness::Unknown) {
        (None, None)
    } else {
        // if there are no potential side effects in the test, we can safely remove the if-else
        (
            Some(SourceCodeSnippet::from_str(
                checker.locator().slice(body.as_ref()),
            )),
            Some(Fix::safe_edit(Edit::deletion(body.end(), orelse.end()))),
        )
    };

    let mut diagnostic = Diagnostic::new(UselessIfElse { body }, *range);

    if let Some(fix) = fix {
        diagnostic.set_fix(fix);
    }

    checker.diagnostics.push(diagnostic);
}
