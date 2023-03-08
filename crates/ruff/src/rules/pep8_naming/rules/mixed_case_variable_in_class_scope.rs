use rustpython_parser::ast::{Expr, Stmt};

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::rules::pep8_naming::helpers;

#[violation]
pub struct MixedCaseVariableInClassScope {
    pub name: String,
}

impl Violation for MixedCaseVariableInClassScope {
    #[derive_message_formats]
    fn message(&self) -> String {
        let MixedCaseVariableInClassScope { name } = self;
        format!("Variable `{name}` in class scope should not be mixedCase")
    }
}

/// N815
pub fn mixed_case_variable_in_class_scope(
    checker: &mut Checker,
    expr: &Expr,
    stmt: &Stmt,
    name: &str,
) {
    if checker
        .settings
        .pep8_naming
        .ignore_names
        .iter()
        .any(|ignore_name| ignore_name == name)
    {
        return;
    }
    if helpers::is_mixed_case(name) && !helpers::is_namedtuple_assignment(checker, stmt) {
        checker.diagnostics.push(Diagnostic::new(
            MixedCaseVariableInClassScope {
                name: name.to_string(),
            },
            Range::from(expr),
        ));
    }
}
