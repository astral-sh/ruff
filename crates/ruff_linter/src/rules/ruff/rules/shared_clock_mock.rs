use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{Expr, ExprCall};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::Ranged;

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for mocks of shared clock and sleep functions in `time` and `asyncio`.
///
/// ## Why is this bad?
/// Python modules are shared within a process. Patching `time.sleep` also affects
/// other threads that call it. Patching `asyncio.sleep` can affect unrelated tasks.
/// Those callers can consume a mock's finite `side_effect` sequence, add unexpected
/// calls to assertions, or lose the delays that normally prevent busy loops.
///
/// Qualifying the target with a consuming module, as in `worker.time.sleep`, still
/// changes the shared function if `worker` imports `time`. Instead, replace the
/// consuming module's `time` or `asyncio` binding with a private object. Preserve
/// any other attributes used by that module, and use an asynchronous replacement
/// for `asyncio.sleep`.
///
/// ## Example
/// ```python
/// from unittest.mock import Mock, patch
///
/// from example import worker
///
/// sleep = Mock()
/// with patch.object(worker.time, "sleep", sleep):
///     worker.retry()
/// ```
///
/// Use instead:
/// ```python
/// from types import SimpleNamespace
/// from unittest.mock import Mock, patch
///
/// from example import worker
///
/// sleep = Mock()
/// with patch.object(worker, "time", SimpleNamespace(sleep=sleep)):
///     worker.retry()
/// ```
///
/// ## Known problems
/// This rule recognizes `unittest.mock.patch`, `patch.object`, pytest-mock's
/// conventional fixture names, and `monkeypatch.setattr`. It also recognizes
/// locally constructed `pytest.MonkeyPatch` instances.
///
/// Targets ending in `.time` or `.asyncio` are assumed to refer to the corresponding
/// standard-library module. This can produce false positives when an attribute
/// already holds a private clock. Dynamic string targets and aliases of these
/// attributes may not be recognized. Direct assignments are not checked.
///
/// ## References
/// - [Python documentation: Where to patch](https://docs.python.org/3/library/unittest.mock.html#where-to-patch)
/// - [pytest documentation: Monkeypatching](https://docs.pytest.org/en/stable/how-to/monkeypatch.html)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION", category = Category::Suspicious)]
pub(crate) struct SharedClockMock {
    function: String,
}

impl Violation for SharedClockMock {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { function } = self;
        format!("Mocking `{function}` may affect unrelated threads or tasks")
    }
}

#[derive(Clone, Copy)]
enum PatchApi {
    String,
    Object,
    MonkeyPatch,
}

impl PatchApi {
    /// Recognize imported patch functions before falling back to fixture names.
    fn from_call(checker: &Checker, call: &ExprCall) -> Option<Self> {
        if let Some(name) = checker
            .semantic()
            .resolve_qualified_name(&call.func)
            .or_else(|| typing::resolve_assignment(&call.func, checker.semantic()))
        {
            return match name.segments() {
                ["unittest", "mock", "patch"] | ["mock", "patch"] => Some(Self::String),
                ["unittest", "mock", "patch", "object"] | ["mock", "patch", "object"] => {
                    Some(Self::Object)
                }
                ["pytest", "MonkeyPatch", "setattr"]
                | ["_pytest", "monkeypatch", "MonkeyPatch", "setattr"] => Some(Self::MonkeyPatch),
                _ => None,
            };
        }

        let name = UnqualifiedName::from_expr(&call.func)?;
        match name.segments() {
            ["monkeypatch", "setattr"] => Some(Self::MonkeyPatch),
            [
                "mocker" | "class_mocker" | "module_mocker" | "package_mocker" | "session_mocker",
                "patch",
            ] => Some(Self::String),
            [
                "mocker" | "class_mocker" | "module_mocker" | "package_mocker" | "session_mocker",
                "patch",
                "object",
            ] => Some(Self::Object),
            _ => None,
        }
    }
}

/// `shared-clock-mock`
pub(crate) fn shared_clock_mock(checker: &Checker, call: &ExprCall) {
    let Some(api) = PatchApi::from_call(checker, call) else {
        return;
    };
    let Some(target) = call.arguments.find_argument_value("target", 0) else {
        return;
    };

    let (module, member) = match api {
        PatchApi::String | PatchApi::MonkeyPatch
            if let Some(target) = target.as_string_literal_expr() =>
        {
            let Some((module, member)) = target.value.to_str().rsplit_once('.') else {
                return;
            };
            (module.rsplit('.').next(), member)
        }
        PatchApi::Object | PatchApi::MonkeyPatch => {
            let attribute = match api {
                PatchApi::MonkeyPatch => "name",
                _ => "attribute",
            };
            let Some(member) = call
                .arguments
                .find_argument_value(attribute, 1)
                .and_then(Expr::as_string_literal_expr)
            else {
                return;
            };
            (clock_module(checker, target), member.value.to_str())
        }
        PatchApi::String => return,
    };

    if let Some(module) = module
        && matches!(
            (module, member),
            (
                "time",
                "sleep"
                    | "time"
                    | "time_ns"
                    | "monotonic"
                    | "monotonic_ns"
                    | "perf_counter"
                    | "perf_counter_ns"
            ) | ("asyncio", "sleep")
        )
    {
        checker.report_diagnostic(
            SharedClockMock {
                function: format!("{module}.{member}"),
            },
            target.range(),
        );
    }
}

/// Resolve direct imports and use the final attribute as a heuristic for consumers.
fn clock_module<'a>(checker: &Checker, target: &'a Expr) -> Option<&'a str> {
    if let Some(name) = checker.semantic().resolve_qualified_name(target) {
        match name.segments() {
            ["time"] => return Some("time"),
            ["asyncio"] => return Some("asyncio"),
            _ => {}
        }
    }

    target
        .as_attribute_expr()
        .map(|target| target.attr.as_str())
}
