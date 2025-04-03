use ruff_python_ast as ast;

use crate::db::Db;
use crate::symbol::{Boundness, Symbol};
use crate::types::class_base::ClassBase;
use crate::types::diagnostic::report_base_with_incompatible_slots;
use crate::types::{Class, ClassLiteralType, Type};

use super::InferContext;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum SlotsKind {
    /// `__slots__` is not found in the class.
    NotSpecified,
    /// `__slots__` is defined but empty: `__slots__ = ()`.
    Empty,
    /// `__slots__` is defined and is not empty: `__slots__ = ("a", "b")`.
    NotEmpty,
    /// `__slots__` is defined but its value is dynamic:
    /// * `__slots__ = tuple(a for a in b)`
    /// * `__slots__ = ["a", "b"]`
    Dynamic,
}

impl SlotsKind {
    fn from(db: &dyn Db, base: Class) -> Self {
        let Symbol::Type(slots_ty, bound) = base.own_class_member(db, "__slots__").symbol else {
            return Self::NotSpecified;
        };

        if matches!(bound, Boundness::PossiblyUnbound) {
            return Self::Dynamic;
        }

        match slots_ty {
            // __slots__ = ("a", "b")
            Type::Tuple(tuple) => {
                if tuple.elements(db).is_empty() {
                    Self::Empty
                } else {
                    Self::NotEmpty
                }
            }

            // __slots__ = "abc"  # Same as `("abc",)`
            Type::StringLiteral(_) => Self::NotEmpty,

            _ => Self::Dynamic,
        }
    }
}

pub(super) fn check_class_slots(context: &InferContext, class: Class, node: &ast::StmtClassDef) {
    let db = context.db();

    let mut first_with_solid_base = None;
    let mut common_solid_base = None;
    let mut found_second = false;

    for (index, base) in class.explicit_bases(db).iter().enumerate() {
        let Type::ClassLiteral(ClassLiteralType { class: base }) = base else {
            continue;
        };

        let solid_base = base.iter_mro(db).find_map(|current| {
            let ClassBase::Class(current) = current else {
                return None;
            };

            match SlotsKind::from(db, current) {
                SlotsKind::NotEmpty => Some(current),
                SlotsKind::NotSpecified | SlotsKind::Empty => None,
                SlotsKind::Dynamic => None,
            }
        });

        if solid_base.is_none() {
            continue;
        }

        let base_node = &node.bases()[index];

        if first_with_solid_base.is_none() {
            first_with_solid_base = Some(index);
            common_solid_base = solid_base;
            continue;
        }

        if solid_base == common_solid_base {
            continue;
        }

        found_second = true;
        report_base_with_incompatible_slots(context, base_node);
    }

    if found_second {
        if let Some(index) = first_with_solid_base {
            let base_node = &node.bases()[index];
            report_base_with_incompatible_slots(context, base_node);
        }
    }
}
