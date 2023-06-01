use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_semantic::scope::ScopeId;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for unused type annotations.
///
/// ## Why is this bad?
/// Type annotations are used to provide type hints to static analysis tools. If
/// they are not used, they are redundant.
///
/// ## Example
/// ```python
/// def foo():
///     bar: int
/// ```
///
/// ## References
/// - [PEP 484](https://peps.python.org/pep-0484/)
#[violation]
pub struct UnusedAnnotation {
    name: String,
}

impl Violation for UnusedAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnusedAnnotation { name } = self;
        format!("Local variable `{name}` is annotated but never used")
    }
}

/// F842
pub(crate) fn unused_annotation(checker: &mut Checker, scope: ScopeId) {
    let scope = &checker.semantic_model().scopes[scope];

    let bindings: Vec<_> = scope
        .bindings()
        .filter_map(|(name, binding_id)| {
            let binding = &checker.semantic_model().bindings[binding_id];
            if binding.kind.is_annotation()
                && !binding.is_used()
                && !checker.settings.dummy_variable_rgx.is_match(name)
            {
                Some((name.to_string(), binding.range))
            } else {
                None
            }
        })
        .collect();

    for (name, range) in bindings {
        checker
            .diagnostics
            .push(Diagnostic::new(UnusedAnnotation { name }, range));
    }
}
