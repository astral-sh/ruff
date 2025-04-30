use std::cmp::Ordering;

use crate::db::Db;

use super::{
    class_base::ClassBase, subclass_of::SubclassOfInner, DynamicType, KnownInstanceType,
    SuperOwnerKind, TodoType, Type,
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

        (Type::SliceLiteral(left), Type::SliceLiteral(right)) => left.cmp(right),
        (Type::SliceLiteral(_), _) => Ordering::Less,
        (_, Type::SliceLiteral(_)) => Ordering::Greater,

        (Type::FunctionLiteral(left), Type::FunctionLiteral(right)) => left.cmp(right),
        (Type::FunctionLiteral(_), _) => Ordering::Less,
        (_, Type::FunctionLiteral(_)) => Ordering::Greater,

        (Type::BoundMethod(left), Type::BoundMethod(right)) => left.cmp(right),
        (Type::BoundMethod(_), _) => Ordering::Less,
        (_, Type::BoundMethod(_)) => Ordering::Greater,

        (Type::MethodWrapper(left), Type::MethodWrapper(right)) => left.cmp(right),
        (Type::MethodWrapper(_), _) => Ordering::Less,
        (_, Type::MethodWrapper(_)) => Ordering::Greater,

        (Type::WrapperDescriptor(left), Type::WrapperDescriptor(right)) => left.cmp(right),
        (Type::WrapperDescriptor(_), _) => Ordering::Less,
        (_, Type::WrapperDescriptor(_)) => Ordering::Greater,

        (Type::DataclassDecorator(left), Type::DataclassDecorator(right)) => {
            left.bits().cmp(&right.bits())
        }
        (Type::DataclassDecorator(_), _) => Ordering::Less,
        (_, Type::DataclassDecorator(_)) => Ordering::Greater,

        (Type::DataclassTransformer(left), Type::DataclassTransformer(right)) => {
            left.bits().cmp(&right.bits())
        }
        (Type::DataclassTransformer(_), _) => Ordering::Less,
        (_, Type::DataclassTransformer(_)) => Ordering::Greater,

        (Type::Callable(left), Type::Callable(right)) => left.cmp(right),
        (Type::Callable(_), _) => Ordering::Less,
        (_, Type::Callable(_)) => Ordering::Greater,

        (Type::Tuple(left), Type::Tuple(right)) => left.cmp(right),
        (Type::Tuple(_), _) => Ordering::Less,
        (_, Type::Tuple(_)) => Ordering::Greater,

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
            }
        }

        (Type::SubclassOf(_), _) => Ordering::Less,
        (_, Type::SubclassOf(_)) => Ordering::Greater,

        (Type::NominalInstance(left), Type::NominalInstance(right)) => {
            left.class().cmp(&right.class())
        }
        (Type::NominalInstance(_), _) => Ordering::Less,
        (_, Type::NominalInstance(_)) => Ordering::Greater,

        (Type::ProtocolInstance(left_proto), Type::ProtocolInstance(right_proto)) => {
            left_proto.cmp(right_proto)
        }
        (Type::ProtocolInstance(_), _) => Ordering::Less,
        (_, Type::ProtocolInstance(_)) => Ordering::Greater,

        (Type::TypeVar(left), Type::TypeVar(right)) => left.cmp(right),
        (Type::TypeVar(_), _) => Ordering::Less,
        (_, Type::TypeVar(_)) => Ordering::Greater,

        (Type::AlwaysTruthy, _) => Ordering::Less,
        (_, Type::AlwaysTruthy) => Ordering::Greater,

        (Type::AlwaysFalsy, _) => Ordering::Less,
        (_, Type::AlwaysFalsy) => Ordering::Greater,

        (Type::BoundSuper(left), Type::BoundSuper(right)) => {
            (match (left.pivot_class(db), right.pivot_class(db)) {
                (ClassBase::Class(left), ClassBase::Class(right)) => left.cmp(right),
                (ClassBase::Class(_), _) => Ordering::Less,
                (_, ClassBase::Class(_)) => Ordering::Greater,
                (ClassBase::Protocol, _) => Ordering::Less,
                (_, ClassBase::Protocol) => Ordering::Greater,
                (ClassBase::Generic, _) => Ordering::Less,
                (_, ClassBase::Generic) => Ordering::Greater,
                (ClassBase::Dynamic(left), ClassBase::Dynamic(right)) => {
                    dynamic_elements_ordering(*left, *right)
                }
            })
            .then_with(|| match (left.owner(db), right.owner(db)) {
                (SuperOwnerKind::Class(left), SuperOwnerKind::Class(right)) => left.cmp(right),
                (SuperOwnerKind::Class(_), _) => Ordering::Less,
                (_, SuperOwnerKind::Class(_)) => Ordering::Greater,
                (SuperOwnerKind::Instance(left), SuperOwnerKind::Instance(right)) => {
                    left.class().cmp(&right.class())
                }
                (SuperOwnerKind::Instance(_), _) => Ordering::Less,
                (_, SuperOwnerKind::Instance(_)) => Ordering::Greater,
                (SuperOwnerKind::Dynamic(left), SuperOwnerKind::Dynamic(right)) => {
                    dynamic_elements_ordering(*left, *right)
                }
            })
        }
        (Type::BoundSuper(_), _) => Ordering::Less,
        (_, Type::BoundSuper(_)) => Ordering::Greater,

        (Type::KnownInstance(left_instance), Type::KnownInstance(right_instance)) => {
            match (left_instance, right_instance) {
                (KnownInstanceType::Any, _) => Ordering::Less,
                (_, KnownInstanceType::Any) => Ordering::Greater,

                (KnownInstanceType::Tuple, _) => Ordering::Less,
                (_, KnownInstanceType::Tuple) => Ordering::Greater,

                (KnownInstanceType::AlwaysFalsy, _) => Ordering::Less,
                (_, KnownInstanceType::AlwaysFalsy) => Ordering::Greater,

                (KnownInstanceType::AlwaysTruthy, _) => Ordering::Less,
                (_, KnownInstanceType::AlwaysTruthy) => Ordering::Greater,

                (KnownInstanceType::Annotated, _) => Ordering::Less,
                (_, KnownInstanceType::Annotated) => Ordering::Greater,

                (KnownInstanceType::Callable, _) => Ordering::Less,
                (_, KnownInstanceType::Callable) => Ordering::Greater,

                (KnownInstanceType::ChainMap, _) => Ordering::Less,
                (_, KnownInstanceType::ChainMap) => Ordering::Greater,

                (KnownInstanceType::ClassVar, _) => Ordering::Less,
                (_, KnownInstanceType::ClassVar) => Ordering::Greater,

                (KnownInstanceType::Concatenate, _) => Ordering::Less,
                (_, KnownInstanceType::Concatenate) => Ordering::Greater,

                (KnownInstanceType::Counter, _) => Ordering::Less,
                (_, KnownInstanceType::Counter) => Ordering::Greater,

                (KnownInstanceType::DefaultDict, _) => Ordering::Less,
                (_, KnownInstanceType::DefaultDict) => Ordering::Greater,

                (KnownInstanceType::Deque, _) => Ordering::Less,
                (_, KnownInstanceType::Deque) => Ordering::Greater,

                (KnownInstanceType::Dict, _) => Ordering::Less,
                (_, KnownInstanceType::Dict) => Ordering::Greater,

                (KnownInstanceType::Final, _) => Ordering::Less,
                (_, KnownInstanceType::Final) => Ordering::Greater,

                (KnownInstanceType::FrozenSet, _) => Ordering::Less,
                (_, KnownInstanceType::FrozenSet) => Ordering::Greater,

                (KnownInstanceType::TypeGuard, _) => Ordering::Less,
                (_, KnownInstanceType::TypeGuard) => Ordering::Greater,

                (KnownInstanceType::TypedDict, _) => Ordering::Less,
                (_, KnownInstanceType::TypedDict) => Ordering::Greater,

                (KnownInstanceType::List, _) => Ordering::Less,
                (_, KnownInstanceType::List) => Ordering::Greater,

                (KnownInstanceType::Literal, _) => Ordering::Less,
                (_, KnownInstanceType::Literal) => Ordering::Greater,

                (KnownInstanceType::LiteralString, _) => Ordering::Less,
                (_, KnownInstanceType::LiteralString) => Ordering::Greater,

                (KnownInstanceType::Optional, _) => Ordering::Less,
                (_, KnownInstanceType::Optional) => Ordering::Greater,

                (KnownInstanceType::OrderedDict, _) => Ordering::Less,
                (_, KnownInstanceType::OrderedDict) => Ordering::Greater,

                (KnownInstanceType::Generic, _) => Ordering::Less,
                (_, KnownInstanceType::Generic) => Ordering::Greater,

                (KnownInstanceType::Protocol, _) => Ordering::Less,
                (_, KnownInstanceType::Protocol) => Ordering::Greater,

                (KnownInstanceType::NoReturn, _) => Ordering::Less,
                (_, KnownInstanceType::NoReturn) => Ordering::Greater,

                (KnownInstanceType::Never, _) => Ordering::Less,
                (_, KnownInstanceType::Never) => Ordering::Greater,

                (KnownInstanceType::Set, _) => Ordering::Less,
                (_, KnownInstanceType::Set) => Ordering::Greater,

                (KnownInstanceType::Type, _) => Ordering::Less,
                (_, KnownInstanceType::Type) => Ordering::Greater,

                (KnownInstanceType::TypeAlias, _) => Ordering::Less,
                (_, KnownInstanceType::TypeAlias) => Ordering::Greater,

                (KnownInstanceType::Unknown, _) => Ordering::Less,
                (_, KnownInstanceType::Unknown) => Ordering::Greater,

                (KnownInstanceType::Not, _) => Ordering::Less,
                (_, KnownInstanceType::Not) => Ordering::Greater,

                (KnownInstanceType::Intersection, _) => Ordering::Less,
                (_, KnownInstanceType::Intersection) => Ordering::Greater,

                (KnownInstanceType::TypeOf, _) => Ordering::Less,
                (_, KnownInstanceType::TypeOf) => Ordering::Greater,

                (KnownInstanceType::CallableTypeOf, _) => Ordering::Less,
                (_, KnownInstanceType::CallableTypeOf) => Ordering::Greater,

                (KnownInstanceType::Unpack, _) => Ordering::Less,
                (_, KnownInstanceType::Unpack) => Ordering::Greater,

                (KnownInstanceType::TypingSelf, _) => Ordering::Less,
                (_, KnownInstanceType::TypingSelf) => Ordering::Greater,

                (KnownInstanceType::Required, _) => Ordering::Less,
                (_, KnownInstanceType::Required) => Ordering::Greater,

                (KnownInstanceType::NotRequired, _) => Ordering::Less,
                (_, KnownInstanceType::NotRequired) => Ordering::Greater,

                (KnownInstanceType::TypeIs, _) => Ordering::Less,
                (_, KnownInstanceType::TypeIs) => Ordering::Greater,

                (KnownInstanceType::ReadOnly, _) => Ordering::Less,
                (_, KnownInstanceType::ReadOnly) => Ordering::Greater,

                (KnownInstanceType::Union, _) => Ordering::Less,
                (_, KnownInstanceType::Union) => Ordering::Greater,

                (
                    KnownInstanceType::TypeAliasType(left),
                    KnownInstanceType::TypeAliasType(right),
                ) => left.cmp(right),
                (KnownInstanceType::TypeAliasType(_), _) => Ordering::Less,
                (_, KnownInstanceType::TypeAliasType(_)) => Ordering::Greater,

                (KnownInstanceType::TypeVar(left), KnownInstanceType::TypeVar(right)) => {
                    left.cmp(right)
                }
            }
        }

        (Type::KnownInstance(_), _) => Ordering::Less,
        (_, Type::KnownInstance(_)) => Ordering::Greater,

        (Type::PropertyInstance(left), Type::PropertyInstance(right)) => left.cmp(right),
        (Type::PropertyInstance(_), _) => Ordering::Less,
        (_, Type::PropertyInstance(_)) => Ordering::Greater,

        (Type::Dynamic(left), Type::Dynamic(right)) => dynamic_elements_ordering(*left, *right),
        (Type::Dynamic(_), _) => Ordering::Less,
        (_, Type::Dynamic(_)) => Ordering::Greater,

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

        #[cfg(debug_assertions)]
        (DynamicType::Todo(TodoType(left)), DynamicType::Todo(TodoType(right))) => left.cmp(right),

        #[cfg(not(debug_assertions))]
        (DynamicType::Todo(TodoType), DynamicType::Todo(TodoType)) => Ordering::Equal,

        (DynamicType::SubscriptedGeneric, _) => Ordering::Less,
        (_, DynamicType::SubscriptedGeneric) => Ordering::Greater,

        (DynamicType::SubscriptedProtocol, _) => Ordering::Less,
        (_, DynamicType::SubscriptedProtocol) => Ordering::Greater,
    }
}
