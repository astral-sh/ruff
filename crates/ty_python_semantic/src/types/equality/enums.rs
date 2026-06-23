//! Equality reasoning for values from the same enum class.

use crate::Db;
use crate::types::{EnumClassLiteral, IntersectionBuilder, LiteralValueTypeKind, Type};

/// Return the enum class when `ty` represents an open enum domain.
fn open_enum_class<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<EnumClassLiteral<'db>> {
    match ty.resolve_type_alias(db) {
        Type::NominalInstance(instance) => {
            let enum_class = instance.class_literal(db).into_enum_class(db)?;
            (!enum_class.members_are_exhaustive(db)).then_some(enum_class)
        }
        Type::Intersection(intersection) => {
            let mut enum_classes = intersection
                .positive(db)
                .iter()
                .filter_map(|positive| open_enum_class(db, *positive));
            let enum_class = enum_classes.next()?;
            enum_classes
                .all(|other| other == enum_class)
                .then_some(enum_class)
        }
        _ => None,
    }
}

/// Return the enum class when `ty` is an exact set of literals from one enum.
fn exact_enum_member_class<'db>(db: &'db dyn Db, ty: Type<'db>) -> Option<EnumClassLiteral<'db>> {
    match ty.resolve_type_alias(db) {
        Type::LiteralValue(literal) => {
            let LiteralValueTypeKind::Enum(literal) = literal.kind() else {
                return None;
            };
            Some(literal.enum_class_literal(db))
        }
        Type::Union(union) => {
            let mut enum_classes = union
                .elements(db)
                .iter()
                .map(|element| exact_enum_member_class(db, *element));
            let enum_class = enum_classes.next()??;
            enum_classes
                .all(|other| other == Some(enum_class))
                .then_some(enum_class)
        }
        _ => None,
    }
}

/// Return the constraint established by membership in an exact set of open-enum members.
pub(in crate::types) fn enum_membership_constraint<'db>(
    db: &'db dyn Db,
    target: Type<'db>,
    members: Type<'db>,
    is_positive: bool,
) -> Option<Type<'db>> {
    let enum_class = open_enum_class(db, target)?;
    if exact_enum_member_class(db, members)? != enum_class {
        return None;
    }

    let enum_instance = enum_class.class_literal(db).to_non_generic_instance(db);
    if enum_instance.overrides_equality(db) {
        return None;
    }

    if is_positive {
        Some(members)
    } else {
        Some(
            IntersectionBuilder::new(db)
                .add_positive(enum_instance)
                .add_negative(members)
                .build(),
        )
    }
}
