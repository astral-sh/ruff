use ruff_macros::{ViolationMetadata, derive_message_formats};

use crate::Violation;

#[derive(ViolationMetadata)]
pub(crate) struct AsyncMock;

impl Violation for AsyncMock {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`unittest.mock.AsyncMock` without `spec` or `spec_set` argument".to_string()
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
            matches!(qualified_name.segments(), ["unittest", "mock", "AsyncMock"])
        })
    {
        if call.arguments.find_argument("spec", 0).is_none()
            && call.arguments.find_keyword("spec_set").is_none()
        {
            let mut diagnostic = checker.report_diagnostic(AsyncMock, call.func.range());
        }
    }
}
