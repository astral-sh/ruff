use std::cmp::Ordering;

use ruff_db::files::{File, FilePath};

use crate::db::Db;
use crate::types::Type;

use super::{
    class_base::ClassBase, Class, ClassLiteralType, DynamicType, InstanceType, KnownClass,
    KnownInstanceType, TodoType,
};

/// Return an [`Ordering`] that describes the canonical order in which two types should appear
/// in an [`crate::types::IntersectionType`] or a [`crate::types::UnionType`].
///
/// Two unions with equal sets of elements always have elements ordered the same way in our
/// representation. This helps reduce memory usage, and also makes it easier to answer the
/// question of whether two unions are equal.
///
/// ## Why not just implement `Ord` on `Type`?
///
/// It would be fairly easy to slap `#[derive(PartialOrd, Ord)]` on `Type`. However, this would
/// order types according to their Salsa ID. While this would mean that types would always be
/// consistently ordered in any single run of red-knot, the order in which they would appear
/// might vary between different runs of red-knot. Unless we implemented an entirely different
/// order for display purposes, this would make it difficult to write mdtests, and would also
/// be quite confusing for users.
///
/// Moreover, it doesn't really "make sense" for `Type` to implement `Ord` in terms of the
/// semantics. There are many different ways in which you could plausibly sort a list of types;
/// this is only one (somewhat arbitrary, at times) possible ordering.
pub(super) fn sort_union_elements<'db>(
    db: &'db dyn Db,
    left: &Type<'db>,
    right: &Type<'db>,
) -> Ordering {
    if left == right {
        return Ordering::Equal;
    }

    match (left, right) {
        (Type::Never, Type::Never) => Ordering::Equal,
        (Type::Never, _) => Ordering::Less,
        (_, Type::Never) => Ordering::Greater,

        (Type::LiteralString, Type::LiteralString) => Ordering::Equal,
        (Type::LiteralString, _) => Ordering::Less,
        (_, Type::LiteralString) => Ordering::Greater,

        (Type::BooleanLiteral(left), Type::BooleanLiteral(right)) => left.cmp(right),
        (Type::BooleanLiteral(_), _) => Ordering::Less,
        (_, Type::BooleanLiteral(_)) => Ordering::Greater,

        (Type::IntLiteral(left), Type::IntLiteral(right)) => left.cmp(right),
        (Type::IntLiteral(_), _) => Ordering::Less,
        (_, Type::IntLiteral(_)) => Ordering::Greater,

        (Type::StringLiteral(left), Type::StringLiteral(right)) => {
            left.value(db).cmp(right.value(db))
        }

        (Type::StringLiteral(_), _) => Ordering::Less,
        (_, Type::StringLiteral(_)) => Ordering::Greater,

        (Type::BytesLiteral(left), Type::BytesLiteral(right)) => {
            left.value(db).cmp(right.value(db))
        }

        (Type::BytesLiteral(_), _) => Ordering::Less,
        (_, Type::BytesLiteral(_)) => Ordering::Greater,

        (Type::SliceLiteral(left), Type::SliceLiteral(right)) => {
            left.as_tuple(db).cmp(&right.as_tuple(db))
        }

        (Type::SliceLiteral(_), _) => Ordering::Less,
        (_, Type::SliceLiteral(_)) => Ordering::Greater,

        // First ensure functions in the same file are grouped together,
        // then sort by the function's name, then by the function's Salsa ID.
        (Type::FunctionLiteral(left_fn), Type::FunctionLiteral(right_fn)) => sort_files(
            db,
            left_fn.body_scope(db).file(db),
            right_fn.body_scope(db).file(db),
        )
        .then_with(|| left_fn.name(db).cmp(right_fn.name(db)))
        .then_with(|| left_fn.cmp(right_fn)),

        (Type::FunctionLiteral(_), _) => Ordering::Less,
        (_, Type::FunctionLiteral(_)) => Ordering::Greater,

        (Type::Tuple(left), Type::Tuple(right)) => {
            let left_elements = left.elements(db);
            let right_elements = right.elements(db);
            left_elements
                .len()
                .cmp(&right_elements.len())
                .then_with(|| sort_sequences(db, left_elements, right_elements))
        }
        (Type::Tuple(_), _) => Ordering::Less,
        (_, Type::Tuple(_)) => Ordering::Greater,

        (Type::ModuleLiteral(left_mod), Type::ModuleLiteral(right_mod)) => {
            sort_files(db, left_mod.module(db).file(), right_mod.module(db).file())
        }

        (Type::ModuleLiteral(_), _) => Ordering::Less,
        (_, Type::ModuleLiteral(_)) => Ordering::Greater,

        (
            Type::ClassLiteral(ClassLiteralType { class: left }),
            Type::ClassLiteral(ClassLiteralType { class: right }),
        ) => sort_class_elements(db, *left, *right),

        (Type::ClassLiteral(_), _) => Ordering::Less,
        (_, Type::ClassLiteral(_)) => Ordering::Greater,

        (Type::SubclassOf(left), Type::SubclassOf(right)) => {
            match (left.subclass_of(), right.subclass_of()) {
                (ClassBase::Class(left), ClassBase::Class(right)) => {
                    sort_class_elements(db, left, right)
                }
                (ClassBase::Class(_), _) => Ordering::Less,
                (_, ClassBase::Class(_)) => Ordering::Greater,

                (ClassBase::Dynamic(left), ClassBase::Dynamic(right)) => {
                    sort_dynamic_elements(left, right)
                }
            }
        }

        (Type::SubclassOf(_), _) => Ordering::Less,
        (_, Type::SubclassOf(_)) => Ordering::Greater,

        (
            Type::Instance(InstanceType { class: left }),
            Type::Instance(InstanceType { class: right }),
        ) => sort_class_elements(db, *left, *right),

        (Type::Instance(_), _) => Ordering::Less,
        (_, Type::Instance(_)) => Ordering::Greater,

        // Nice to have this after most other types, since it's a type users will be less familiar with.
        (Type::AlwaysTruthy, Type::AlwaysTruthy) => Ordering::Equal,
        (Type::AlwaysTruthy, _) => Ordering::Less,
        (_, Type::AlwaysTruthy) => Ordering::Greater,

        // Nice to have this after most other types, since it's a type users will be less familiar with.
        (Type::AlwaysFalsy, Type::AlwaysFalsy) => Ordering::Equal,
        (Type::AlwaysFalsy, _) => Ordering::Less,
        (_, Type::AlwaysFalsy) => Ordering::Greater,

        (Type::KnownInstance(left_instance), Type::KnownInstance(right_instance)) => left_instance
            .repr(db)
            .cmp(right_instance.repr(db))
            .then_with(|| match (left_instance, right_instance) {
                (
                    KnownInstanceType::TypeAliasType(left),
                    KnownInstanceType::TypeAliasType(right),
                ) => left
                    .name(db)
                    .cmp(right.name(db))
                    .then_with(|| left.cmp(right)),
                _ => Ordering::Equal,
            }),

        (Type::KnownInstance(_), _) => Ordering::Less,
        (_, Type::KnownInstance(_)) => Ordering::Greater,

        (Type::Dynamic(left), Type::Dynamic(right)) => sort_dynamic_elements(*left, *right),
        (Type::Dynamic(_), _) => Ordering::Less,
        (_, Type::Dynamic(_)) => Ordering::Greater,

        (Type::Union(left), Type::Union(right)) => {
            let left_elements = left.elements(db);
            let right_elements = right.elements(db);
            left_elements
                .len()
                .cmp(&right_elements.len())
                .then_with(|| sort_sequences(db, left_elements, right_elements))
        }
        (Type::Union(_), _) => Ordering::Less,
        (_, Type::Union(_)) => Ordering::Greater,

        (Type::Intersection(left), Type::Intersection(right)) => {
            let left_pos = left.positive(db);
            let right_pos = right.positive(db);
            let left_neg = left.negative(db);
            let right_neg = right.negative(db);

            left_pos
                .len()
                .cmp(&right_pos.len())
                .then_with(|| left_neg.len().cmp(&right_neg.len()))
                .then_with(|| {
                    sort_sequences(
                        db,
                        left_pos.iter().chain(left_neg),
                        right_pos.iter().chain(right_neg),
                    )
                })
        }
    }
}

