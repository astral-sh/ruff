use itertools::Itertools;
use ruff_diagnostics::Applicability;
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::StmtClassDef;
use ruff_python_semantic::analyze;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::fix::edits::{Parentheses, remove_argument};
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
/// The rule's fix will also be marked as unsafe if the class has
/// comments in its argument list that could be deleted.
///
///
/// ## References
/// - [Python documentation: `abc.ABC`](https://docs.python.org/3/library/abc.html#abc.ABC)
/// - [Python documentation: `abc.ABCMeta`](https://docs.python.org/3/library/abc.html#abc.ABCMeta)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.2.0")]
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
    // Determine whether the class definition contains at least one argument.
    let Some(arguments) = &class_def.arguments.as_ref() else {
        return;
    };

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

    let applicability = if !class_def.bases().is_empty() {
        // The class has base arguments (not just a `metaclass` keyword).
        // Applying the fix may change semantics, so it is considered unsafe.
        Applicability::Unsafe
    } else if checker.comment_ranges().intersects(arguments.range()) {
        // The `metaclass` keyword overlaps with a comment.
        // Applying the fix would remove or alter the comment, so it is unsafe.
        Applicability::Unsafe
    } else {
        // An empty class definition with no overlapping comments.
        // Applying the fix is considered safe.
        Applicability::Safe
    };
    let mut diagnostic = checker.report_diagnostic(MetaClassABCMeta, keyword.range);

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("abc", "ABC"),
            keyword.range.start(),
            checker.semantic(),
        )?;

        // Check the `abc.ABC` is in base classes.
        let has_abc = analyze::class::any_qualified_base_class(
            class_def,
            checker.semantic(),
            &|qualified_name| matches!(qualified_name.segments(), ["abc", "ABC"]),
        );

        let delete_metaclass_keyword = remove_argument(
            keyword,
            arguments,
            Parentheses::Remove,
            checker.source(),
            checker.tokens(),
        )?;

        Ok(if position > 0 {
            // When the `abc.ABCMeta` is not the first keyword and `abc.ABC` is not
            // in base classes put `abc.ABC` before the first keyword argument.
            if has_abc {
                Fix::applicable_edit(delete_metaclass_keyword, applicability)
            } else {
                Fix::applicable_edits(
                    delete_metaclass_keyword,
                    [
                        Edit::insertion(format!("{binding}, "), class_def.keywords()[0].start()),
                        import_edit,
                    ],
                    applicability,
                )
            }
        } else {
            let edit_action = if has_abc {
                // Class already inherits the `abc.ABC`, delete the `metaclass` keyword only.
                delete_metaclass_keyword
            } else {
                // Replace `metaclass` keyword by `abc.ABC`.
                Edit::range_replacement(binding, keyword.range)
            };

            Fix::applicable_edits(edit_action, [import_edit], applicability)
        })
    });
}
