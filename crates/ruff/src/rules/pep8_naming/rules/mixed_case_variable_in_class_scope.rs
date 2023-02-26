use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::rules::pep8_naming::helpers;
use crate::violation::Violation;

define_violation!(
    pub struct MixedCaseVariableInClassScope {
        pub name: String,
    }
);
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
            Range::from_located(expr),
        ));
    }
}
