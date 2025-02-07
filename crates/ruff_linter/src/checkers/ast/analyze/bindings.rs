use ruff_diagnostics::{Diagnostic, Fix};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::{
    flake8_import_conventions, flake8_pyi, flake8_pytest_style, flake8_type_checking, pyflakes,
    pylint, pyupgrade, refurb, ruff,
};

/// Run lint rules over the [`Binding`]s.
pub(crate) fn bindings(checker: &Checker) {
    if !checker.any_enabled(&[
        Rule::AssignmentInAssert,
        Rule::InvalidAllFormat,
        Rule::InvalidAllObject,
        Rule::NonAsciiName,
        Rule::UnaliasedCollectionsAbcSetImport,
        Rule::UnconventionalImportAlias,
        Rule::UnsortedDunderSlots,
        Rule::UnusedVariable,
        Rule::UnquotedTypeAlias,
        Rule::UsedDummyVariable,
        Rule::PytestUnittestRaisesAssertion,
        Rule::ForLoopWrites,
        Rule::CustomTypeVarForSelf,
        Rule::PrivateTypeParameter,
    ]) {
        return;
    }

    for (binding_id, binding) in checker.semantic.bindings.iter_enumerated() {
        if checker.enabled(Rule::UnusedVariable) {
            if binding.kind.is_bound_exception()
                && binding.is_unused()
                && !checker
                    .settings
                    .dummy_variable_rgx
                    .is_match(binding.name(checker.source()))
            {
                let mut diagnostic = Diagnostic::new(
                    pyflakes::rules::UnusedVariable {
                        name: binding.name(checker.source()).to_string(),
                    },
                    binding.range(),
                );
                diagnostic.try_set_fix(|| {
                    pyflakes::fixes::remove_exception_handler_assignment(binding, checker.locator)
                        .map(Fix::safe_edit)
                });
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::InvalidAllFormat) {
            if let Some(diagnostic) = pylint::rules::invalid_all_format(binding) {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::InvalidAllObject) {
            if let Some(diagnostic) = pylint::rules::invalid_all_object(binding) {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::NonAsciiName) {
            if let Some(diagnostic) = pylint::rules::non_ascii_name(binding, checker.locator) {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::UnconventionalImportAlias) {
            if let Some(diagnostic) = flake8_import_conventions::rules::unconventional_import_alias(
                checker,
                binding,
                &checker.settings.flake8_import_conventions.aliases,
            ) {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::UnaliasedCollectionsAbcSetImport) {
            if let Some(diagnostic) =
                flake8_pyi::rules::unaliased_collections_abc_set_import(checker, binding)
            {
                checker.report_diagnostic(diagnostic);
            }
        }
        if !checker.source_type.is_stub() && checker.enabled(Rule::UnquotedTypeAlias) {
            if let Some(diagnostics) =
                flake8_type_checking::rules::unquoted_type_alias(checker, binding)
            {
                checker.report_diagnostics(diagnostics);
            }
        }
        if checker.enabled(Rule::UnsortedDunderSlots) {
            if let Some(diagnostic) = ruff::rules::sort_dunder_slots(checker, binding) {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::UsedDummyVariable) {
            if let Some(diagnostic) = ruff::rules::used_dummy_variable(checker, binding, binding_id)
            {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::AssignmentInAssert) {
            if let Some(diagnostic) = ruff::rules::assignment_in_assert(checker, binding) {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::PytestUnittestRaisesAssertion) {
            if let Some(diagnostic) =
                flake8_pytest_style::rules::unittest_raises_assertion_binding(checker, binding)
            {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::ForLoopWrites) {
            if let Some(diagnostic) = refurb::rules::for_loop_writes_binding(checker, binding) {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::CustomTypeVarForSelf) {
            if let Some(diagnostic) =
                flake8_pyi::rules::custom_type_var_instead_of_self(checker, binding)
            {
                checker.report_diagnostic(diagnostic);
            }
        }
        if checker.enabled(Rule::PrivateTypeParameter) {
            if let Some(diagnostic) = pyupgrade::rules::private_type_parameter(checker, binding) {
                checker.report_diagnostic(diagnostic);
            }
        }
    }
}
