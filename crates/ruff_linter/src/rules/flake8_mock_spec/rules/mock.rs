use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

#[derive(ViolationMetadata)]
pub(crate) struct Mock;

impl Violation for Mock {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`unittest.mock.Mock` without `spec` or `spec_set` argument".to_string()
    }
}

pub(crate) fn mock(checker: &Checker, call: &ast::ExprCall) {
    if !checker.semantic().seen_module(Modules::UNITTEST) {
        return;
    }

    if checker
        .semantic()
        .resolve_qualified_name(&call.func)
        .is_some_and(|qualified_name| {
            matches!(qualified_name.segments(), ["unittest", "mock", "Mock"])
        })
    {
        if call.arguments.find_argument("spec", 0).is_none()
            && call.arguments.find_keyword("spec_set").is_none()
        {
            let mut diagnostic = checker.report_diagnostic(Mock, call.func.range());
        }
    }
}
