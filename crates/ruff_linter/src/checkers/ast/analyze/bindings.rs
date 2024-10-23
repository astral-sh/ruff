use ruff_diagnostics::{Diagnostic, Fix};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{flake8_import_conventions, flake8_pyi, pyflakes, pylint, ruff};

/// Run lint rules over the [`Binding`]s.
pub(crate) fn bindings(checker: &mut Checker) {
    if !checker.any_enabled(&[
        Rule::InvalidAllFormat,
        Rule::InvalidAllObject,
        Rule::NonAsciiName,
        Rule::UnaliasedCollectionsAbcSetImport,
        Rule::UnconventionalImportAlias,
        Rule::UnsortedDunderSlots,
        Rule::UnusedVariable,
    ]) {
        return;
    }

    for binding in &*checker.semantic.bindings {
        if checker.enabled(Rule::UnusedVariable) {
            if binding.kind.is_bound_exception()
                && binding.is_unused()
                && !checker
                    .settings
                    .dummy_variable_rgx
                    .is_match(binding.name(checker.locator))
            {
                let mut diagnostic = Diagnostic::new(
                    pyflakes::rules::UnusedVariable {
                        name: binding.name(checker.locator).to_string(),
                    },
                    binding.range(),
                );
                diagnostic.try_set_fix(|| {
                    pyflakes::fixes::remove_exception_handler_assignment(binding, checker.locator)
                        .map(Fix::safe_edit)
                });
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker.enabled(Rule::InvalidAllFormat) {
            if let Some(diagnostic) = pylint::rules::invalid_all_format(binding) {
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker.enabled(Rule::InvalidAllObject) {
            if let Some(diagnostic) = pylint::rules::invalid_all_object(binding) {
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker.enabled(Rule::NonAsciiName) {
            if let Some(diagnostic) = pylint::rules::non_ascii_name(binding, checker.locator) {
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker.enabled(Rule::UnconventionalImportAlias) {
            if let Some(diagnostic) = flake8_import_conventions::rules::unconventional_import_alias(
                checker,
                binding,
                &checker.settings.flake8_import_conventions.aliases,
            ) {
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker.enabled(Rule::UnaliasedCollectionsAbcSetImport) {
            if let Some(diagnostic) =
                flake8_pyi::rules::unaliased_collections_abc_set_import(checker, binding)
            {
                checker.diagnostics.push(diagnostic);
            }
        }
        if checker.enabled(Rule::UnsortedDunderSlots) {
            if let Some(diagnostic) = ruff::rules::sort_dunder_slots(checker, binding) {
                checker.diagnostics.push(diagnostic);
            }
        }
    }
}
