use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::Modules;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::fix::edits::pad;
use crate::rules::flake8_async::helpers::MethodName;

/// ## What it does
/// Checks for calls to trio functions that are not immediately awaited.
///
/// ## Why is this bad?
/// Many of the functions exposed by trio are asynchronous, and must be awaited
/// to take effect. Calling a trio function without an `await` can lead to
/// `RuntimeWarning` diagnostics and unexpected behaviour.
///
/// ## Example
/// ```python
/// async def double_sleep(x):
///     trio.sleep(2 * x)
/// ```
///
/// Use instead:
/// ```python
/// async def double_sleep(x):
///     await trio.sleep(2 * x)
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as unsafe, as adding an `await` to a function
/// call changes its semantics and runtime behavior.
#[derive(ViolationMetadata)]
pub(crate) struct TrioSyncCall {
    method_name: MethodName,
}

impl Violation for TrioSyncCall {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { method_name } = self;
        format!("Call to `{method_name}` is not immediately awaited")
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add `await`".to_string())
    }
}

/// ASYNC105
pub(crate) fn sync_call(checker: &Checker, call: &ExprCall) {
    if !checker.semantic().seen_module(Modules::TRIO) {
        return;
    }

    let Some(method_name) = ({
        let Some(qualified_name) = checker
            .semantic()
            .resolve_qualified_name(call.func.as_ref())
        else {
            return;
        };
        MethodName::try_from(&qualified_name)
    }) else {
        return;
    };

    if !method_name.is_async() {
        return;
    }

    if checker
        .semantic()
        .current_expression_parent()
        .is_some_and(Expr::is_await_expr)
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(TrioSyncCall { method_name }, call.range);
    if checker.semantic().in_async_context() {
        diagnostic.set_fix(Fix::unsafe_edit(Edit::insertion(
            pad(
                "await".to_string(),
                TextRange::new(call.func.start(), call.func.start()),
                checker.locator(),
            ),
            call.func.start(),
        )));
    }
    checker.report_diagnostic(diagnostic);
}
