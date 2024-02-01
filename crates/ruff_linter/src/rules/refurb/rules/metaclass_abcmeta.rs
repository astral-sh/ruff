use itertools::Itertools;

use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::StmtClassDef;
use ruff_text_size::{Ranged, TextRange};

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;

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
/// class C(metaclass=ABCMeta):
///     pass
/// ```
///
/// Use instead:
/// ```python
/// class C(ABC):
///     pass
/// ```
///
/// ## References
/// - [Python documentation: `abc.ABC`](https://docs.python.org/3/library/abc.html#abc.ABC)
/// - [Python documentation: `abc.ABCMeta`](https://docs.python.org/3/library/abc.html#abc.ABCMeta)
#[violation]
pub struct MetaClassABCMeta;

impl AlwaysFixableViolation for MetaClassABCMeta {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Use of `metaclass=abc.ABCMeta` to define abstract base class")
    }

    fn fix_title(&self) -> String {
        format!("Replace with `abc.ABC`")
    }
}

/// FURB180
pub(crate) fn metaclass_abcmeta(checker: &mut Checker, class_def: &StmtClassDef) {
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
        .resolve_call_path(&keyword.value)
        .is_some_and(|call_path| matches!(call_path.as_slice(), ["abc", "ABCMeta"]))
    {
        return;
    }

    let mut diagnostic = Diagnostic::new(MetaClassABCMeta, keyword.range);

    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import("abc", "ABC"),
            keyword.range.start(),
            checker.semantic(),
        )?;
        Ok(if position > 0 {
            // When the `abc.ABCMeta` is not the first keyword, put `abc.ABC` before the first
            // keyword.
            Fix::safe_edits(
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
            )
        } else {
            Fix::safe_edits(
                Edit::range_replacement(binding, keyword.range),
                [import_edit],
            )
        })
    });

    checker.diagnostics.push(diagnostic);
}
