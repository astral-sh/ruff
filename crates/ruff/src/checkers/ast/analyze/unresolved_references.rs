use ruff_diagnostics::Diagnostic;
use ruff_python_semantic::Exceptions;

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

                checker.diagnostics.push(Diagnostic::new(
                    pyflakes::rules::UndefinedName {
                        name: reference.name(checker.locator).to_string(),
                    },
                    reference.range(),
                ));
            }
        }
    }
}
