use rustpython_ast::{Constant, Expr, ExprKind};

use crate::ast::cmp;
use crate::ast::helpers::unparse_expr;
use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checkers::ast::Checker;
use crate::registry::{Check, CheckCode, CheckKind};
use crate::violations;

#[derive(Debug, PartialEq)]
enum DictionaryKey<'a> {
    Constant(&'a Constant),
    Variable(&'a str),
}

impl<'a> TryFrom<&'a Expr> for DictionaryKey<'a> {
    type Error = ();

    fn try_from(expr: &'a Expr) -> Result<Self, Self::Error> {
        match &expr.node {
            ExprKind::Constant { value, .. } => Ok(DictionaryKey::Constant(value)),
            ExprKind::Name { id, .. } => Ok(DictionaryKey::Variable(id)),
            _ => Err(()),
        }
    }
}

/// F601
pub fn repeated_literal_keys(checker: &mut Checker, keys: &[Expr], values: &[Expr]) {
    let num_keys = keys.len();
    for i in 0..num_keys {
        let k1: Option<DictionaryKey> = (&keys[i]).try_into().ok();
        for j in i + 1..num_keys {
            let k2: Option<DictionaryKey> = (&keys[j]).try_into().ok();
            if let (Some(DictionaryKey::Constant(v1)), Some(DictionaryKey::Constant(v2))) =
                (&k1, &k2)
            {
                if v1 == v2 {
                    let mut check = Check::new(
                        violations::MultiValueRepeatedKeyLiteral(unparse_expr(
                            &keys[j],
                            checker.style,
                        )),
                        Range::from_located(&keys[j]),
                    );
                    if checker.patch(&CheckCode::F601) {
                        if cmp::expr(&values[i], &values[j]) {
                            check.amend(if j < num_keys - 1 {
                                Fix::deletion(keys[j].location, keys[j + 1].location)
                            } else {
                                Fix::deletion(
                                    values[j - 1].end_location.unwrap(),
                                    values[j].end_location.unwrap(),
                                )
                            });
                        }
                    }
                    checker.checks.push(check);
                }
            }
        }
    }
}

/// F602
pub fn repeated_variable_keys(checker: &mut Checker, keys: &[Expr], values: &[Expr]) {
    let num_keys = keys.len();
    for i in 0..num_keys {
        let k1: Option<DictionaryKey> = (&keys[i]).try_into().ok();
        for j in i + 1..num_keys {
            let k2: Option<DictionaryKey> = (&keys[j]).try_into().ok();
            if let (Some(DictionaryKey::Variable(v1)), Some(DictionaryKey::Variable(v2))) =
                (&k1, &k2)
            {
                if v1 == v2 {
                    let mut check = Check::new(
                        violations::MultiValueRepeatedKeyVariable((*v2).to_string()),
                        Range::from_located(&keys[j]),
                    );
                    if checker.patch(&CheckCode::F602) {
                        if cmp::expr(&values[i], &values[j]) {
                            check.amend(if j < num_keys - 1 {
                                Fix::deletion(keys[j].location, keys[j + 1].location)
                            } else {
                                Fix::deletion(
                                    values[j - 1].end_location.unwrap(),
                                    values[j].end_location.unwrap(),
                                )
                            });
                        }
                    }
                    checker.checks.push(check);
                }
            }
        }
    }
}
