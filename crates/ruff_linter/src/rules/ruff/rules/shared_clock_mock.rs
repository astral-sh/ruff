use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::name::UnqualifiedName;
use ruff_python_ast::{Expr, ExprAttribute, ExprCall, Stmt};
use ruff_python_semantic::analyze::typing;
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

/// ## What it does
/// Checks for patches, assignments, and deletions of shared clock and sleep
/// functions in `time` and `asyncio`.
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
/// conventional fixture names, and pytest's `monkeypatch.setattr` and
/// `monkeypatch.delattr`, including locally constructed `pytest.MonkeyPatch`
/// instances.
///
/// Targets ending in `.time` or `.asyncio` are assumed to refer to the corresponding
/// standard-library module, except for attributes directly on `self` or `cls`.
/// This can produce false positives when an attribute
/// already holds a private clock, for example when a pytest fixture replaced
/// the consuming module's binding before the test ran. Dynamic string targets
/// and aliases of these attributes may not be recognized.
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
        format!("Changing `{function}` may affect unrelated threads or tasks")
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
                ["pytest", "MonkeyPatch", "setattr" | "delattr"]
                | [
                    "_pytest",
                    "monkeypatch",
                    "MonkeyPatch",
                    "setattr" | "delattr",
                ] => Some(Self::MonkeyPatch),
                _ => None,
            };
        }

        let name = UnqualifiedName::from_expr(&call.func)?;
        match name.segments() {
            ["monkeypatch", "setattr" | "delattr"] => Some(Self::MonkeyPatch),
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

    let (module, member, range) = match api {
        PatchApi::String | PatchApi::MonkeyPatch
            if let Some(target) = target.as_string_literal_expr() =>
        {
            let Some((module, member)) = target.value.to_str().rsplit_once('.') else {
                return;
            };
            (module.rsplit('.').next(), member, target.range())
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
            (
                clock_module(checker, target),
                member.value.to_str(),
                target.range().cover(member.range()),
            )
        }
        PatchApi::String => return,
    };

    report_shared_clock(checker, module, member, range);
}

/// `shared-clock-mock`
pub(crate) fn shared_clock_attribute(checker: &Checker, attribute: &ExprAttribute) {
    // An annotation without a value does not write to the attribute.
    if let Stmt::AnnAssign(assignment) = checker.semantic().current_statement()
        && assignment.value.is_none()
        && assignment.target.range() == attribute.range()
    {
        return;
    }
    report_shared_clock(
        checker,
        clock_module(checker, &attribute.value),
        attribute.attr.as_str(),
        attribute.range(),
    );
}

fn report_shared_clock(checker: &Checker, module: Option<&str>, member: &str, range: TextRange) {
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
        let mut diagnostic = checker.report_diagnostic(
            SharedClockMock {
                function: format!("{module}.{member}"),
            },
            range,
        );
        diagnostic.help("Replace the binding in the consuming module with a private clock");
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

    let attribute = target.as_attribute_expr()?;
    if let Some(receiver) = attribute.value.as_name_expr()
        && matches!(receiver.id.as_str(), "self" | "cls")
        && checker
            .semantic()
            .resolve_name(receiver)
            .is_some_and(|id| checker.semantic().binding(id).kind.is_argument())
    {
        return None;
    }
    Some(attribute.attr.as_str())
}