/// Determine a canonical order for two [`File`]s.
///
/// This is useful for ordering modules, classes and functions:
/// for all three, it makes sense to group types from the same module together
/// in intersections and unions.
fn sort_files(db: &dyn Db, left_file: File, right_file: File) -> Ordering {
    if left_file == right_file {
        return Ordering::Equal;
    }

    let left_path = left_file.path(db.upcast());
    let right_path = right_file.path(db.upcast());

    match (left_path, right_path) {
        (FilePath::System(left_path), FilePath::System(right_path)) => left_path.cmp(right_path),
        (FilePath::System(_), _) => Ordering::Less,
        (_, FilePath::System(_)) => Ordering::Greater,

        (FilePath::Vendored(left_path), FilePath::Vendored(right_path)) => {
            left_path.cmp(right_path)
        }
        (FilePath::Vendored(_), _) => Ordering::Less,
        (_, FilePath::Vendored(_)) => Ordering::Greater,

        (FilePath::SystemVirtual(left_path), FilePath::SystemVirtual(right_path)) => {
            left_path.cmp(right_path)
        }
    }
    .then_with(|| left_file.cmp(&right_file))
}

/// Determine a canonical order for two [`Class`]es.
fn sort_class_elements<'db>(db: &'db dyn Db, left: Class<'db>, right: Class<'db>) -> Ordering {
    if left == right {
        return Ordering::Equal;
    }

    // aesthetically, it's nice if `None` is always last
    if left.is_known(db, KnownClass::NoneType) {
        return Ordering::Greater;
    }
    if right.is_known(db, KnownClass::NoneType) {
        return Ordering::Less;
    }

    // General case: first, group the classes according to which file they're in
    sort_files(db, left.file(db), right.file(db))
        // then sort by the class's name
        .then_with(|| left.name(db).cmp(right.name(db)))
        // lastly, sort by the Salsa ID directly
        .then_with(|| left.cmp(&right))
}

