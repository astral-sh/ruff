use ruff_diagnostics::{Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_semantic::Binding;
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for the presence of unused variables in unpacked assignments.
///
/// ## Why is this bad?
/// A variable that is defined but never used can confuse readers.
///
/// If a variable is intentionally defined-but-not-used, it should be
/// prefixed with an underscore, or some other value that adheres to the
/// [`lint.dummy-variable-rgx`] pattern.
///
/// ## Example
///
/// ```python
/// def get_pair():
///     return 1, 2
///
///
/// def foo():
///     x, y = get_pair()
///     return x
/// ```
///
/// Use instead:
///
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
        Some("Prefix it with an underscore or any other dummy variable pattern".to_string())
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
pub(crate) fn unused_unpacked_variable(checker: &Checker, name: &str, binding: &Binding) {
    if !binding.is_unpacked_assignment() {
        return;
    }

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
