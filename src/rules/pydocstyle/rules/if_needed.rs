use crate::ast::cast;
use crate::ast::helpers::identifier_range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::Diagnostic;
use crate::violations;
use crate::visibility::is_overload;

/// D418
pub fn if_needed(checker: &mut Checker, docstring: &Docstring) {
    let (
        DefinitionKind::Function(stmt)
        | DefinitionKind::NestedFunction(stmt)
        | DefinitionKind::Method(stmt)
    ) = docstring.kind else {
        return
    };
    if !is_overload(checker, cast::decorator_list(stmt)) {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        violations::SkipDocstring,
        identifier_range(stmt, checker.locator),
    ));
}
