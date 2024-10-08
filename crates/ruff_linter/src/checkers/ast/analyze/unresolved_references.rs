use ruff_diagnostics::Diagnostic;
use ruff_python_semantic::Exceptions;
use ruff_python_stdlib::builtins::version_builtin_was_added;

use crate::checkers::ast::Checker;
use crate::codes::Rule;
use crate::rules::pyflakes;

/// Run lint rules over all [`UnresolvedReference`] entities in the [`SemanticModel`].
pub(crate) fn unresolved_references(checker: &mut Checker) {
    if !checker.any_enabled(&[Rule::UndefinedLocalWithImportStarUsage, Rule::UndefinedName]) {
        return;
    }

    for reference in checker.semantic.unresolved_references() {
        if reference.is_wildcard_import() {
            if checker.enabled(Rule::UndefinedLocalWithImportStarUsage) {
                checker.diagnostics.push(Diagnostic::new(
                    pyflakes::rules::UndefinedLocalWithImportStarUsage {
                        name: reference.name(checker.locator).to_string(),
                    },
                    reference.range(),
                ));
            }
        } else {
            if checker.enabled(Rule::UndefinedName) {
                // Avoid flagging if `NameError` is handled.
                if reference.exceptions().contains(Exceptions::NAME_ERROR) {
                    continue;
                }

                // Allow __path__.
                if checker.path.ends_with("__init__.py") {
                    if reference.name(checker.locator) == "__path__" {
                        continue;
                    }
                }

                let symbol_name = reference.name(checker.locator);

                checker.diagnostics.push(Diagnostic::new(
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
