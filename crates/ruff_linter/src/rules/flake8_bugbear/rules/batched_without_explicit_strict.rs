use crate::checkers::ast::Checker;
use crate::fix::edits::add_argument;
use crate::rules::flake8_bugbear::rules::is_infinite_iterator;
use crate::settings::types::PythonVersion;
use ruff_diagnostics::{AlwaysFixableViolation, Applicability, Diagnostic, Fix};
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
/// This rule's fix is marked as unsafe for `batched` calls that contain
/// `**kwargs`, as adding a `strict` keyword argument to such a call may lead
/// to a duplicate keyword argument error.
///
/// ## References
/// - [Python documentation: `batched`](https://docs.python.org/3/library/itertools.html#batched)
#[derive(ViolationMetadata)]
pub(crate) struct BatchedWithoutExplicitStrict;

impl AlwaysFixableViolation for BatchedWithoutExplicitStrict {
    #[derive_message_formats]
    fn message(&self) -> String {
        "`batched()` without an explicit `strict=` parameter".to_string()
    }

    fn fix_title(&self) -> String {
        "Add explicit value for parameter `strict=`".to_string()
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
    let first_positional = arguments.args.first();

    if !matches!(qualified_name.segments(), ["itertools", "batched"])
        || arguments.find_keyword("strict").is_some()
        || first_positional.is_some_and(|it| is_infinite_iterator(it, semantic))
    {
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
        Applicability::Unsafe
    } else {
        Applicability::Safe
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
