use ruff_diagnostics::Diagnostic;
use ruff_python_semantic::Exceptions;
use ruff_python_stdlib::builtins::version_builtin_was_added;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::pyflakes;

/// Run lint rules over all [`UnresolvedReference`] entities in the [`SemanticModel`].
pub(crate) fn unresolved_references(checker: &Checker) {
    if !checker.any_enabled(&[Rule::UndefinedLocalWithImportStarUsage, Rule::UndefinedName]) {
        return;
    }

    for reference in checker.semantic.unresolved_references() {
        if reference.is_wildcard_import() {
            if checker.enabled(Rule::UndefinedLocalWithImportStarUsage) {
                checker.report_diagnostic(Diagnostic::new(
                    pyflakes::rules::UndefinedLocalWithImportStarUsage {
                        name: reference.name(checker.source()).to_string(),
                    },
                    reference.range(),
                ));
            }
        } else {
            if checker.enabled(Rule::UndefinedName) {
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

                checker.report_diagnostic(Diagnostic::new(
                    pyflakes::rules::UndefinedName {
                        name: symbol_name.to_string(),
                        minor_version_builtin_added: version_builtin_was_added(symbol_name),
                    },
                    reference.range(),
                ));
            }
        }
    }
}
