use ruff_python_semantic::Exceptions;
use ruff_python_stdlib::builtins::version_builtin_was_added;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::pyflakes;

/// Run lint rules over all [`UnresolvedReference`] entities in the [`SemanticModel`].
pub(crate) fn unresolved_references(checker: &Checker) {
    if !checker.any_rule_enabled(&[Rule::UndefinedLocalWithImportStarUsage, Rule::UndefinedName]) {
        return;
    }

    for reference in checker.semantic.unresolved_references() {
        if reference.is_wildcard_import() {
            // F406
            checker.report_diagnostic_if_enabled(
                pyflakes::rules::UndefinedLocalWithImportStarUsage {
                    name: reference.name(checker.source()).to_string(),
                },
                reference.range(),
            );
        } else {
            // F821
            if checker.is_rule_enabled(Rule::UndefinedName) {
                if checker.semantic.in_no_type_check() {
                    continue;
                }

                // Avoid flagging if `NameError` is handled.
                if reference.exceptions().contains(Exceptions::NAME_ERROR) {
                    continue;
                }

                // Allow __path__.
                if checker.path.ends_with("__init__.py") {
                    if reference.name(checker.source()) == "__path__" {
                        continue;
                    }
                }

                let symbol_name = reference.name(checker.source());

                checker.report_diagnostic(
                    pyflakes::rules::UndefinedName {
                        name: symbol_name.to_string(),
                        minor_version_builtin_added: version_builtin_was_added(symbol_name),
                    },
                    reference.range(),
                );
            }
        }
    }
}
