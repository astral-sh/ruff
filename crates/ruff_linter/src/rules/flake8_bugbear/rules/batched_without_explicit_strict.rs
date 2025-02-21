use crate::checkers::ast::Checker;
use crate::rules::flake8_bugbear::rules::is_infinite_iterable;
use ruff_diagnostics::{Diagnostic, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::ExprCall;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks for `itertools.batched` calls without an explicit `strict` parameter.
///
/// ## Why is this bad?
/// By default, if the length of the iterable is not divisible by
/// the second argument to `itertools.batched`, the last batch
/// will be shorter than the rest.
///
/// In Python 3.13, a `strict` parameter was added which allows controlling if the batches must be of uniform length.
/// Pass `strict=True` to raise a `ValueError` if the batches are of non-uniform length.
/// Otherwise, pass `strict=False` to make the intention explicit.
///
/// ## Example
/// ```python
/// itertools.batched(iterable, n)
/// ```
///
/// Use instead if the batches must be of uniform length:
/// ```python
/// itertools.batched(iterable, n, strict=True)
/// ```
///
/// Or if the batches can be of non-uniform length:
/// ```python
/// itertools.batched(iterable, n, strict=False)
/// ```
///
/// ## Known deviations
/// Unlike the upstream `B911`, this rule will not report infinite iterators
/// (e.g., `itertools.cycle(...)`).
///
/// ## Options
/// - `target-version`
///
/// ## References
/// - [Python documentation: `batched`](https://docs.python.org/3/library/itertools.html#batched)
#[derive(ViolationMetadata)]
pub(crate) struct BatchedWithoutExplicitStrict;

impl Violation for BatchedWithoutExplicitStrict {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::None;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`itertools.batched()` without an explicit `strict` parameter".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add an explicit `strict` parameter".to_string())
    }
}

/// B911
pub(crate) fn batched_without_explicit_strict(checker: &Checker, call: &ExprCall) {
    if checker.target_version() < PythonVersion::PY313 {
        return;
    }

    let semantic = checker.semantic();
    let (func, arguments) = (&call.func, &call.arguments);

    let Some(qualified_name) = semantic.resolve_qualified_name(func) else {
        return;
    };

    if !matches!(qualified_name.segments(), ["itertools", "batched"]) {
        return;
    }

    if arguments.find_keyword("strict").is_some() {
        return;
    }

    let Some(iterable) = arguments.find_positional(0) else {
        return;
    };

    if is_infinite_iterable(iterable, semantic) {
        return;
    }

    let diagnostic = Diagnostic::new(BatchedWithoutExplicitStrict, call.range);
    checker.report_diagnostic(diagnostic);
}
