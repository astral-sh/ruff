use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::ExprName;
use ruff_python_semantic::{Imported, ScopeId};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::flake8_type_checking::helpers::quote_annotation;
use crate::{Fix, FixAvailability, Violation};

/// ## What it does
/// Checks for unquoted annotation references to imports defined only inside
/// an `if TYPE_CHECKING:` block.
///
/// ## Why is this bad?
/// Python evaluates function parameter and return annotations, and
/// module-level and class-body `AnnAssign` annotations, at runtime. If the
/// annotation references a symbol that is only imported under
/// `typing.TYPE_CHECKING`, the reference raises `NameError` when the
/// surrounding function or class definition executes.
///
/// Wrapping the annotation in a string literal defers its evaluation, so
/// the reference is only resolved by tools that explicitly inspect type
/// hints (such as `typing.get_type_hints`).
///
/// The rule is silent in three cases:
///
/// - Local-variable annotations (Python never evaluates those).
/// - Files using `from __future__ import annotations`.
/// - Code targeting Python 3.14 or later, where PEP 649 defers all
///   annotations by default.
///
/// ## Example
/// ```python
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     from threading import Thread
///
///
/// def foo(t: Thread) -> None: ...  # raises `NameError` at definition time
/// ```
///
/// Use instead:
/// ```python
/// from typing import TYPE_CHECKING
///
/// if TYPE_CHECKING:
///     from threading import Thread
///
///
/// def foo(t: "Thread") -> None: ...
/// ```
///
/// ## Fix safety
/// The fix is marked unsafe when the annotation range intersects a comment,
/// since the rewrite drops any inline comments inside the quoted expression.
///
/// ## References
/// - [PEP 563: Runtime annotation resolution and `TYPE_CHECKING`](https://peps.python.org/pep-0563/#runtime-annotation-resolution-and-type-checking)
/// - [PEP 649: Deferred evaluation of annotations using descriptors](https://peps.python.org/pep-0649/)
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "NEXT_RUFF_VERSION")]
pub(crate) struct UnquotedTypeCheckingOnlyImport {
    qualified_name: String,
}

impl Violation for UnquotedTypeCheckingOnlyImport {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { qualified_name } = self;
        format!(
            "Annotation `{qualified_name}` needs to be a string literal. Import is only available under `TYPE_CHECKING`."
        )
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add quotes".to_string())
    }
}

/// TC200
pub(crate) fn unquoted_type_checking_only_import(checker: &Checker, name: &ExprName) {
    let semantic = checker.semantic();

    // Skip references whose annotation context is not runtime-evaluated. The
    // `RUNTIME_EVALUATED_ANNOTATION` flag is already cleared by the annotation visitor for
    // local-variable annotations, `from __future__ import annotations` files, stubs, and
    // py3.14+ targets (PEP 649), so this single guard covers all four cases.
    if !semantic.in_runtime_evaluated_annotation() {
        return;
    }

    if semantic.in_string_type_definition() {
        return;
    }

    // Defer to TC004 when it's enabled: it owns the same diagnostic from the import side and
    // would otherwise race with this rule's fix.
    if checker.is_rule_enabled(Rule::RuntimeImportInTypeCheckingBlock) {
        return;
    }

    let Some(binding_id) = semantic.resolve_name(name) else {
        return;
    };
    let binding = semantic.binding(binding_id);

    let Some(import) = binding.as_any_import() else {
        return;
    };

    if !binding.context.is_typing() {
        return;
    }

    // A TYPE_CHECKING import inside a nested function scope cannot be rescued by quoting;
    // `get_type_hints` would still fail to resolve it at the call site.
    if binding.scope != ScopeId::global() {
        return;
    }

    // `quote_annotation` needs the reference's expression NodeId so it can walk up to the
    // smallest valid enclosing expression (subscript, attribute, call, `X | Y` union) and
    // quote that whole thing rather than just the bare name.
    let Some(reference) = binding
        .references()
        .map(|reference_id| semantic.reference(reference_id))
        .find(|reference| reference.range() == name.range())
    else {
        return;
    };
    let Some(expression_id) = reference.expression_id() else {
        return;
    };

    let edit = quote_annotation(
        expression_id,
        semantic,
        checker.stylist(),
        checker.locator(),
        checker.default_string_flags(),
    );

    let mut diagnostic = checker.report_diagnostic(
        UnquotedTypeCheckingOnlyImport {
            qualified_name: import.qualified_name().to_string(),
        },
        name.range(),
    );

    if checker.comment_ranges().intersects(edit.range()) {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
}
