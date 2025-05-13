//! ALLURE002 — класс Test* без @pytest.mark.team_*

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{Decorator, Expr, StmtClassDef};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// PTQACORE002
#[derive(ViolationMetadata)]
pub(crate) struct MissingTeamMarker;

impl Violation for MissingTeamMarker {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Test class is missing a `@pytest.mark.team_*` decorator".to_string()
    }
}

pub(crate) fn team_marker_exists(checker: &mut Checker, cls: &StmtClassDef) {
    if !cls.name.as_str().starts_with("Test") {
        return;
    }
    if !has_team_marker(&cls.decorator_list) {
        checker.report_diagnostic(Diagnostic::new(MissingTeamMarker, cls.range()));
    }
}

fn has_team_marker(decorators: &[Decorator]) -> bool {
    decorators.iter().any(|decorator| {
        // ищем @pytest.mark.team_*
        let Expr::Attribute(attr) = &decorator.expression else {
            return false;
        };
        if !attr.attr.as_str().starts_with("team_") {
            return false;
        }
        let Expr::Attribute(parent) = attr.value.as_ref() else {
            return false;
        };
        parent.attr.as_str() == "mark"
            && matches!(parent.value.as_ref(), Expr::Name(name) if name.id.as_str() == "pytest")
    })
}
