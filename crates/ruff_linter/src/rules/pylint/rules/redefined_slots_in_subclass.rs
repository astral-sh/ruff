use std::hash::Hash;

use ruff_python_semantic::analyze::class::iter_super_class;
use rustc_hash::FxHashSet;

use ruff_diagnostics::{Diagnostic, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

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
///     __slots__ = ("a", "d")  # slot "a" redefined
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
    base: String,
    slot_name: String,
}

impl Violation for RedefinedSlotsInSubclass {
    #[derive_message_formats]
    fn message(&self) -> String {
        let RedefinedSlotsInSubclass { base, slot_name } = self;
        format!("Slot `{slot_name}` redefined from base class `{base}`")
    }
}

// PLW0244
pub(crate) fn redefined_slots_in_subclass(checker: &Checker, class_def: &ast::StmtClassDef) {
    // Early return if this is not a subclass
    if class_def.bases().is_empty() {
        return;
    }

    let ast::StmtClassDef { body, .. } = class_def;
    let class_slots = slots_members(body);

    // If there are no slots, we're safe
    if class_slots.is_empty() {
        return;
    }

    for slot in class_slots {
        check_super_slots(checker, class_def, &slot);
    }
}

#[derive(Clone, Debug)]
struct Slot<'a> {
    name: &'a str,
    range: TextRange,
}

impl std::cmp::PartialEq for Slot<'_> {
    // We will only ever be comparing slots
    // within a class and with the slots of
    // a super class. In that context, we
    // want to compare names and not ranges.
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl std::cmp::Eq for Slot<'_> {}

impl Hash for Slot<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl Ranged for Slot<'_> {
    fn range(&self) -> TextRange {
        self.range
    }
}

fn check_super_slots(checker: &Checker, class_def: &ast::StmtClassDef, slot: &Slot) {
    for super_class in iter_super_class(class_def, checker.semantic()).skip(1) {
        if slots_members(&super_class.body).contains(slot) {
            checker.report_diagnostic(Diagnostic::new(
                RedefinedSlotsInSubclass {
                    base: super_class.name.to_string(),
                    slot_name: slot.name.to_string(),
                },
                slot.range(),
            ));
        }
    }
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
