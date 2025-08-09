use itertools::Itertools;

use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StmtClassDef;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::{AlwaysFixableViolation, Edit, Fix};

/// ## What it does
/// Checks for uses of `metaclass=abc.ABCMeta` to define abstract base classes
/// (ABCs).
///
/// ## Why is this bad?
///
/// Instead of `class C(metaclass=abc.ABCMeta): ...`, use `class C(ABC): ...`
/// to define an abstract base class. Inheriting from the `ABC` wrapper class
/// is semantically identical to setting `metaclass=abc.ABCMeta`, but more
/// succinct.
///
/// ## Example
/// ```python
/// import abc
///
///
/// class C(metaclass=abc.ABCMeta):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// import abc
///
///
/// class C(abc.ABC):
///     pass
/// ```
///
/// ## Fix safety
/// The rule's fix is unsafe if the class has base classes. This is because the base classes might
/// be validating the class's other base classes (e.g., `typing.Protocol` does this) or otherwise
/// alter runtime behavior if more base classes are added.
///
/// ## References
/// - [Python documentation: `abc.ABC`](https://docs.python.org/3/library/abc.html#abc.ABC)
/// - [Python documentation: `abc.ABCMeta`](https://docs.python.org/3/library/abc.html#abc.ABCMeta)
#[derive(ViolationMetadata)]
pub(crate) struct MetaClassABCMeta;

impl AlwaysFixableViolation for MetaClassABCMeta {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Use of `metaclass=abc.ABCMeta` to define abstract base class".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace with `abc.ABC`".to_string()
    }
}

/// FURB180
pub(crate) fn metaclass_abcmeta(checker: &Checker, class_def: &StmtClassDef) {
    // Identify the `metaclass` keyword.
    let Some((position, keyword)) = class_def.keywords().iter().find_position(|&keyword| {
        keyword
            .arg
            .as_ref()
            .is_some_and(|arg| arg.as_str() == "metaclass")
    }) else {
        return;
    };

    // Determine whether it's assigned to `abc.ABCMeta`.
    if !checker
        .semantic()
        .resolve_qualified_name(&keyword.value)
        .is_some_and(|qualified_name| matches!(qualified_name.segments(), ["abc", "ABCMeta"]))
    {
        return;
    }

    let applicability = if class_def.bases().is_empty() {
        Applicability::Safe
    } else {
        Applicability::Unsafe
    };
    let mut diagnostic = checker.report_diagnostic(MetaClassABCMeta, keyword.range);

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("abc", "ABC"),
            keyword.range.start(),
            checker.semantic(),
        )?;
        Ok(if position > 0 {
            // When the `abc.ABCMeta` is not the first keyword, put `abc.ABC` before the first
            // keyword.
            Fix::applicable_edits(
                // Delete from the previous argument, to the end of the `metaclass` argument.
                Edit::range_deletion(TextRange::new(
                    class_def.keywords()[position - 1].end(),
                    keyword.end(),
                )),
                // Insert `abc.ABC` before the first keyword.
                [
                    Edit::insertion(format!("{binding}, "), class_def.keywords()[0].start()),
                    import_edit,
                ],
                applicability,
            )
        } else {
            Fix::applicable_edits(
                Edit::range_replacement(binding, keyword.range),
                [import_edit],
                applicability,
            )
        })
    });
}
