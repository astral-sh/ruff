use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::cast;
use ruff_python_ast::helpers::identifier_range;
use ruff_python_semantic::analyze::visibility::is_overload;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::{DefinitionKind, Docstring};

#[violation]
pub struct OverloadWithDocstring;

impl Violation for OverloadWithDocstring {
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
    if !is_overload(&checker.ctx, cast::decorator_list(stmt)) {
        return;
    }
    checker.diagnostics.push(Diagnostic::new(
        OverloadWithDocstring,
        identifier_range(stmt, checker.locator),
    ));
}
