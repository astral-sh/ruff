use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;
use crate::rules::flake8_bugbear::rules::is_infinite_iterator;
use crate::settings::types::PythonVersion;
use ruff_diagnostics::{Applicability, Diagnostic, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Arguments, ExprCall};

/// ## What it does
/// Checks for `itertools.batched` calls without an explicit `strict` parameter.
///
/// ## Why is this bad?
/// By default, if the length of the iterable is not divisible by
/// the second argument to `itertools.batched`, the last batch
/// will be shorter than the rest.
///
/// Pass `strict=True` to raise a `ValueError` if the batches are of non-uniform length.
/// Otherwise, pass `strict=False` to make the intention explicit.
///
/// ## Example
/// ```python
/// itertools.batched(iterable, n)
/// ```
///
/// Use instead:
/// ```python
/// itertools.batched(iterable, n, strict=True)
/// ```
///
/// ## Fix safety
/// The fix for this rule adds a `strict=False` argument.
/// It does not change runtime behaviour, since `False` is already the default value.
/// However, it is marked as unsafe as it might not preserve the original intention.
///
/// For calls that has `**kwargs`, the fix will be marked as display-only
/// due to the risk of introducing a duplicate keyword argument error.
///
/// ## Known deviations
/// Unlike the upstream `B911`, this rule will not report infinite iterators
/// (e.g., `itertools.cycle(...)`).
///
/// ## References
/// - [Python documentation: `batched`](https://docs.python.org/3/library/itertools.html#batched)
#[derive(ViolationMetadata)]
pub(crate) struct BatchedWithoutExplicitStrict;

impl Violation for BatchedWithoutExplicitStrict {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "`itertools.batched()` without an explicit `strict=` parameter".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add explicit value for parameter `strict=`".to_string())
    }
}

/// B911
pub(crate) fn batched_without_explicit_strict(checker: &mut Checker, call: &ExprCall) {
    if checker.settings.target_version < PythonVersion::Py313 {
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

    let Some(first_positional) = arguments.find_positional(0) else {
        return;
    };

    if is_infinite_iterator(first_positional, semantic) {
        return;
    }

    let diagnostic = Diagnostic::new(BatchedWithoutExplicitStrict, call.range);
    let fix = add_strict_fix(checker, arguments);

    checker.diagnostics.push(diagnostic.with_fix(fix));
}

#[inline]
fn add_strict_fix(checker: &Checker, arguments: &Arguments) -> Fix {
    let edit = add_argument(
        "strict=False",
        arguments,
        checker.comment_ranges(),
        checker.locator().contents(),
    );

    let applicability = if has_kwargs(arguments) {
        Applicability::DisplayOnly
    } else {
        Applicability::Unsafe
    };

    Fix::applicable_edit(edit, applicability)
}

#[inline]
fn has_kwargs(arguments: &Arguments) -> bool {
    arguments
        .keywords
        .iter()
        .any(|keyword| keyword.arg.is_none())
}