/// Determine a canonical order for two instances of [`DynamicType`].
fn sort_dynamic_elements(left: DynamicType, right: DynamicType) -> Ordering {
    if left == right {
        return Ordering::Equal;
    }

    match (left, right) {
        (DynamicType::Any, DynamicType::Any) => Ordering::Equal,
        (DynamicType::Any, _) => Ordering::Less,
        (_, DynamicType::Any) => Ordering::Greater,

        (DynamicType::Unknown, DynamicType::Unknown) => Ordering::Equal,
        (DynamicType::Unknown, _) => Ordering::Less,
        (_, DynamicType::Unknown) => Ordering::Greater,

        #[cfg(debug_assertions)]
        (DynamicType::Todo(left), DynamicType::Todo(right)) => match (left, right) {
            (
                TodoType::FileAndLine(left_file, left_line),
                TodoType::FileAndLine(right_file, right_line),
            ) => left_file
                .cmp(right_file)
                .then_with(|| left_line.cmp(&right_line)),
            (TodoType::FileAndLine(..), _) => Ordering::Less,
            (_, TodoType::FileAndLine(..)) => Ordering::Greater,

            (TodoType::Message(left), TodoType::Message(right)) => left.cmp(right),
        },

        #[cfg(not(debug_assertions))]
        (DynamicType::Todo(TodoType), DynamicType::Todo(TodoType)) => Ordering::Equal,
    }
}

/// Determine a canonical order for two types that wrap sequences of other types.
///
/// This is useful for ordering tuples, unions and intersections.
fn sort_sequences<'db>(
    db: &'db dyn Db,
    left: impl IntoIterator<Item = &'db Type<'db>>,
    right: impl IntoIterator<Item = &'db Type<'db>>,
) -> Ordering {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| sort_union_elements(db, left, right))
        .find(|ordering| !ordering.is_eq())
        .unwrap_or(Ordering::Equal)
}
