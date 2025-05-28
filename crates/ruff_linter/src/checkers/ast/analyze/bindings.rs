use ruff_text_size::Ranged;

use crate::Fix;
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
                checker
                    .report_diagnostic(
                        pyflakes::rules::UnusedVariable {
                            name: binding.name(checker.source()).to_string(),
                        },
                        binding.range(),
                    )
                    .try_set_fix(|| {
                        pyflakes::fixes::remove_exception_handler_assignment(
                            binding,
                            checker.locator,
                        )
                        .map(Fix::safe_edit)
                    });
            }
        }
        if checker.enabled(Rule::InvalidAllFormat) {
            pylint::rules::invalid_all_format(checker, binding);
        }
        if checker.enabled(Rule::InvalidAllObject) {
            pylint::rules::invalid_all_object(checker, binding);
        }
        if checker.enabled(Rule::NonAsciiName) {
            pylint::rules::non_ascii_name(checker, binding);
        }
        if checker.enabled(Rule::UnconventionalImportAlias) {
            flake8_import_conventions::rules::unconventional_import_alias(
                checker,
                binding,
                &checker.settings.flake8_import_conventions.aliases,
            );
        }
        if checker.enabled(Rule::UnaliasedCollectionsAbcSetImport) {
            flake8_pyi::rules::unaliased_collections_abc_set_import(checker, binding);
        }
        if !checker.source_type.is_stub() && checker.enabled(Rule::UnquotedTypeAlias) {
            flake8_type_checking::rules::unquoted_type_alias(checker, binding);
        }
        if checker.enabled(Rule::UnsortedDunderSlots) {
            ruff::rules::sort_dunder_slots(checker, binding);
        }
        if checker.enabled(Rule::UsedDummyVariable) {
            ruff::rules::used_dummy_variable(checker, binding, binding_id);
        }
        if checker.enabled(Rule::AssignmentInAssert) {
            ruff::rules::assignment_in_assert(checker, binding);
        }
        if checker.enabled(Rule::PytestUnittestRaisesAssertion) {
            flake8_pytest_style::rules::unittest_raises_assertion_binding(checker, binding);
        }
        if checker.enabled(Rule::ForLoopWrites) {
            refurb::rules::for_loop_writes_binding(checker, binding);
        }
        if checker.enabled(Rule::CustomTypeVarForSelf) {
            flake8_pyi::rules::custom_type_var_instead_of_self(checker, binding);
        }
        if checker.enabled(Rule::PrivateTypeParameter) {
            pyupgrade::rules::private_type_parameter(checker, binding);
        }
    }
}
