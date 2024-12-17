use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};
use std::collections::HashMap;

use crate::checkers::ast::Checker;

/// ## What it does
/// Checks for a re-defined slot in a subclass.
///
/// ## Why is this bad?
/// If a class defines a slot also defined in a base class, the
/// instance variable defined by the base class slot is inaccessible
/// (except by retrieving its descriptor directly from the base class).
///
/// ## Example
/// ```python
/// class Base:
///     __slots__ = ("a", "b")
///
///
/// class Subclass(Base):
///     __slots__ = ("a", "d")  # [redefined-slots-in-subclass]
/// ```
///
/// Use instead:
/// ```python
/// class Base:
///     __slots__ = ("a", "b")
///
///
/// class Subclass(Base):
///     __slots__ = "d"
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct RedefinedSlotsInSubclass {
    name: String,
}

impl Violation for RedefinedSlotsInSubclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedSlotsInSubclass { name } = self;
        format!("Redefined slots ['{name}'] in subclass")
    }
}

// PLW0244
pub(crate) fn redefined_slots_in_subclass(checker: &mut Checker, body: &[Stmt]) {
    for slot in redefined_slots(body) {
        checker.diagnostics.push(Diagnostic::new(
            RedefinedSlotsInSubclass {
                name: slot.name.to_string(),
            },
            slot.range(),
        ));
    }
}

#[derive(Clone, Hash, Debug, Eq, PartialEq)]
struct Slot<'a> {
    name: &'a str,
    range: TextRange,
}

impl Ranged for Slot<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

fn redefined_slots(body: &[Stmt]) -> Vec<Slot> {
    // First, collect all the slot attributes that are assigned to classes.
    let mut class_slots = HashMap::new();
    let mut class_bases = HashMap::new();
    for stmt in body {
        let Stmt::ClassDef(ast::StmtClassDef {
            name,
            body,
            arguments,
            ..
        }) = stmt
        else {
            continue;
        };
        class_slots.insert(name.as_str(), slots_members(body));

        if let Some(arguments) = arguments {
            let mut bases = FxHashSet::default();
            for base in arguments.args.iter() {
                let Expr::Name(ast::ExprName { id, .. }) = base else {
                    continue;
                };
                bases.insert(id.as_str());
            }
            class_bases.insert(name.as_str(), bases);
        };
    }

    if class_slots.is_empty() || class_bases.is_empty() {
        return vec![];
    }

    // Second, find redefined slots.
    let mut slots_redefined = vec![];
    for (klass, bases) in &class_bases {
        if let Some(slots) = class_slots.get(klass) {
            for base in bases {
                if let Some(base_slots) = class_slots.get(base) {
                    for slot in slots {
                        for base_slot in base_slots {
                            if slot.name == base_slot.name {
                                slots_redefined.push(slot.to_owned());
                            }
                        }
                    }
                }
            }
        }
    }
    slots_redefined
}

fn slots_members(body: &[Stmt]) -> FxHashSet<Slot> {
    let mut members = FxHashSet::default();
    for stmt in body {
        match stmt {
            // Ex) `__slots__ = ("name",)`
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                let [Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() else {
                    continue;
                };

                if id == "__slots__" {
                    members.extend(slots_attributes(value));
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
                    members.extend(slots_attributes(value));
                }
            }

            // Ex) `__slots__ += ("name",)`
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                if id == "__slots__" {
                    members.extend(slots_attributes(value));
                }
            }
            _ => {}
        }
    }
    members
}

fn slots_attributes(expr: &Expr) -> impl Iterator<Item = Slot> {
    // Ex) `__slots__ = ("name",)`
    let elts_iter = match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::List(ast::ExprList { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => Some(elts.iter().filter_map(|elt| match elt {
            Expr::StringLiteral(ast::ExprStringLiteral { value, range }) => Some(Slot {
                name: value.to_str(),
                range: *range,
            }),
            _ => None,
        })),
        _ => None,
    };

    // Ex) `__slots__ = {"name": ...}`
    let keys_iter = match expr {
        Expr::Dict(ast::ExprDict { .. }) => Some(
            expr.as_dict_expr()
                .unwrap()
                .iter_keys()
                .filter_map(|key| match key {
                    Some(Expr::StringLiteral(ast::ExprStringLiteral { value, range })) => {
                        Some(Slot {
                            name: value.to_str(),
                            range: *range,
                        })
                    }
                    _ => None,
                }),
        ),
        _ => None,
    };

    elts_iter
        .into_iter()
        .flatten()
        .chain(keys_iter.into_iter().flatten())
}
