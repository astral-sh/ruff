use rustpython_ast::{Expr, Stmt};

use crate::ast::types::Range;
use crate::check_ast::Checker;
use crate::checks::CheckKind;
use crate::pep8_naming::helpers;
use crate::Check;

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
        checker.add_check(Check::new(
            CheckKind::NonLowercaseVariableInFunction(name.to_string()),
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
        checker.add_check(Check::new(
            CheckKind::MixedCaseVariableInClassScope(name.to_string()),
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
        checker.add_check(Check::new(
            CheckKind::MixedCaseVariableInGlobalScope(name.to_string()),
            Range::from_located(expr),
        ));
    }
}
