use std::cmp::Ordering;

use salsa::plumbing::AsId;

use crate::{
    db::Db,
    semantic_index::definition::Definition,
    types::{LiteralValueTypeKind, TypedDictType, bound_super::SuperOwnerKind, instance::Protocol},
};

use super::{
    DynamicType, TodoType, Type, TypeGuardLike, TypeGuardType, TypeIsType, class_base::ClassBase,
    subclass_of::SubclassOfInner,
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
    ordering_purpose: OrderingPurpose,
) -> Ordering {
    if ordering_purpose.is_normalization() {
        debug_assert_eq!(
            *left,
            left.normalized(db),
            "`left` must be normalized before a meaningful ordering \
            can be established for normalization"
        );
        debug_assert_eq!(
            *right,
            right.normalized(db),
            "`right` must be normalized before a meaningful ordering \
            can be established for normalization"
        );
    }

    if left == right {
        return Ordering::Equal;
    }

    match (left, right) {
        (Type::Never, _) => Ordering::Less,
        (_, Type::Never) => Ordering::Greater,

        (Type::LiteralValue(left), Type::LiteralValue(right)) => {
            match (left.kind(), right.kind()) {
                (LiteralValueTypeKind::LiteralString, _) => Ordering::Less,
                (_, LiteralValueTypeKind::LiteralString) => Ordering::Greater,

                (LiteralValueTypeKind::Bool(left), LiteralValueTypeKind::Bool(right)) => {
                    left.cmp(&right)
                }
                (LiteralValueTypeKind::Bool(_), _) => Ordering::Less,
                (_, LiteralValueTypeKind::Bool(_)) => Ordering::Greater,

                (LiteralValueTypeKind::Int(left), LiteralValueTypeKind::Int(right)) => {
                    left.cmp(&right)
                }
                (LiteralValueTypeKind::Int(_), _) => Ordering::Less,
                (_, LiteralValueTypeKind::Int(_)) => Ordering::Greater,

                (LiteralValueTypeKind::String(left), LiteralValueTypeKind::String(right)) => {
                    match ordering_purpose {
                        OrderingPurpose::Normalization => left.cmp(&right),
                        OrderingPurpose::Determinism => left.value(db).cmp(right.value(db)),
                    }
                }
                (LiteralValueTypeKind::String(_), _) => Ordering::Less,
                (_, LiteralValueTypeKind::String(_)) => Ordering::Greater,

                (LiteralValueTypeKind::Bytes(left), LiteralValueTypeKind::Bytes(right)) => {
                    match ordering_purpose {
                        OrderingPurpose::Normalization => left.cmp(&right),
                        OrderingPurpose::Determinism => left.value(db).cmp(right.value(db)),
                    }
                }
                (LiteralValueTypeKind::Bytes(_), _) => Ordering::Less,
                (_, LiteralValueTypeKind::Bytes(_)) => Ordering::Greater,

                (LiteralValueTypeKind::Enum(left), LiteralValueTypeKind::Enum(right)) => {
                    match ordering_purpose {
                        OrderingPurpose::Normalization => left.cmp(&right),
                        OrderingPurpose::Determinism => union_or_intersection_elements_ordering(
                            db,
                            &left.enum_class_instance(db),
                            &right.enum_class_instance(db),
                            ordering_purpose,
                        )
                        .then_with(|| left.name(db).cmp(right.name(db))),
                    }
                }
            }
        }

        (Type::LiteralValue(_), _) => Ordering::Less,
        (_, Type::LiteralValue(_)) => Ordering::Greater,

        (Type::FunctionLiteral(left), Type::FunctionLiteral(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => definition_ordering(
                db,
                left.definition(db),
                right.definition(db),
                ordering_purpose,
            )
            .then_with(|| {
                let left_sig = left.signature(db);
                let right_sig = right.signature(db);
                signatures_deterministic_ordering(db, left_sig, right_sig)
            }),
        },
        (Type::FunctionLiteral(_), _) => Ordering::Less,
        (_, Type::FunctionLiteral(_)) => Ordering::Greater,

        (Type::BoundMethod(left), Type::BoundMethod(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => union_or_intersection_elements_ordering(
                db,
                &left.self_instance(db),
                &right.self_instance(db),
                ordering_purpose,
            )
            .then_with(|| {
                union_or_intersection_elements_ordering(
                    db,
                    &Type::FunctionLiteral(left.function(db)),
                    &Type::FunctionLiteral(right.function(db)),
                    ordering_purpose,
                )
            }),
        },
        (Type::BoundMethod(_), _) => Ordering::Less,
        (_, Type::BoundMethod(_)) => Ordering::Greater,

        (Type::KnownBoundMethod(left), Type::KnownBoundMethod(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => {
                known_bound_method_deterministic_ordering(db, *left, *right)
            }
        },
        (Type::KnownBoundMethod(_), _) => Ordering::Less,
        (_, Type::KnownBoundMethod(_)) => Ordering::Greater,

        (Type::WrapperDescriptor(left), Type::WrapperDescriptor(right)) => left.cmp(right),
        (Type::WrapperDescriptor(_), _) => Ordering::Less,
        (_, Type::WrapperDescriptor(_)) => Ordering::Greater,

        (Type::DataclassDecorator(left), Type::DataclassDecorator(right)) => match ordering_purpose
        {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => left.flags(db).cmp(&right.flags(db)).then_with(|| {
                let left_specs = left.field_specifiers(db);
                let right_specs = right.field_specifiers(db);
                left_specs.len().cmp(&right_specs.len()).then_with(|| {
                    for (l, r) in left_specs.iter().zip(right_specs) {
                        let spec_cmp =
                            union_or_intersection_elements_ordering(db, l, r, ordering_purpose);
                        if spec_cmp != Ordering::Equal {
                            return spec_cmp;
                        }
                    }
                    Ordering::Equal
                })
            }),
        },
        (Type::DataclassDecorator(_), _) => Ordering::Less,
        (_, Type::DataclassDecorator(_)) => Ordering::Greater,

        (Type::DataclassTransformer(left), Type::DataclassTransformer(right)) => {
            match ordering_purpose {
                OrderingPurpose::Normalization => left.cmp(right),
                OrderingPurpose::Determinism => {
                    left.flags(db).cmp(&right.flags(db)).then_with(|| {
                        let left_specs = left.field_specifiers(db);
                        let right_specs = right.field_specifiers(db);
                        left_specs.len().cmp(&right_specs.len()).then_with(|| {
                            for (l, r) in left_specs.iter().zip(right_specs) {
                                let spec_cmp = union_or_intersection_elements_ordering(
                                    db,
                                    l,
                                    r,
                                    ordering_purpose,
                                );
                                if spec_cmp != Ordering::Equal {
                                    return spec_cmp;
                                }
                            }
                            Ordering::Equal
                        })
                    })
                }
            }
        }
        (Type::DataclassTransformer(_), _) => Ordering::Less,
        (_, Type::DataclassTransformer(_)) => Ordering::Greater,

        (Type::Callable(left), Type::Callable(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => left.kind(db).cmp(&right.kind(db)).then_with(|| {
                let left_sigs = left.signatures(db);
                let right_sigs = right.signatures(db);
                signatures_deterministic_ordering(db, left_sigs, right_sigs)
            }),
        },
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
                    union_or_intersection_elements_ordering(
                        db,
                        &Type::TypeVar(left),
                        &Type::TypeVar(right),
                        ordering_purpose,
                    )
                }
                (SubclassOfInner::TypeVar(_), _) => Ordering::Less,
                (_, SubclassOfInner::TypeVar(_)) => Ordering::Greater,
            }
        }

        (Type::SubclassOf(_), _) => Ordering::Less,
        (_, Type::SubclassOf(_)) => Ordering::Greater,

        (Type::TypeIs(left), Type::TypeIs(right)) => {
            typeis_ordering(db, *left, *right, ordering_purpose)
        }
        (Type::TypeIs(_), _) => Ordering::Less,
        (_, Type::TypeIs(_)) => Ordering::Greater,

        (Type::TypeGuard(left), Type::TypeGuard(right)) => {
            typeguard_ordering(db, *left, *right, ordering_purpose)
        }
        (Type::TypeGuard(_), _) => Ordering::Less,
        (_, Type::TypeGuard(_)) => Ordering::Greater,

        (Type::NominalInstance(left), Type::NominalInstance(right)) => {
            union_or_intersection_elements_ordering(
                db,
                &Type::from(left.class(db)),
                &Type::from(right.class(db)),
                ordering_purpose,
            )
        }
        (Type::NominalInstance(_), _) => Ordering::Less,
        (_, Type::NominalInstance(_)) => Ordering::Greater,

        (Type::ProtocolInstance(left_proto), Type::ProtocolInstance(right_proto)) => {
            match (left_proto.inner, right_proto.inner) {
                (Protocol::FromClass(left), Protocol::FromClass(right)) => left.cmp(&right),
                (Protocol::FromClass(_), _) => Ordering::Less,
                (_, Protocol::FromClass(_)) => Ordering::Greater,

                (Protocol::Synthesized(left), Protocol::Synthesized(right)) => {
                    match ordering_purpose {
                        OrderingPurpose::Normalization => left.cmp(&right),
                        OrderingPurpose::Determinism => {
                            let left_members = left.interface().members(db);
                            let right_members = right.interface().members(db);

                            let length_cmp = left_members.len().cmp(&right_members.len());
                            if length_cmp != Ordering::Equal {
                                return length_cmp;
                            }

                            for (left, right) in left_members.zip(right_members) {
                                let member_cmp = left.ordering(db, &right, ordering_purpose);
                                if member_cmp != Ordering::Equal {
                                    return member_cmp;
                                }
                            }

                            unreachable!(
                                "Two equal synthesized protocols should share the same Salsa ID"
                            )
                        }
                    }
                }
            }
        }
        (Type::ProtocolInstance(_), _) => Ordering::Less,
        (_, Type::ProtocolInstance(_)) => Ordering::Greater,

        // This is one place where we want to compare the typevar identities directly, instead of
        // falling back on `is_same_typevar_as` or `can_be_bound_for`.
        (Type::TypeVar(left), Type::TypeVar(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => left
                .name(db)
                .cmp(right.name(db))
                .then_with(|| left.kind(db).cmp(&right.kind(db)))
                .then_with(|| left.paramspec_attr(db).cmp(&right.paramspec_attr(db)))
                .then_with(|| {
                    match (
                        left.typevar(db).definition(db),
                        right.typevar(db).definition(db),
                    ) {
                        (Some(left), Some(right)) => {
                            definition_ordering(db, left, right, ordering_purpose)
                        }
                        (Some(_), None) => Ordering::Less,
                        (None, Some(_)) => Ordering::Greater,
                        (None, None) => Ordering::Equal,
                    }
                })
                .then_with(|| left.binding_context(db).cmp(&right.binding_context(db))),
        },
        (Type::TypeVar(_), _) => Ordering::Less,
        (_, Type::TypeVar(_)) => Ordering::Greater,

        (Type::AlwaysTruthy, _) => Ordering::Less,
        (_, Type::AlwaysTruthy) => Ordering::Greater,

        (Type::AlwaysFalsy, _) => Ordering::Less,
        (_, Type::AlwaysFalsy) => Ordering::Greater,

        (Type::BoundSuper(left), Type::BoundSuper(right)) => {
            (match (left.pivot_class(db), right.pivot_class(db)) {
                (ClassBase::Class(left), ClassBase::Class(right)) => {
                    union_or_intersection_elements_ordering(
                        db,
                        &Type::from(left),
                        &Type::from(right),
                        ordering_purpose,
                    )
                }
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
                (SuperOwnerKind::Class(left), SuperOwnerKind::Class(right)) => {
                    union_or_intersection_elements_ordering(
                        db,
                        &Type::from(left),
                        &Type::from(right),
                        ordering_purpose,
                    )
                }
                (SuperOwnerKind::Class(_), _) => Ordering::Less,
                (_, SuperOwnerKind::Class(_)) => Ordering::Greater,
                (SuperOwnerKind::Instance(left), SuperOwnerKind::Instance(right)) => {
                    union_or_intersection_elements_ordering(
                        db,
                        &Type::from(left.class(db)),
                        &Type::from(right.class(db)),
                        ordering_purpose,
                    )
                }
                (SuperOwnerKind::Instance(_), _) => Ordering::Less,
                (_, SuperOwnerKind::Instance(_)) => Ordering::Greater,
                (
                    SuperOwnerKind::InstanceTypeVar(left, _),
                    SuperOwnerKind::InstanceTypeVar(right, _),
                ) => union_or_intersection_elements_ordering(
                    db,
                    &Type::TypeVar(left),
                    &Type::TypeVar(right),
                    ordering_purpose,
                ),
                (SuperOwnerKind::InstanceTypeVar(..), _) => Ordering::Less,
                (_, SuperOwnerKind::InstanceTypeVar(..)) => Ordering::Greater,
                (SuperOwnerKind::ClassTypeVar(left, _), SuperOwnerKind::ClassTypeVar(right, _)) => {
                    union_or_intersection_elements_ordering(
                        db,
                        &Type::TypeVar(left),
                        &Type::TypeVar(right),
                        ordering_purpose,
                    )
                }
                (SuperOwnerKind::ClassTypeVar(..), _) => Ordering::Less,
                (_, SuperOwnerKind::ClassTypeVar(..)) => Ordering::Greater,
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

        (Type::KnownInstance(left), Type::KnownInstance(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => {
                known_instance_deterministic_ordering(db, *left, *right)
            }
        },
        (Type::KnownInstance(_), _) => Ordering::Less,
        (_, Type::KnownInstance(_)) => Ordering::Greater,

        (Type::PropertyInstance(left), Type::PropertyInstance(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => match (left.getter(db), right.getter(db)) {
                (Some(left), Some(right)) => {
                    union_or_intersection_elements_ordering(db, &left, &right, ordering_purpose)
                }
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => match (left.setter(db), right.setter(db)) {
                    (Some(left), Some(right)) => {
                        union_or_intersection_elements_ordering(db, &left, &right, ordering_purpose)
                    }
                    (Some(_), _) => Ordering::Less,
                    (_, Some(_)) => Ordering::Greater,
                    (None, None) => {
                        unreachable!("Two equal property instances should share the same Salsa ID")
                    }
                },
            },
        },
        (Type::PropertyInstance(_), _) => Ordering::Less,
        (_, Type::PropertyInstance(_)) => Ordering::Greater,

        (Type::Dynamic(left), Type::Dynamic(right)) => dynamic_elements_ordering(*left, *right),
        (Type::Dynamic(_), _) => Ordering::Less,
        (_, Type::Dynamic(_)) => Ordering::Greater,

        (Type::TypeAlias(left), Type::TypeAlias(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => match (left.definition(db), right.definition(db)) {
                (Some(left), Some(right)) => definition_ordering(db, left, right, ordering_purpose),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => union_or_intersection_elements_ordering(
                    db,
                    &left.value_type(db),
                    &right.value_type(db),
                    ordering_purpose,
                ),
            },
        },
        (Type::TypeAlias(_), _) => Ordering::Less,
        (_, Type::TypeAlias(_)) => Ordering::Greater,

        (Type::TypedDict(left), Type::TypedDict(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => match (left, right) {
                (TypedDictType::Class(left), TypedDictType::Class(right)) => {
                    union_or_intersection_elements_ordering(
                        db,
                        &Type::from(*left),
                        &Type::from(*right),
                        ordering_purpose,
                    )
                }
                (TypedDictType::Class(_), TypedDictType::Synthesized(_)) => Ordering::Less,
                (TypedDictType::Synthesized(_), TypedDictType::Class(_)) => Ordering::Greater,
                (TypedDictType::Synthesized(left), TypedDictType::Synthesized(right)) => {
                    let left_items = left.items(db);
                    let right_items = right.items(db);
                    left_items.len().cmp(&right_items.len()).then_with(|| {
                        for ((left_name, left_field), (right_name, right_field)) in
                            left_items.iter().zip(right_items.iter())
                        {
                            let name_cmp = left_name.cmp(right_name);
                            if name_cmp != Ordering::Equal {
                                return name_cmp;
                            }
                            let ty_cmp = union_or_intersection_elements_ordering(
                                db,
                                &left_field.declared_ty,
                                &right_field.declared_ty,
                                ordering_purpose,
                            );
                            if ty_cmp != Ordering::Equal {
                                return ty_cmp;
                            }
                        }
                        Ordering::Equal
                    })
                }
            },
        },
        (Type::TypedDict(_), _) => Ordering::Less,
        (_, Type::TypedDict(_)) => Ordering::Greater,

        (Type::NewTypeInstance(left), Type::NewTypeInstance(right)) => match ordering_purpose {
            OrderingPurpose::Normalization => left.cmp(right),
            OrderingPurpose::Determinism => definition_ordering(
                db,
                left.definition(db),
                right.definition(db),
                ordering_purpose,
            ),
        },
        (Type::NewTypeInstance(_), _) => Ordering::Less,
        (_, Type::NewTypeInstance(_)) => Ordering::Greater,

        (Type::Union(_), _) | (_, Type::Union(_)) if ordering_purpose.is_normalization() => {
            unreachable!("Our type representation does not permit nested unions")
        }

        (Type::Union(left), Type::Union(right)) => {
            let recursive_definition_cmp = left
                .recursively_defined(db)
                .cmp(&right.recursively_defined(db));
            if recursive_definition_cmp != Ordering::Equal {
                return recursive_definition_cmp;
            }

            let left_elements = left.elements(db);
            let right_elements = right.elements(db);

            let length_cmp = left_elements.len().cmp(&right_elements.len());
            if length_cmp != Ordering::Equal {
                return length_cmp;
            }

            for (left, right) in left_elements.iter().zip(right_elements) {
                let ordering =
                    union_or_intersection_elements_ordering(db, left, right, ordering_purpose);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }

            unreachable!("Two equal unions should share the same Salsa ID")
        }
        (Type::Union(_), _) => Ordering::Less,
        (_, Type::Union(_)) => Ordering::Greater,

        (Type::Intersection(left), Type::Intersection(right)) => {
            // Lexicographically compare the elements of the two unequal intersections.
            let left_positive = left.positive(db);
            let right_positive = right.positive(db);

            let pos_length_cmp = left_positive.len().cmp(&right_positive.len());
            if pos_length_cmp != Ordering::Equal {
                return pos_length_cmp;
            }

            let left_negative = left.negative(db);
            let right_negative = right.negative(db);

            let neg_length_cmp = left_negative.len().cmp(&right_negative.len());
            if neg_length_cmp != Ordering::Equal {
                return neg_length_cmp;
            }

            for (left, right) in left_positive.iter().zip(right_positive) {
                let ordering =
                    union_or_intersection_elements_ordering(db, left, right, ordering_purpose);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }

            for (left, right) in left_negative.iter().zip(right_negative) {
                let ordering =
                    union_or_intersection_elements_ordering(db, left, right, ordering_purpose);
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }

            unreachable!("Two equal intersections should share the same Salsa ID")
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

        (DynamicType::UnknownGeneric(left), DynamicType::UnknownGeneric(right)) => left.cmp(&right),
        (DynamicType::UnknownGeneric(_), _) => Ordering::Less,
        (_, DynamicType::UnknownGeneric(_)) => Ordering::Greater,

        (DynamicType::UnspecializedTypeVar, _) => Ordering::Less,
        (_, DynamicType::UnspecializedTypeVar) => Ordering::Greater,

        #[cfg(debug_assertions)]
        (DynamicType::Todo(TodoType(left)), DynamicType::Todo(TodoType(right))) => left.cmp(right),

        #[cfg(not(debug_assertions))]
        (DynamicType::Todo(TodoType), DynamicType::Todo(TodoType)) => Ordering::Equal,

        (DynamicType::TodoUnpack, _) => Ordering::Less,
        (_, DynamicType::TodoUnpack) => Ordering::Greater,

        (DynamicType::TodoStarredExpression, _) => Ordering::Less,
        (_, DynamicType::TodoStarredExpression) => Ordering::Greater,

        (DynamicType::TodoTypeVarTuple, _) => Ordering::Less,
        (_, DynamicType::TodoTypeVarTuple) => Ordering::Greater,

        (DynamicType::Divergent(left), DynamicType::Divergent(right)) => left.cmp(&right),
        (DynamicType::Divergent(_), _) => Ordering::Less,
        (_, DynamicType::Divergent(_)) => Ordering::Greater,
    }
}

/// Generic helper for ordering type guard-like types.
///
/// The following criteria are considered, in order:
/// * Boundness: Unbound precedes bound
/// * Symbol name: String comparison
/// * Guarded type: [`union_or_intersection_elements_ordering`]
fn guard_like_ordering<'db, T: TypeGuardLike<'db>>(
    db: &'db dyn Db,
    left: T,
    right: T,
    ordering_purpose: OrderingPurpose,
) -> Ordering {
    let (left_ty, right_ty) = (left.return_type(db), right.return_type(db));

    match (left.place_info(db), right.place_info(db)) {
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,

        (None, None) => {
            union_or_intersection_elements_ordering(db, &left_ty, &right_ty, ordering_purpose)
        }

        (Some(_), Some(_)) => match left.place_name(db).cmp(&right.place_name(db)) {
            Ordering::Equal => {
                union_or_intersection_elements_ordering(db, &left_ty, &right_ty, ordering_purpose)
            }
            ordering => ordering,
        },
    }
}

/// Determine a canonical order for two instances of [`TypeIsType`].
fn typeis_ordering(
    db: &dyn Db,
    left: TypeIsType,
    right: TypeIsType,
    ordering_purpose: OrderingPurpose,
) -> Ordering {
    guard_like_ordering(db, left, right, ordering_purpose)
}

/// Determine a canonical order for two instances of [`TypeGuardType`].
fn typeguard_ordering(
    db: &dyn Db,
    left: TypeGuardType,
    right: TypeGuardType,
    ordering_purpose: OrderingPurpose,
) -> Ordering {
    guard_like_ordering(db, left, right, ordering_purpose)
}

fn definition_ordering(
    db: &dyn Db,
    left: Definition,
    right: Definition,
    ordering_purpose: OrderingPurpose,
) -> Ordering {
    match ordering_purpose {
        OrderingPurpose::Normalization => left.cmp(&right),
        OrderingPurpose::Determinism => left
            .file(db)
            .path(db)
            .as_str()
            .cmp(right.file(db).path(db).as_str())
            .then_with(|| left.file_scope(db).cmp(&right.file_scope(db)))
            .then_with(|| left.place(db).cmp(&right.place(db))),
    }
}

/// Deterministic ordering for [`KnownBoundMethodType`] instances.
///
/// Uses the standard Less/Greater pattern for exhaustive matching without wildcards.
/// For inner types, we use structural comparison rather than Salsa IDs where possible.
fn known_bound_method_deterministic_ordering<'db>(
    db: &'db dyn Db,
    left: super::KnownBoundMethodType<'db>,
    right: super::KnownBoundMethodType<'db>,
) -> Ordering {
    use super::KnownBoundMethodType;

    match (left, right) {
        (
            KnownBoundMethodType::FunctionTypeDunderGet(l),
            KnownBoundMethodType::FunctionTypeDunderGet(r),
        ) => definition_ordering(
            db,
            l.definition(db),
            r.definition(db),
            OrderingPurpose::Determinism,
        )
        .then_with(|| {
            let l_sig = l.signature(db);
            let r_sig = r.signature(db);
            signatures_deterministic_ordering(db, l_sig, r_sig)
        }),
        (KnownBoundMethodType::FunctionTypeDunderGet(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::FunctionTypeDunderGet(_)) => Ordering::Greater,

        (
            KnownBoundMethodType::FunctionTypeDunderCall(l),
            KnownBoundMethodType::FunctionTypeDunderCall(r),
        ) => definition_ordering(
            db,
            l.definition(db),
            r.definition(db),
            OrderingPurpose::Determinism,
        )
        .then_with(|| {
            let l_sig = l.signature(db);
            let r_sig = r.signature(db);
            signatures_deterministic_ordering(db, l_sig, r_sig)
        }),
        (KnownBoundMethodType::FunctionTypeDunderCall(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::FunctionTypeDunderCall(_)) => Ordering::Greater,

        (
            KnownBoundMethodType::PropertyDunderGet(l),
            KnownBoundMethodType::PropertyDunderGet(r),
        ) => union_or_intersection_elements_ordering(
            db,
            &Type::PropertyInstance(l),
            &Type::PropertyInstance(r),
            OrderingPurpose::Determinism,
        ),
        (KnownBoundMethodType::PropertyDunderGet(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::PropertyDunderGet(_)) => Ordering::Greater,

        (
            KnownBoundMethodType::PropertyDunderSet(l),
            KnownBoundMethodType::PropertyDunderSet(r),
        ) => union_or_intersection_elements_ordering(
            db,
            &Type::PropertyInstance(l),
            &Type::PropertyInstance(r),
            OrderingPurpose::Determinism,
        ),
        (KnownBoundMethodType::PropertyDunderSet(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::PropertyDunderSet(_)) => Ordering::Greater,

        (KnownBoundMethodType::StrStartswith(l), KnownBoundMethodType::StrStartswith(r)) => {
            l.value(db).cmp(r.value(db))
        }
        (KnownBoundMethodType::StrStartswith(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::StrStartswith(_)) => Ordering::Greater,

        (KnownBoundMethodType::ConstraintSetRange, KnownBoundMethodType::ConstraintSetRange) => {
            Ordering::Equal
        }
        (KnownBoundMethodType::ConstraintSetRange, _) => Ordering::Less,
        (_, KnownBoundMethodType::ConstraintSetRange) => Ordering::Greater,

        (KnownBoundMethodType::ConstraintSetAlways, KnownBoundMethodType::ConstraintSetAlways) => {
            Ordering::Equal
        }
        (KnownBoundMethodType::ConstraintSetAlways, _) => Ordering::Less,
        (_, KnownBoundMethodType::ConstraintSetAlways) => Ordering::Greater,

        (KnownBoundMethodType::ConstraintSetNever, KnownBoundMethodType::ConstraintSetNever) => {
            Ordering::Equal
        }
        (KnownBoundMethodType::ConstraintSetNever, _) => Ordering::Less,
        (_, KnownBoundMethodType::ConstraintSetNever) => Ordering::Greater,

        (
            KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(l),
            KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(r),
        ) => l.as_id().cmp(&r.as_id()),
        (KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_)) => Ordering::Greater,

        (
            KnownBoundMethodType::ConstraintSetSatisfies(l),
            KnownBoundMethodType::ConstraintSetSatisfies(r),
        ) => l.as_id().cmp(&r.as_id()),
        (KnownBoundMethodType::ConstraintSetSatisfies(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::ConstraintSetSatisfies(_)) => Ordering::Greater,

        (
            KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(l),
            KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(r),
        ) => l.as_id().cmp(&r.as_id()),
        (KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_), _) => Ordering::Less,
        (_, KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_)) => Ordering::Greater,

        (
            KnownBoundMethodType::GenericContextSpecializeConstrained(l),
            KnownBoundMethodType::GenericContextSpecializeConstrained(r),
        ) => generic_context_deterministic_ordering(db, l, r),
    }
}

/// Deterministic ordering for [`KnownInstanceType`] instances.
///
/// Uses the standard Less/Greater pattern for exhaustive matching without wildcards.
/// For inner types, we use structural comparison rather than Salsa IDs where possible.
fn known_instance_deterministic_ordering<'db>(
    db: &'db dyn Db,
    left: super::KnownInstanceType<'db>,
    right: super::KnownInstanceType<'db>,
) -> Ordering {
    use super::KnownInstanceType;

    match (left, right) {
        (KnownInstanceType::SubscriptedProtocol(l), KnownInstanceType::SubscriptedProtocol(r)) => {
            generic_context_deterministic_ordering(db, l, r)
        }
        (KnownInstanceType::SubscriptedProtocol(_), _) => Ordering::Less,
        (_, KnownInstanceType::SubscriptedProtocol(_)) => Ordering::Greater,

        (KnownInstanceType::SubscriptedGeneric(l), KnownInstanceType::SubscriptedGeneric(r)) => {
            generic_context_deterministic_ordering(db, l, r)
        }
        (KnownInstanceType::SubscriptedGeneric(_), _) => Ordering::Less,
        (_, KnownInstanceType::SubscriptedGeneric(_)) => Ordering::Greater,

        (KnownInstanceType::TypeVar(l), KnownInstanceType::TypeVar(r)) => l
            .name(db)
            .cmp(r.name(db))
            .then_with(|| l.kind(db).cmp(&r.kind(db)))
            .then_with(|| match (l.definition(db), r.definition(db)) {
                (Some(l_def), Some(r_def)) => {
                    definition_ordering(db, l_def, r_def, OrderingPurpose::Determinism)
                }
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }),
        (KnownInstanceType::TypeVar(_), _) => Ordering::Less,
        (_, KnownInstanceType::TypeVar(_)) => Ordering::Greater,

        (KnownInstanceType::TypeAliasType(l), KnownInstanceType::TypeAliasType(r)) => {
            match (l.definition(db), r.definition(db)) {
                (Some(l_def), Some(r_def)) => {
                    definition_ordering(db, l_def, r_def, OrderingPurpose::Determinism)
                }
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => union_or_intersection_elements_ordering(
                    db,
                    &l.value_type(db),
                    &r.value_type(db),
                    OrderingPurpose::Determinism,
                ),
            }
        }
        (KnownInstanceType::TypeAliasType(_), _) => Ordering::Less,
        (_, KnownInstanceType::TypeAliasType(_)) => Ordering::Greater,

        (KnownInstanceType::Deprecated(l), KnownInstanceType::Deprecated(r)) => {
            match (l.message(db), r.message(db)) {
                (Some(l_msg), Some(r_msg)) => l_msg.value(db).cmp(r_msg.value(db)),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        }
        (KnownInstanceType::Deprecated(_), _) => Ordering::Less,
        (_, KnownInstanceType::Deprecated(_)) => Ordering::Greater,

        (KnownInstanceType::Field(l), KnownInstanceType::Field(r)) => l
            .init(db)
            .cmp(&r.init(db))
            .then_with(|| l.kw_only(db).cmp(&r.kw_only(db)))
            .then_with(|| l.alias(db).cmp(&r.alias(db)))
            .then_with(|| match (l.default_type(db), r.default_type(db)) {
                (Some(l_ty), Some(r_ty)) => union_or_intersection_elements_ordering(
                    db,
                    &l_ty,
                    &r_ty,
                    OrderingPurpose::Determinism,
                ),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }),
        (KnownInstanceType::Field(_), _) => Ordering::Less,
        (_, KnownInstanceType::Field(_)) => Ordering::Greater,

        // ConstraintSets wrap a BDD node that doesn't expose structural data for comparison.
        // Fall back to Salsa ID ordering. These are primarily used in mdtests.
        (KnownInstanceType::ConstraintSet(l), KnownInstanceType::ConstraintSet(r)) => {
            l.as_id().cmp(&r.as_id())
        }
        (KnownInstanceType::ConstraintSet(_), _) => Ordering::Less,
        (_, KnownInstanceType::ConstraintSet(_)) => Ordering::Greater,

        (KnownInstanceType::GenericContext(l), KnownInstanceType::GenericContext(r)) => {
            generic_context_deterministic_ordering(db, l, r)
        }
        (KnownInstanceType::GenericContext(_), _) => Ordering::Less,
        (_, KnownInstanceType::GenericContext(_)) => Ordering::Greater,

        (KnownInstanceType::Specialization(l), KnownInstanceType::Specialization(r)) => {
            specialization_deterministic_ordering(db, l, r)
        }
        (KnownInstanceType::Specialization(_), _) => Ordering::Less,
        (_, KnownInstanceType::Specialization(_)) => Ordering::Greater,

        (KnownInstanceType::UnionType(l), KnownInstanceType::UnionType(r)) => {
            match (l.union_type(db), r.union_type(db)) {
                (Ok(l_ty), Ok(r_ty)) => union_or_intersection_elements_ordering(
                    db,
                    l_ty,
                    r_ty,
                    OrderingPurpose::Determinism,
                ),
                (Ok(_), Err(_)) => Ordering::Less,
                (Err(_), Ok(_)) => Ordering::Greater,
                // Both errors: fall back to Salsa ID
                (Err(_), Err(_)) => l.as_id().cmp(&r.as_id()),
            }
        }
        (KnownInstanceType::UnionType(_), _) => Ordering::Less,
        (_, KnownInstanceType::UnionType(_)) => Ordering::Greater,

        (KnownInstanceType::Literal(l), KnownInstanceType::Literal(r)) => {
            union_or_intersection_elements_ordering(
                db,
                &l.inner(db),
                &r.inner(db),
                OrderingPurpose::Determinism,
            )
        }
        (KnownInstanceType::Literal(_), _) => Ordering::Less,
        (_, KnownInstanceType::Literal(_)) => Ordering::Greater,

        (KnownInstanceType::Annotated(l), KnownInstanceType::Annotated(r)) => {
            union_or_intersection_elements_ordering(
                db,
                &l.inner(db),
                &r.inner(db),
                OrderingPurpose::Determinism,
            )
        }
        (KnownInstanceType::Annotated(_), _) => Ordering::Less,
        (_, KnownInstanceType::Annotated(_)) => Ordering::Greater,

        (KnownInstanceType::TypeGenericAlias(l), KnownInstanceType::TypeGenericAlias(r)) => {
            union_or_intersection_elements_ordering(
                db,
                &l.inner(db),
                &r.inner(db),
                OrderingPurpose::Determinism,
            )
        }
        (KnownInstanceType::TypeGenericAlias(_), _) => Ordering::Less,
        (_, KnownInstanceType::TypeGenericAlias(_)) => Ordering::Greater,

        (KnownInstanceType::Callable(l), KnownInstanceType::Callable(r)) => {
            union_or_intersection_elements_ordering(
                db,
                &Type::Callable(l),
                &Type::Callable(r),
                OrderingPurpose::Determinism,
            )
        }
        (KnownInstanceType::Callable(_), _) => Ordering::Less,
        (_, KnownInstanceType::Callable(_)) => Ordering::Greater,

        (KnownInstanceType::LiteralStringAlias(l), KnownInstanceType::LiteralStringAlias(r)) => {
            union_or_intersection_elements_ordering(
                db,
                &l.inner(db),
                &r.inner(db),
                OrderingPurpose::Determinism,
            )
        }
        (KnownInstanceType::LiteralStringAlias(_), _) => Ordering::Less,
        (_, KnownInstanceType::LiteralStringAlias(_)) => Ordering::Greater,

        (KnownInstanceType::NewType(l), KnownInstanceType::NewType(r)) => definition_ordering(
            db,
            l.definition(db),
            r.definition(db),
            OrderingPurpose::Determinism,
        ),
        (KnownInstanceType::NewType(_), _) => Ordering::Less,
        (_, KnownInstanceType::NewType(_)) => Ordering::Greater,

        (KnownInstanceType::NamedTupleSpec(l), KnownInstanceType::NamedTupleSpec(r)) => {
            named_tuple_spec_deterministic_ordering(db, l, r)
        }
    }
}

/// Deterministic ordering for [`CallableSignature`] instances.
///
/// Compares signatures structurally by comparing their parameters and return types.
fn signatures_deterministic_ordering<'db>(
    db: &'db dyn Db,
    left: &super::signatures::CallableSignature<'db>,
    right: &super::signatures::CallableSignature<'db>,
) -> Ordering {
    let left_overloads: Vec<_> = left.iter().collect();
    let right_overloads: Vec<_> = right.iter().collect();

    left_overloads
        .len()
        .cmp(&right_overloads.len())
        .then_with(|| {
            for (l, r) in left_overloads.iter().zip(right_overloads.iter()) {
                let params_cmp =
                    parameters_deterministic_ordering(db, l.parameters(), r.parameters());
                if params_cmp != Ordering::Equal {
                    return params_cmp;
                }

                let ret_cmp = union_or_intersection_elements_ordering(
                    db,
                    &l.return_ty,
                    &r.return_ty,
                    OrderingPurpose::Determinism,
                );
                if ret_cmp != Ordering::Equal {
                    return ret_cmp;
                }
            }
            Ordering::Equal
        })
}

/// Deterministic ordering for [`Parameters`] instances.
fn parameters_deterministic_ordering<'db>(
    db: &'db dyn Db,
    left: &super::signatures::Parameters<'db>,
    right: &super::signatures::Parameters<'db>,
) -> Ordering {
    left.as_slice()
        .len()
        .cmp(&right.as_slice().len())
        .then_with(|| {
            for (l, r) in left.as_slice().iter().zip(right.as_slice()) {
                let kind_cmp = parameter_kind_discriminant(l.kind())
                    .cmp(&parameter_kind_discriminant(r.kind()));
                if kind_cmp != Ordering::Equal {
                    return kind_cmp;
                }

                let name_cmp = l.name().cmp(&r.name());
                if name_cmp != Ordering::Equal {
                    return name_cmp;
                }

                let ty_cmp = union_or_intersection_elements_ordering(
                    db,
                    &l.annotated_type(),
                    &r.annotated_type(),
                    OrderingPurpose::Determinism,
                );
                if ty_cmp != Ordering::Equal {
                    return ty_cmp;
                }

                match (l.default_type(), r.default_type()) {
                    (Some(l_default), Some(r_default)) => {
                        let default_cmp = union_or_intersection_elements_ordering(
                            db,
                            &l_default,
                            &r_default,
                            OrderingPurpose::Determinism,
                        );
                        if default_cmp != Ordering::Equal {
                            return default_cmp;
                        }
                    }
                    (Some(_), None) => return Ordering::Less,
                    (None, Some(_)) => return Ordering::Greater,
                    (None, None) => {}
                }
            }
            Ordering::Equal
        })
}

/// Assign a stable discriminant to each [`ParameterKind`] variant.
fn parameter_kind_discriminant(kind: &super::signatures::ParameterKind) -> u8 {
    use super::signatures::ParameterKind;
    match kind {
        ParameterKind::PositionalOnly { .. } => 0,
        ParameterKind::PositionalOrKeyword { .. } => 1,
        ParameterKind::Variadic { .. } => 2,
        ParameterKind::KeywordOnly { .. } => 3,
        ParameterKind::KeywordVariadic { .. } => 4,
    }
}

/// Deterministic ordering for [`GenericContext`] instances.
///
/// Compares the type variables structurally by name and kind.
fn generic_context_deterministic_ordering<'db>(
    db: &'db dyn Db,
    left: super::generics::GenericContext<'db>,
    right: super::generics::GenericContext<'db>,
) -> Ordering {
    let left_vars: Vec<_> = left.variables(db).collect();
    let right_vars: Vec<_> = right.variables(db).collect();

    left_vars.len().cmp(&right_vars.len()).then_with(|| {
        for (l, r) in left_vars.iter().zip(right_vars.iter()) {
            let cmp = union_or_intersection_elements_ordering(
                db,
                &Type::TypeVar(*l),
                &Type::TypeVar(*r),
                OrderingPurpose::Determinism,
            );
            if cmp != Ordering::Equal {
                return cmp;
            }
        }
        Ordering::Equal
    })
}

/// Deterministic ordering for [`Specialization`] instances.
///
/// Compares the generic context, types, and materialization kind structurally.
fn specialization_deterministic_ordering<'db>(
    db: &'db dyn Db,
    left: super::generics::Specialization<'db>,
    right: super::generics::Specialization<'db>,
) -> Ordering {
    generic_context_deterministic_ordering(db, left.generic_context(db), right.generic_context(db))
        .then_with(|| {
            let left_types = left.types(db);
            let right_types = right.types(db);
            left_types.len().cmp(&right_types.len()).then_with(|| {
                for (l, r) in left_types.iter().zip(right_types.iter()) {
                    let cmp = union_or_intersection_elements_ordering(
                        db,
                        l,
                        r,
                        OrderingPurpose::Determinism,
                    );
                    if cmp != Ordering::Equal {
                        return cmp;
                    }
                }
                Ordering::Equal
            })
        })
        .then_with(|| {
            match (
                left.materialization_kind(db),
                right.materialization_kind(db),
            ) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Less,
                (Some(_), None) => Ordering::Greater,
                (Some(super::MaterializationKind::Top), Some(super::MaterializationKind::Top))
                | (
                    Some(super::MaterializationKind::Bottom),
                    Some(super::MaterializationKind::Bottom),
                ) => Ordering::Equal,
                (
                    Some(super::MaterializationKind::Top),
                    Some(super::MaterializationKind::Bottom),
                ) => Ordering::Less,
                (
                    Some(super::MaterializationKind::Bottom),
                    Some(super::MaterializationKind::Top),
                ) => Ordering::Greater,
            }
        })
}

/// Deterministic ordering for [`NamedTupleSpec`] instances.
///
/// Compares the fields structurally by name and type.
fn named_tuple_spec_deterministic_ordering<'db>(
    db: &'db dyn Db,
    left: super::class::NamedTupleSpec<'db>,
    right: super::class::NamedTupleSpec<'db>,
) -> Ordering {
    left.has_known_fields(db)
        .cmp(&right.has_known_fields(db))
        .then_with(|| {
            let left_fields = left.fields(db);
            let right_fields = right.fields(db);
            left_fields.len().cmp(&right_fields.len()).then_with(|| {
                for (l, r) in left_fields.iter().zip(right_fields.iter()) {
                    let name_cmp = l.name.cmp(&r.name);
                    if name_cmp != Ordering::Equal {
                        return name_cmp;
                    }
                    let ty_cmp = union_or_intersection_elements_ordering(
                        db,
                        &l.ty,
                        &r.ty,
                        OrderingPurpose::Determinism,
                    );
                    if ty_cmp != Ordering::Equal {
                        return ty_cmp;
                    }
                    match (&l.default, &r.default) {
                        (Some(l_default), Some(r_default)) => {
                            let default_cmp = union_or_intersection_elements_ordering(
                                db,
                                l_default,
                                r_default,
                                OrderingPurpose::Determinism,
                            );
                            if default_cmp != Ordering::Equal {
                                return default_cmp;
                            }
                        }
                        (Some(_), None) => return Ordering::Less,
                        (None, Some(_)) => return Ordering::Greater,
                        (None, None) => {}
                    }
                }
                Ordering::Equal
            })
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OrderingPurpose {
    Normalization,
    Determinism,
}

impl OrderingPurpose {
    const fn is_normalization(self) -> bool {
        matches!(self, OrderingPurpose::Normalization)
    }
}
