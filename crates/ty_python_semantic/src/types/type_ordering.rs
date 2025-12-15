use std::cmp::Ordering;

use salsa::plumbing::AsId;

use crate::{db::Db, types::bound_super::SuperOwnerKind};

use super::{
    DynamicType, TodoType, Type, TypeIsType, class_base::ClassBase, subclass_of::SubclassOfInner,
};

/// Return an [`Ordering`] that describes the canonical order in which two types should appear
/// in an [`crate::types::IntersectionType`] or a [`crate::types::UnionType`] in order for them
/// to be compared for equivalence.
///
/// Two intersections are compared lexicographically. Element types in the intersection must
/// already be sorted. Two unions are never compared in this function because DNF does not permit
/// nested unions.
///
/// ## Why not just implement [`Ord`] on [`Type`]?
///
/// It would be fairly easy to slap `#[derive(PartialOrd, Ord)]` on [`Type`], and the ordering we
/// create here is not user-facing. However, it doesn't really "make sense" for `Type` to implement
/// [`Ord`] in terms of the semantics. There are many different ways in which you could plausibly
/// sort a list of types; this is only one (somewhat arbitrary, at times) possible ordering.
pub(super) fn union_or_intersection_elements_ordering<'db>(
    db: &'db dyn Db,
    left: &Type<'db>,
    right: &Type<'db>,
) -> Ordering {
    debug_assert_eq!(
        *left,
        left.normalized(db),
        "`left` must be normalized before a meaningful ordering can be established"
    );
    debug_assert_eq!(
        *right,
        right.normalized(db),
        "`right` must be normalized before a meaningful ordering can be established"
    );

    if left == right {
        return Ordering::Equal;
    }

    match (left, right) {
        (Type::Never, _) => Ordering::Less,
        (_, Type::Never) => Ordering::Greater,

        (Type::LiteralString, _) => Ordering::Less,
        (_, Type::LiteralString) => Ordering::Greater,

        (Type::BooleanLiteral(left), Type::BooleanLiteral(right)) => left.cmp(right),
        (Type::BooleanLiteral(_), _) => Ordering::Less,
        (_, Type::BooleanLiteral(_)) => Ordering::Greater,

        (Type::IntLiteral(left), Type::IntLiteral(right)) => left.cmp(right),
        (Type::IntLiteral(_), _) => Ordering::Less,
        (_, Type::IntLiteral(_)) => Ordering::Greater,

        (Type::StringLiteral(left), Type::StringLiteral(right)) => left.cmp(right),
        (Type::StringLiteral(_), _) => Ordering::Less,
        (_, Type::StringLiteral(_)) => Ordering::Greater,

        (Type::BytesLiteral(left), Type::BytesLiteral(right)) => left.cmp(right),
        (Type::BytesLiteral(_), _) => Ordering::Less,
        (_, Type::BytesLiteral(_)) => Ordering::Greater,

        (Type::EnumLiteral(left), Type::EnumLiteral(right)) => left.cmp(right),
        (Type::EnumLiteral(_), _) => Ordering::Less,
        (_, Type::EnumLiteral(_)) => Ordering::Greater,

        (Type::FunctionLiteral(left), Type::FunctionLiteral(right)) => left.cmp(right),
        (Type::FunctionLiteral(_), _) => Ordering::Less,
        (_, Type::FunctionLiteral(_)) => Ordering::Greater,

        (Type::BoundMethod(left), Type::BoundMethod(right)) => left.cmp(right),
        (Type::BoundMethod(_), _) => Ordering::Less,
        (_, Type::BoundMethod(_)) => Ordering::Greater,

        (Type::KnownBoundMethod(left), Type::KnownBoundMethod(right)) => left.cmp(right),
        (Type::KnownBoundMethod(_), _) => Ordering::Less,
        (_, Type::KnownBoundMethod(_)) => Ordering::Greater,

        (Type::WrapperDescriptor(left), Type::WrapperDescriptor(right)) => left.cmp(right),
        (Type::WrapperDescriptor(_), _) => Ordering::Less,
        (_, Type::WrapperDescriptor(_)) => Ordering::Greater,

        (Type::DataclassDecorator(left), Type::DataclassDecorator(right)) => left.cmp(right),
        (Type::DataclassDecorator(_), _) => Ordering::Less,
        (_, Type::DataclassDecorator(_)) => Ordering::Greater,

        (Type::DataclassTransformer(left), Type::DataclassTransformer(right)) => left.cmp(right),
        (Type::DataclassTransformer(_), _) => Ordering::Less,
        (_, Type::DataclassTransformer(_)) => Ordering::Greater,

        (Type::Callable(left), Type::Callable(right)) => left.cmp(right),
        (Type::Callable(_), _) => Ordering::Less,
        (_, Type::Callable(_)) => Ordering::Greater,

        (Type::ModuleLiteral(left), Type::ModuleLiteral(right)) => left.cmp(right),
        (Type::ModuleLiteral(_), _) => Ordering::Less,
        (_, Type::ModuleLiteral(_)) => Ordering::Greater,

        (Type::ClassLiteral(left), Type::ClassLiteral(right)) => left.cmp(right),
        (Type::ClassLiteral(_), _) => Ordering::Less,
        (_, Type::ClassLiteral(_)) => Ordering::Greater,

        (Type::GenericAlias(left), Type::GenericAlias(right)) => left.cmp(right),
        (Type::GenericAlias(_), _) => Ordering::Less,
        (_, Type::GenericAlias(_)) => Ordering::Greater,

        (Type::SubclassOf(left), Type::SubclassOf(right)) => {
            match (left.subclass_of(), right.subclass_of()) {
                (SubclassOfInner::Class(left), SubclassOfInner::Class(right)) => left.cmp(&right),
                (SubclassOfInner::Class(_), _) => Ordering::Less,
                (_, SubclassOfInner::Class(_)) => Ordering::Greater,
                (SubclassOfInner::Dynamic(left), SubclassOfInner::Dynamic(right)) => {
                    dynamic_elements_ordering(left, right)
                }
                (SubclassOfInner::TypeVar(left), SubclassOfInner::TypeVar(right)) => {
                    left.as_id().cmp(&right.as_id())
                }
                (SubclassOfInner::TypeVar(_), _) => Ordering::Less,
                (_, SubclassOfInner::TypeVar(_)) => Ordering::Greater,
            }
        }

        (Type::SubclassOf(_), _) => Ordering::Less,
        (_, Type::SubclassOf(_)) => Ordering::Greater,

        (Type::TypeIs(left), Type::TypeIs(right)) => typeis_ordering(db, *left, *right),
        (Type::TypeIs(_), _) => Ordering::Less,
        (_, Type::TypeIs(_)) => Ordering::Greater,

        (Type::NominalInstance(left), Type::NominalInstance(right)) => {
            left.class(db).cmp(&right.class(db))
        }
        (Type::NominalInstance(_), _) => Ordering::Less,
        (_, Type::NominalInstance(_)) => Ordering::Greater,

        (Type::ProtocolInstance(left_proto), Type::ProtocolInstance(right_proto)) => {
            left_proto.cmp(right_proto)
        }
        (Type::ProtocolInstance(_), _) => Ordering::Less,
        (_, Type::ProtocolInstance(_)) => Ordering::Greater,

        // This is one place where we want to compare the typevar identities directly, instead of
        // falling back on `is_same_typevar_as` or `can_be_bound_for`.
        (Type::TypeVar(left), Type::TypeVar(right)) => left.as_id().cmp(&right.as_id()),
        (Type::TypeVar(_), _) => Ordering::Less,
        (_, Type::TypeVar(_)) => Ordering::Greater,

        (Type::AlwaysTruthy, _) => Ordering::Less,
        (_, Type::AlwaysTruthy) => Ordering::Greater,

        (Type::AlwaysFalsy, _) => Ordering::Less,
        (_, Type::AlwaysFalsy) => Ordering::Greater,

        (Type::BoundSuper(left), Type::BoundSuper(right)) => {
            (match (left.pivot_class(db), right.pivot_class(db)) {
                (ClassBase::Class(left), ClassBase::Class(right)) => left.cmp(&right),
                (ClassBase::Class(_), _) => Ordering::Less,
                (_, ClassBase::Class(_)) => Ordering::Greater,

                (ClassBase::Protocol, _) => Ordering::Less,
                (_, ClassBase::Protocol) => Ordering::Greater,

                (ClassBase::Generic, _) => Ordering::Less,
                (_, ClassBase::Generic) => Ordering::Greater,

                (ClassBase::TypedDict, _) => Ordering::Less,
                (_, ClassBase::TypedDict) => Ordering::Greater,

                (ClassBase::Dynamic(left), ClassBase::Dynamic(right)) => {
                    dynamic_elements_ordering(left, right)
                }
            })
            .then_with(|| match (left.owner(db), right.owner(db)) {
                (SuperOwnerKind::Class(left), SuperOwnerKind::Class(right)) => left.cmp(&right),
                (SuperOwnerKind::Class(_), _) => Ordering::Less,
                (_, SuperOwnerKind::Class(_)) => Ordering::Greater,
                (SuperOwnerKind::Instance(left), SuperOwnerKind::Instance(right)) => {
                    left.class(db).cmp(&right.class(db))
                }
                (SuperOwnerKind::Instance(_), _) => Ordering::Less,
                (_, SuperOwnerKind::Instance(_)) => Ordering::Greater,
                (SuperOwnerKind::Dynamic(left), SuperOwnerKind::Dynamic(right)) => {
                    dynamic_elements_ordering(left, right)
                }
            })
        }
        (Type::BoundSuper(_), _) => Ordering::Less,
        (_, Type::BoundSuper(_)) => Ordering::Greater,

        (Type::SpecialForm(left), Type::SpecialForm(right)) => left.cmp(right),
        (Type::SpecialForm(_), _) => Ordering::Less,
        (_, Type::SpecialForm(_)) => Ordering::Greater,

        (Type::KnownInstance(left), Type::KnownInstance(right)) => left.cmp(right),
        (Type::KnownInstance(_), _) => Ordering::Less,
        (_, Type::KnownInstance(_)) => Ordering::Greater,

        (Type::PropertyInstance(left), Type::PropertyInstance(right)) => left.cmp(right),
        (Type::PropertyInstance(_), _) => Ordering::Less,
        (_, Type::PropertyInstance(_)) => Ordering::Greater,

        (Type::Dynamic(left), Type::Dynamic(right)) => dynamic_elements_ordering(*left, *right),
        (Type::Dynamic(_), _) => Ordering::Less,
        (_, Type::Dynamic(_)) => Ordering::Greater,

        (Type::TypeAlias(left), Type::TypeAlias(right)) => left.cmp(right),
        (Type::TypeAlias(_), _) => Ordering::Less,
        (_, Type::TypeAlias(_)) => Ordering::Greater,

        (Type::TypedDict(left), Type::TypedDict(right)) => {
            left.defining_class().cmp(&right.defining_class())
        }
        (Type::TypedDict(_), _) => Ordering::Less,
        (_, Type::TypedDict(_)) => Ordering::Greater,

        (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => left.cmp(right),
        (Type::NewTypeInstance(_), _) => Ordering::Less,
        (_, Type::NewTypeInstance(_)) => Ordering::Greater,

        (Type::Union(_), _) | (_, Type::Union(_)) => {
            unreachable!("our type representation does not permit nested unions");
        }

        (Type::Intersection(left), Type::Intersection(right)) => {
            // Lexicographically compare the elements of the two unequal intersections.
            let left_positive = left.positive(db);
            let right_positive = right.positive(db);
            if left_positive.len() != right_positive.len() {
                return left_positive.len().cmp(&right_positive.len());
            }
            let left_negative = left.negative(db);
            let right_negative = right.negative(db);
            if left_negative.len() != right_negative.len() {
                return left_negative.len().cmp(&right_negative.len());
            }
            for (left, right) in left_positive.iter().zip(right_positive) {
                let ordering = union_or_intersection_elements_ordering(db, left, right);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            for (left, right) in left_negative.iter().zip(right_negative) {
                let ordering = union_or_intersection_elements_ordering(db, left, right);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }

            unreachable!("Two equal, normalized intersections should share the same Salsa ID")
        }
    }
}

/// Determine a canonical order for two instances of [`DynamicType`].
fn dynamic_elements_ordering(left: DynamicType, right: DynamicType) -> Ordering {
    match (left, right) {
        (DynamicType::Any, _) => Ordering::Less,
        (_, DynamicType::Any) => Ordering::Greater,

        (DynamicType::Unknown, _) => Ordering::Less,
        (_, DynamicType::Unknown) => Ordering::Greater,

        (DynamicType::UnknownGeneric(_), _) => Ordering::Less,
        (_, DynamicType::UnknownGeneric(_)) => Ordering::Greater,

        #[cfg(debug_assertions)]
        (DynamicType::Todo(TodoType(left)), DynamicType::Todo(TodoType(right))) => left.cmp(right),

        #[cfg(not(debug_assertions))]
        (DynamicType::Todo(TodoType), DynamicType::Todo(TodoType)) => Ordering::Equal,

        (DynamicType::TodoUnpack, _) => Ordering::Less,
        (_, DynamicType::TodoUnpack) => Ordering::Greater,

        (DynamicType::TodoStarredExpression, _) => Ordering::Less,
        (_, DynamicType::TodoStarredExpression) => Ordering::Greater,

        (DynamicType::Divergent(left), DynamicType::Divergent(right)) => left.cmp(&right),
        (DynamicType::Divergent(_), _) => Ordering::Less,
        (_, DynamicType::Divergent(_)) => Ordering::Greater,
    }
}

/// Determine a canonical order for two instances of [`TypeIsType`].
///
/// The following criteria are considered, in order:
/// * Boundness: Unbound precedes bound
/// * Symbol name: String comparison
/// * Guarded type: [`union_or_intersection_elements_ordering`]
fn typeis_ordering(db: &dyn Db, left: TypeIsType, right: TypeIsType) -> Ordering {
    let (left_ty, right_ty) = (left.return_type(db), right.return_type(db));

    match (left.place_info(db), right.place_info(db)) {
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,

        (None, None) => union_or_intersection_elements_ordering(db, &left_ty, &right_ty),

        (Some(_), Some(_)) => match left.place_name(db).cmp(&right.place_name(db)) {
            Ordering::Equal => union_or_intersection_elements_ordering(db, &left_ty, &right_ty),
            ordering => ordering,
        },
    }
}
