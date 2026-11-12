use std::hash::Hash;

use ruff_python_semantic::analyze::class::iter_super_class;
use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, Stmt};
use ruff_text_size::{Ranged, TextRange};

use crate::Violation;
use crate::checkers::ast::Checker;
use crate::codes::Category;

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
#[violation_metadata(preview_since = "0.9.3", category = Category::Correctness)]
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
    if class_def.bases().next().is_none() {
        return;
    }

    let ast::StmtClassDef { body, .. } = class_def;
    let class_slots = slots_members(body);

    // If there are no slots, we're safe
    if class_slots.is_empty() {
        return;
    }

    #[expect(
        clippy::iter_over_hash_type,
        reason = "each slot is checked independently against the base classes"
    )]
    for slot in class_slots {
        check_super_slots(checker, class_def, &slot);
    }
}

#[derive(Clone, Debug)]
struct Slot<'a> {
    name: &'a str,
    range: TextRange,
}

impl<'a> Slot<'a> {
    fn from_expr(expr: &'a Expr) -> Option<Self> {
        let string = expr.as_string_literal_expr()?;
        Some(Self {
            name: string.value.to_str(),
            range: string.range,
        })
    }
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
            checker.report_diagnostic(
                RedefinedSlotsInSubclass {
                    base: super_class.name.to_string(),
                    slot_name: slot.name.to_string(),
                },
                slot.range(),
            );
        }
    }
}

fn slots_members(body: &[Stmt]) -> FxHashSet<Slot<'_>> {
    let mut members = FxHashSet::default();
    for stmt in body {
        match stmt {
            // Ex) `__slots__ = ("name",)`
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                let [Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() else {
                    continue;
                };

                if id == "__slots__" {
                    collect_slots(value, &mut members);
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
                    collect_slots(value, &mut members);
                }
            }

            // Ex) `__slots__ += ("name",)`
            Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
                let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() else {
                    continue;
                };

                if id == "__slots__" {
                    collect_slots(value, &mut members);
                }
            }
            _ => {}
        }
    }
    members
}

fn collect_slots<'a>(expr: &'a Expr, members: &mut FxHashSet<Slot<'a>>) {
    match expr {
        Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::List(ast::ExprList { elts, .. })
        | Expr::Set(ast::ExprSet { elts, .. }) => {
            members.extend(elts.iter().filter_map(Slot::from_expr));
        }
        Expr::Dict(dict) => {
            members.extend(dict.iter_keys().flatten().filter_map(Slot::from_expr));
        }
        _ => {}
    }
}
