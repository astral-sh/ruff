use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::pep8_naming::helpers;
use crate::{violations, Diagnostic};

/// N806
pub fn non_lowercase_variable_in_function(
    checker: &mut Checker,
    expr: &Expr,
    stmt: &Stmt,
    name: &str,
) {
    if name.to_lowercase() != name
        && !helpers::is_namedtuple_assignment(stmt, &checker.from_imports)
    {
        checker.checks.push(Diagnostic::new(
            violations::NonLowercaseVariableInFunction(name.to_string()),
            Range::from_located(expr),
        ));
    }
}

/// N815
pub fn mixed_case_variable_in_class_scope(
    checker: &mut Checker,
    expr: &Expr,
    stmt: &Stmt,
    name: &str,
) {
    if helpers::is_mixed_case(name)
        && !helpers::is_namedtuple_assignment(stmt, &checker.from_imports)
    {
        checker.checks.push(Diagnostic::new(
            violations::MixedCaseVariableInClassScope(name.to_string()),
            Range::from_located(expr),
        ));
    }
}

/// N816
pub fn mixed_case_variable_in_global_scope(
    checker: &mut Checker,
    expr: &Expr,
    stmt: &Stmt,
    name: &str,
) {
    if helpers::is_mixed_case(name)
        && !helpers::is_namedtuple_assignment(stmt, &checker.from_imports)
    {
        checker.checks.push(Diagnostic::new(
            violations::MixedCaseVariableInGlobalScope(name.to_string()),
            Range::from_located(expr),
        ));
    }
}
