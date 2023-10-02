use crate::checkers::ast::Checker;
use ruff_diagnostics::AlwaysFixableViolation;
use ruff_diagnostics::Diagnostic;
use ruff_diagnostics::Edit;
use ruff_diagnostics::Fix;
use ruff_macros::derive_message_formats;
use ruff_macros::violation;
use ruff_python_ast::Constant;
use ruff_python_ast::Expr;
use ruff_python_ast::ExprConstant;
use ruff_text_size::Ranged;

#[violation]
pub struct FStringEqualToShorthand;

impl AlwaysFixableViolation for FStringEqualToShorthand {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use equal to shorthand")
    }
    fn fix_title(&self) -> String {
        format!("Replace with equal to shorthand")
    }
}

/// RUF018
pub(crate) fn f_string_equal_to_shorthand(checker: &mut Checker, _expr: &Expr, values: &[Expr]) {
    let mut predicate = None;
    values.into_iter().for_each(|value| match value {
        Expr::Constant(ExprConstant {
            value: Constant::Str(str),
            range,
        }) => predicate = Some((str.value.as_str(), range)),
        Expr::FormattedValue(expr) => {
            let Some((complete_predicate_str, predicate_range)) = predicate else {
                return;
            };
            let mut expr_str = checker.locator().slice(expr).chars();
            expr_str.next();
            expr_str.next_back();
            let predicate_str_trimmed = complete_predicate_str.trim_end();
            let end = complete_predicate_str.len() - predicate_str_trimmed.len();
            let mut predicate_str = predicate_str_trimmed.chars();
            let Some('=') = predicate_str.next_back() else {
                predicate = None;
                return;
            };
            let predicate_str = predicate_str.as_str();
            let predicate_str_trimmed = predicate_str.trim_end();
            let start = predicate_str.len() - predicate_str_trimmed.len();
            if !predicate_str_trimmed.ends_with(expr_str.as_str()) {
                predicate = None;
                return;
            }
            let mut diagnostic = Diagnostic::new(FStringEqualToShorthand, expr.range());
            diagnostic.set_fix(Fix::suggested_edits(
                Edit::range_replacement(
                    format!(
                        "{{{}{}={}}}",
                        expr_str.as_str(),
                        " ".repeat(start),
                        " ".repeat(end)
                    ),
                    expr.range(),
                ),
                [Edit::range_deletion(
                    predicate_range.add_start(
                        (complete_predicate_str
                            .len()
                            .saturating_sub(expr_str.as_str().len())
                            .saturating_sub(end + start + 1) as u32)
                            .into(),
                    ),
                )],
            ));
            checker.diagnostics.push(diagnostic);
            predicate = None
        }
        _ => {}
    });
}
