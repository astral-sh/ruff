use std::collections::BTreeSet;

use ast::Expr;
use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{self as ast, Stmt};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for members that conflict with `__slots__`.
///
/// ## Why is this bad?
/// Defining a member with the same name as a slot entry in the `__slots__`
/// collection can lead to unexpected behavior and bugs. The `__slots__`
/// declaration is used to explicitly declare data members to optimize
/// memory usage by avoiding the creation of a `__dict__` for each instance.
/// If a member shares a name with a slot, it can overshadow the intended
/// slot, preventing it from functioning as a data attribute, which could
/// lead to attribute errors or incorrect data handling in the class.
///
/// Fortunately, Python will throw an error as soon as the file is loaded.
///
#[violation]
pub struct ClassVariableSlotsConflict {
    name: String,
}

impl Violation for ClassVariableSlotsConflict {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!(
            "`{}` in `__slots__` conflicts with class variable",
            self.name
        )
    }
}

/// PLE0242
pub(crate) fn class_variable_slots_conflict(checker: &mut Checker, body: &[Stmt]) {
    // First, collect all the attributes that are assigned to `__slots__`.
    let mut slots = BTreeSet::<String>::new();
    for statement in body {
        match statement {
            // Ex) `__slots__ = ("name",)`
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                let [Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() else {
                    continue;
                };

                if id == "__slots__" {
                    slots.extend(slots_attributes(value));
                }
            }

            // Ex) `__slots__: Tuple[str, ...] = ("name",)`
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                value: Some(value),
                ..
            }) => {
                let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                if id == "__slots__" {
                    slots.extend(slots_attributes(value));
                }
            }

            // Ex) `__slots__ += ("name",)`
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                if id == "__slots__" {
                    slots.extend(slots_attributes(value));
                }
            }
            _ => {}
        }
    }

    if slots.is_empty() {
        return;
    }

    // let mut seen_names = FxHashSet::default();
    for stmt in body {
        match stmt {
            Stmt::FunctionDef(ast::StmtFunctionDef { name, .. }) => {
                let method_name = name.to_string();
                if slots.contains(&method_name) {
                    checker.diagnostics.push(Diagnostic::new(
                        ClassVariableSlotsConflict { name: method_name },
                        name.range(),
                    ));
                }
            }
            Stmt::Assign(ast::StmtAssign { targets, .. }) => {
                for target in targets {
                    if let Expr::Name(ast::ExprName { id, .. }) = target {
                        if slots.contains(id) {
                            checker.diagnostics.push(Diagnostic::new(
                                ClassVariableSlotsConflict {
                                    name: id.to_string(),
                                },
                                target.range(),
                            ));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn slots_attributes(expr: &Expr) -> Vec<String> {
    // Ex) `__slots__ = ("name",)`
    let elts_iter = match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::List(ast::ExprList { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => Some(elts.iter().filter_map(|elt| match elt {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => Some(value.to_string()),
            _ => None,
        })),
        _ => None,
    };

    // Ex) `__slots__ = {"name": ...}`
    let keys_iter = match expr {
        Expr::Dict(ast::ExprDict { keys, .. }) => Some(keys.iter().filter_map(|key| match key {
            Some(Expr::StringLiteral(ast::ExprStringLiteral { value, .. })) => {
                Some(value.to_string())
            }
            _ => None,
        })),
        _ => None,
    };

    elts_iter
        .into_iter()
        .flatten()
        .chain(keys_iter.into_iter().flatten())
        .collect()
}
