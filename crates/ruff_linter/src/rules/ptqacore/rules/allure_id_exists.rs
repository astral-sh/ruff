use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Decorator, Expr, ExprCall, StmtFunctionDef};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// PTQACORE001
#[derive(ViolationMetadata)]
pub(crate) struct MissingAllureId;

impl Violation for MissingAllureId {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Test is missing an `@allure.id(...)` decorator".to_string()
    }
}

pub(crate) fn missing_allure_id(checker: &mut Checker, func: &StmtFunctionDef) {
    if !func.name.as_str().starts_with("test") {
        return;
    }
    if !has_allure_id(&func.decorator_list) {
        checker.report_diagnostic(Diagnostic::new(MissingAllureId, func.range()));
    }
}

fn has_allure_id(decorators: &[Decorator]) -> bool {
    decorators.iter().any(|decorator| {
        let Expr::Call(ExprCall { func, .. }) = &decorator.expression else {
            return false;
        };
        match func.as_ref() {
            // @allure.id(...)
            Expr::Attribute(attr)
            if attr.attr.as_str() == "id"
                && matches!(attr.value.as_ref(), Expr::Name(name) if name.id.as_str() == "allure") =>
                {
                    true
                }
            // @allure_id(...)
            Expr::Name(name) if name.id.as_str() == "allure_id" => true,
            _ => false,
        }
    })
}
