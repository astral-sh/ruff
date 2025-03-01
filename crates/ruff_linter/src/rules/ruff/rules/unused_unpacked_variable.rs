use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::{Binding, Scope};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of unused variables in unpacked assignments.
///
/// ## Why is this bad?
/// A variable that is defined but not used is likely a mistake, and should
/// be removed to avoid confusion.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`lint.dummy-variable-rgx`] pattern.
///
/// Under [preview mode](https://docs.astral.sh/ruff/preview), this rule also
/// triggers on unused unpacked assignments (for example, `x, y = foo()`).
///
/// ## Example
/// ```python
/// def get_pair():
///     return 1, 2
///
/// def foo():
///     x, y = get_pair()
///     return x
/// ```
///
/// Use instead:
/// ```python
/// def foo():
///     x, _ = get_pair()
///     return x
/// ```
///
/// ## Options
/// - `lint.dummy-variable-rgx`
#[derive(ViolationMetadata)]
pub(crate) struct UnusedUnpackedVariable {
    pub name: String,
}

impl Violation for UnusedUnpackedVariable {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedUnpackedVariable { name } = self;
        format!("Unpacked variable `{name}` is never used")
    }

    fn fix_title(&self) -> Option<String> {
        let UnusedUnpackedVariable { name } = self;
        Some(format!("Remove assignment to unused variable `{name}`"))
    }
}

/// Generate a [`Edit`] to remove an unused variable assignment to a [`Binding`].
fn remove_unused_variable(binding: &Binding, checker: &Checker) -> Option<Fix> {
    let node_id = binding.source?;
    let isolation = Checker::isolation(checker.semantic().parent_statement_id(node_id));

    let name = binding.name(checker.source());
    let renamed = format!("_{name}");
    if checker.settings.dummy_variable_rgx.is_match(&renamed) {
        let edit = Edit::range_replacement(renamed, binding.range());

        return Some(Fix::unsafe_edit(edit).isolate(isolation));
    }

    None
}

/// RUF059
pub(crate) fn unused_unpacked_variable(checker: &Checker, scope: &Scope) {
    if scope.uses_locals() && scope.kind.is_function() {
        return;
    }

    for (name, binding) in scope
        .bindings()
        .map(|(name, binding_id)| (name, checker.semantic().binding(binding_id)))
        .filter_map(|(name, binding)| {
            if checker.settings.preview.is_enabled()
                && binding.is_unpacked_assignment()
                && binding.is_unused()
                && !binding.is_nonlocal()
                && !binding.is_global()
                && !checker.settings.dummy_variable_rgx.is_match(name)
            {
                return Some((name, binding));
            }

            None
        })
    {
        let mut diagnostic = Diagnostic::new(
            UnusedUnpackedVariable {
                name: name.to_string(),
            },
            binding.range(),
        );
        if let Some(fix) = remove_unused_variable(binding, checker) {
            diagnostic.set_fix(fix);
        }
        checker.report_diagnostic(diagnostic);
    }
}
