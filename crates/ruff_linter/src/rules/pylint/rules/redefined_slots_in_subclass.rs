use std::hash::Hash;

use ruff_python_semantic::analyze::class::iter_super_class;
use rustc_hash::FxHashSet;

use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::{self as ast, Expr, ExprKind, StmtClassDef, StmtKind, Suite};
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
pub(crate) fn redefined_slots_in_subclass(checker: &Checker, class_def: StmtClassDef<'_>) {
    // Early return if this is not a subclass
    if class_def.bases().is_empty() {
        return;
    }

    let body = class_def.body();
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

fn check_super_slots(checker: &Checker, class_def: StmtClassDef<'_>, slot: &Slot) {
    for super_class in iter_super_class(class_def, checker.semantic()).skip(1) {
        if slots_members(super_class.body()).contains(slot) {
            checker.report_diagnostic(
                RedefinedSlotsInSubclass {
                    base: super_class.name().to_string(),
                    slot_name: slot.name.to_string(),
                },
                slot.range(),
            );
        }
    }
}

fn slots_members(body: Suite<'_>) -> FxHashSet<Slot<'_>> {
    let mut members = FxHashSet::default();
    for stmt in body {
        match stmt.classify() {
            // Ex) `__slots__ = ("name",)`
            StmtKind::Assign(stmt_assign) => {
                let targets = stmt_assign.targets();
                let value = stmt_assign.value();

                let Some([target]) = targets.as_array() else {
                    continue;
                };

                if target
                    .as_name_expr()
                    .is_some_and(|name| name.id() == "__slots__")
                {
                    members.extend(slots_attributes(value));
                }
            }

            // Ex) `__slots__: Tuple[str, ...] = ("name",)`
            StmtKind::AnnAssign(stmt_ann_assign) if let Some(value) = stmt_ann_assign.value() => {
                let target = stmt_ann_assign.target();

                let ExprKind::Name(name_node) = target.classify() else {
                    continue;
                };
                let id = name_node.id();

                if id == "__slots__" {
                    members.extend(slots_attributes(value));
                }
            }

            // Ex) `__slots__ += ("name",)`
            StmtKind::AugAssign(stmt_aug_assign) => {
                let target = stmt_aug_assign.target();
                let value = stmt_aug_assign.value();

                let ExprKind::Name(name_node) = target.classify() else {
                    continue;
                };
                let id = name_node.id();

                if id == "__slots__" {
                    members.extend(slots_attributes(value));
                }
            }
            _ => {}
        }
    }
    members
}

fn slots_attributes(expr: Expr<'_>) -> impl Iterator<Item = Slot<'_>> {
    // Ex) `__slots__ = ("name",)`
    let elts_iter = match expr.fields() {
        ast::ExprFields::Tuple(ast::ExprTupleFields { elts, .. })
        | ast::ExprFields::List(ast::ExprListFields { elts, .. })
        | ast::ExprFields::Set(ast::ExprSetFields { elts, .. }) => {
            Some(elts.iter().filter_map(|elt| match elt.classify() {
                ExprKind::StringLiteral(expr_string_literal) => {
                    let value = expr_string_literal.value();
                    let range = expr_string_literal.range();
                    Some(Slot {
                        name: value.to_str(),
                        range,
                    })
                }
                _ => None,
            }))
        }
        _ => None,
    };

    // Ex) `__slots__ = {"name": ...}`
    let keys_iter = match expr.classify() {
        ExprKind::Dict(_) => Some(expr.as_dict_expr().unwrap().iter_keys().filter_map(|key| {
            match key.map(Expr::fields) {
                Some(ast::ExprFields::StringLiteral(ast::ExprStringLiteralFields {
                    value,
                    range,
                    node: _,
                })) => Some(Slot {
                    name: value.to_str(),
                    range,
                }),
                _ => None,
            }
        })),
        _ => None,
    };

    elts_iter
        .into_iter()
        .flatten()
        .chain(keys_iter.into_iter().flatten())
}
