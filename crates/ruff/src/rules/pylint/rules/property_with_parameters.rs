use rustpython_parser::ast::{Arguments, Expr, ExprKind, Stmt};

use ruff_macros::{define_violation, derive_message_formats};

use crate::ast::helpers::identifier_range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

define_violation!(
    pub struct PropertyWithParameters;
);
impl Violation for PropertyWithParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Cannot have defined parameters for properties")
    }
}

/// PLR0206
pub fn property_with_parameters(
    checker: &mut Checker,
    stmt: &Stmt,
    decorator_list: &[Expr],
    args: &Arguments,
) {
    if !decorator_list
        .iter()
        .any(|d| matches!(&d.node, ExprKind::Name { id, .. } if id == "property"))
    {
        return;
    }
    if checker.is_builtin("property")
        && args
            .args
            .iter()
            .chain(args.posonlyargs.iter())
            .chain(args.kwonlyargs.iter())
            .count()
            > 1
    {
        checker.diagnostics.push(Diagnostic::new(
            PropertyWithParameters,
            identifier_range(stmt, checker.locator),
        ));
    }
}
