use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::{
    Expr, ExprDict, ExprList, ExprName, ExprSet, ExprStarred, ExprStringLiteral, ExprTuple, Stmt,
    StmtAssign, StmtClassDef, StmtFunctionDef,
};
use ruff_text_size::TextRange;

use crate::checkers::ast::Checker;

use rustc_hash::FxHashMap;

/// ### What it does
/// It checks that there are no data members defined that have the same name as items within `__slots__`.
///
/// ### Why is this bad?
/// Since the idea behind `__slots__` is to disallow the creation of the `__dict__` and `__weakref__` attributes,
/// defining data members contradicts this, and a `ValueError` will be raised at run-time.   
///
/// ## Problematic code
/// ```python
/// class Person:
/// # +1: [class-variable-slots-conflict, class-variable-slots-conflict, class-variable-slots-conflict]
///     __slots__ = ("age", "name", "say_hi")
///     name = None
///
///     def __init__(self, age, name):
///         self.age = age
///         self.name = name
///
///     @property
///     def age(self):
///         return self.age
///
///     def say_hi(self):
///         print(f"Hi, I'm {self.name}.")
/// ```
///
/// ## Correct code
/// ```python
/// class Person:
///     __slots__ = ("_age", "name")
///
///     def __init__(self, age, name):
///         self._age = age
///         self.name = name
///
///     @property
///     def age(self):
///         return self._age
///
///     def say_hi(self):
///         print(f"Hi, I'm {self.name}.")
/// ```
///
/// ## References
/// - [Python documentation: `__slots__`](https://docs.python.org/3/reference/datamodel.html#slots)
#[violation]
pub struct ClassVariableSlotsConflict {
    slot_conflict: String,
}

impl Violation for ClassVariableSlotsConflict {
    #[derive_message_formats]
    fn message(&self) -> String {
        let Self { slot_conflict } = self;
        format!("Value `{slot_conflict}` in slots conflicts with class variable")
    }
}

fn get_slots_from_expr(expr: &Expr) -> Option<FxHashMap<&str, &TextRange>> {
    match expr {
        Expr::Dict(ExprDict { keys, .. }) => {
            if keys.is_empty() {
                None
            } else {
                Some(
                    keys.iter()
                        .filter_map(|item| {
                            if let Some(Expr::StringLiteral(ExprStringLiteral { range, value })) =
                                item
                            {
                                Some((value.to_str(), range))
                            } else {
                                None
                            }
                        })
                        .collect::<FxHashMap<_, _>>(),
                )
            }
        }
        Expr::List(ExprList { elts, .. })
        | Expr::Set(ExprSet { elts, .. })
        | Expr::Tuple(ExprTuple { elts, .. }) => {
            if elts.is_empty() {
                None
            } else {
                if let [Expr::Starred(ExprStarred { value, .. })] = &elts[..] {
                    get_slots_from_expr(value)
                } else {
                    Some(
                        elts.iter()
                            .filter_map(|item| {
                                if let Expr::StringLiteral(ExprStringLiteral { range, value }) =
                                    item
                                {
                                    Some((value.to_str(), range))
                                } else {
                                    None
                                }
                            })
                            .collect::<FxHashMap<_, _>>(),
                    )
                }
            }
        }
        _ => None,
    }
}

// Takes the body of the class and finds if `__slots__` is in there
fn get_slots(body: &[Stmt]) -> Option<FxHashMap<&str, &TextRange>> {
    for stmt in body {
        if let Stmt::Assign(StmtAssign { targets, value, .. }) = stmt {
            if let [Expr::Name(ExprName { id, .. }), ..] = &targets[..] {
                if id == "__slots__" {
                    return get_slots_from_expr(value);
                }
            } else if let [Expr::Tuple(ExprTuple { elts, .. })] = &targets[..] {
                if let Some(index) = elts.iter().enumerate().find_map(|item| {
                    if let (tuple_idx, Expr::Name(ExprName { id, .. })) = item {
                        if id == "__slots__" {
                            return Some(tuple_idx);
                        }
                    }
                    None
                }) {
                    // Some, find the element in the next tuple
                    if let Expr::Tuple(ExprTuple { elts, .. }) = &**value {
                        if index < elts.len() {
                            return get_slots_from_expr(&elts[index]);
                        }
                    }
                }
            }
        }
    }
    None
}

fn traverse_class_body(body: &[Stmt], slots: &FxHashMap<&str, &TextRange>) -> Vec<Diagnostic> {
    body.iter()
        .filter_map(|stmt| {
            match stmt {
                Stmt::Assign(StmtAssign { targets, .. }) => {
                    if let [Expr::Name(ExprName { id, .. })] = &targets[..] {
                        if id != "__slots__" {
                            if let Some(range) = slots.get(&id.as_str()) {
                                return Some(Diagnostic::new(
                                    ClassVariableSlotsConflict {
                                        slot_conflict: id.to_owned(),
                                    },
                                    *range.to_owned(),
                                ));
                            }
                        }
                    } else if let [Expr::Tuple(ExprTuple { elts, .. })] = &targets[..] {
                        for expr in elts {
                            if let Expr::Name(ExprName { id, .. }) = expr {
                                if let Some(range) = slots.get(id.as_str()) {
                                    return Some(Diagnostic::new(
                                        ClassVariableSlotsConflict {
                                            slot_conflict: id.to_owned(),
                                        },
                                        *range.to_owned(),
                                    ));
                                }
                            }
                        }
                    }
                }
                Stmt::FunctionDef(StmtFunctionDef { name, .. }) => {
                    if let Some(range) = slots.get(&name.as_str()) {
                        return Some(Diagnostic::new(
                            ClassVariableSlotsConflict {
                                slot_conflict: name.to_string(),
                            },
                            *range.to_owned(),
                        ));
                    }
                }
                _ => (),
            }
            None
        })
        .collect::<Vec<_>>()
}

/// PLE0242
pub(crate) fn class_variable_slots_conflict(
    checker: &mut Checker,
    StmtClassDef { body, .. }: &StmtClassDef,
) {
    if let Some(slots) = get_slots(body) {
        let diagnostics = traverse_class_body(body, &slots);
        checker.diagnostics.extend(diagnostics);
    }
}
