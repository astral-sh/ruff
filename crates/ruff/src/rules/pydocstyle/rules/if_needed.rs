use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::cast;
use crate::ast::helpers::identifier_range;
use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};
use crate::registry::Diagnostic;
use crate::violation::Violation;
use crate::visibility::is_overload;

define_violation!(
    pub struct SkipDocstring;
);
impl Violation for SkipDocstring {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Function decorated with `@overload` shouldn't contain a docstring")
    }
}

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
        SkipDocstring,
        identifier_range(stmt, checker.locator),
    ));
}
