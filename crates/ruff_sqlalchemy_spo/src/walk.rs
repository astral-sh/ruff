//! Class-body walk: turn a `StmtClassDef` into a [`RawClass`].
//!
//! Mirrors `ruff_python_spo`'s manual class-body dispatch — no visitor trait
//! at this level, just a match over the body statements. A class is treated
//! as a Flask-SQLAlchemy model when it subclasses `db.Model`.

use ruff_python_ast::{Stmt, StmtAssign, StmtClassDef};

use crate::columns::{column_from_call, is_db_attr};
use crate::functions::analyze_method;
use crate::relationships::relationship_from_call;
use crate::{RawClass, expr_str, name_id};

/// Walk a class definition into a [`RawClass`], or `None` if it isn't a
/// `db.Model` subclass.
#[must_use]
pub(crate) fn walk_class(class: &StmtClassDef) -> Option<RawClass> {
    if !is_db_model(class) {
        return None;
    }

    let mut tablename = None;
    let mut columns = Vec::new();
    let mut relationships = Vec::new();
    let mut functions = Vec::new();

    for stmt in &class.body {
        match stmt {
            Stmt::Assign(assign) => {
                let Some(target) = single_name_target(assign) else {
                    continue;
                };
                if target == "__tablename__" {
                    tablename = expr_str(&assign.value);
                } else if let Some(column) = column_from_call(target, &assign.value) {
                    columns.push(column);
                } else if let Some(rel) = relationship_from_call(target, &assign.value) {
                    relationships.push(rel);
                }
            }
            Stmt::FunctionDef(func) => functions.push(analyze_method(func)),
            _ => {}
        }
    }

    Some(RawClass {
        // The OGAR class name stays the Python class name (WoA's
        // convention) — `__tablename__`, recorded above, is NOT used as
        // the model identity (unlike Odoo's dotted `_name`).
        name: class.name.id.as_str().to_string(),
        tablename,
        columns,
        relationships,
        functions,
    })
}

/// `true` if the class subclasses `db.Model`.
fn is_db_model(class: &StmtClassDef) -> bool {
    class
        .arguments
        .as_deref()
        .is_some_and(|args| args.args.iter().any(|base| is_db_attr(base, "Model")))
}

/// The single LHS identifier of `x = ...`, or `None` for tuple/multiple
/// targets.
fn single_name_target(assign: &StmtAssign) -> Option<&str> {
    match assign.targets.as_slice() {
        [target] => name_id(target),
        _ => None,
    }
}
