use ruff_python_ast::name::Name;

use super::{Truthiness, Type};
use crate::{
    Db,
    place::PlaceAndQualifiers,
    types::{
        CallArguments, EnumLiteralType, IntersectionBuilder, KnownClass, MemberLookupPolicy,
        TypeContext, TypeVarBoundOrConstraints, UnionBuilder, UnionType, enums::enum_metadata,
    },
};

pub(super) fn evaluate_type_equality<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
) -> EqualityResult<'db> {
    let special_case = equality_special_case(db, left, right, is_positive);

    if special_case != EqualityResult::Ambiguous {
        return special_case;
    }

    let Ok(eq_bindings) = left.try_call_dunder(
        db,
        "__eq__",
        CallArguments::positional([right]),
        TypeContext::default(),
    ) else {
        return EqualityResult::Ambiguous;
    };
    let Ok(ne_bindings) = left.try_call_dunder(
        db,
        "__ne__",
        CallArguments::positional([right]),
        TypeContext::default(),
    ) else {
        return EqualityResult::Ambiguous;
    };
    let eq_truthiness = eq_bindings.return_type(db).bool(db);
    if eq_truthiness == Truthiness::Ambiguous {
        return EqualityResult::Ambiguous;
    }
    let ne_truthiness = ne_bindings.return_type(db).bool(db);
    if ne_truthiness == eq_truthiness {
        EqualityResult::Ambiguous
    } else {
        match eq_truthiness {
            Truthiness::AlwaysTrue => EqualityResult::AlwaysEqual,
            Truthiness::AlwaysFalse => EqualityResult::AlwaysUnequal,
            Truthiness::Ambiguous => EqualityResult::Ambiguous,
        }
    }
}

fn equality_special_case<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    is_positive: bool,
) -> EqualityResult<'db> {
    match (left, right) {
        (
            Type::Never
            | Type::Dynamic(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::ProtocolInstance(_)
            | Type::DataclassTransformer(_)
            | Type::TypeGuard(_)
            | Type::TypeIs(_),
            _,
        )
        | (
            _,
            Type::Never
            | Type::Dynamic(_)
            | Type::AlwaysFalsy
            | Type::AlwaysTruthy
            | Type::ProtocolInstance(_)
            | Type::DataclassTransformer(_)
            | Type::TypeGuard(_)
            | Type::TypeIs(_),
        ) => EqualityResult::Ambiguous,

        (Type::TypeAlias(alias), other) | (other, Type::TypeAlias(alias)) => {
            equality_special_case(db, alias.value_type(db), other, is_positive)
        }

        (Type::TypeVar(var), other) | (other, Type::TypeVar(var)) => {
            match var.typevar(db).bound_or_constraints(db) {
                None => EqualityResult::Ambiguous,
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    equality_special_case(db, bound, other, is_positive)
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    equality_special_case(db, constraints.as_type(db), other, is_positive)
                }
            }
        }

        (Type::NewTypeInstance(newtype), other) | (other, Type::NewTypeInstance(newtype)) => {
            // We cannot narrow a `NewType` in most cases where we can narrow its concrete base type,
            // because of the fact that e.g. `NewType('UserId', int)` is disjoint from `Literal[42]`,
            // and `NewType('UserId', <some enum class>)` is disjoint from `Literal[<some enum member>]`.
            match equality_special_case(db, newtype.concrete_base_type(db), other, is_positive) {
                EqualityResult::AlwaysEqual => EqualityResult::AlwaysEqual,
                EqualityResult::AlwaysUnequal => EqualityResult::AlwaysUnequal,
                EqualityResult::CanNarrow(_) | EqualityResult::Ambiguous => {
                    EqualityResult::Ambiguous
                }
            }
        }

        (Type::Union(union), other) | (other, Type::Union(union)) => {
            let mut all_always_equal = true;
            let mut all_always_unequal = true;
            let mut narrowed_union = UnionBuilder::new(db);
            for element in union.elements(db) {
                match equality_special_case(db, *element, other, is_positive) {
                    EqualityResult::AlwaysEqual => {
                        all_always_unequal = false;
                        if is_positive {
                            narrowed_union = narrowed_union.add(*element);
                        }
                    }
                    EqualityResult::Ambiguous => {
                        all_always_equal = false;
                        all_always_unequal = false;
                        narrowed_union = narrowed_union.add(*element);
                    }
                    EqualityResult::CanNarrow(narrowed_element) => {
                        all_always_equal = false;
                        all_always_unequal = false;
                        narrowed_union = narrowed_union.add(narrowed_element);
                    }
                    EqualityResult::AlwaysUnequal => {
                        all_always_equal = false;
                        if !is_positive {
                            narrowed_union = narrowed_union.add(*element);
                        }
                    }
                }
            }
            if all_always_equal {
                EqualityResult::AlwaysEqual
            } else if all_always_unequal {
                EqualityResult::AlwaysUnequal
            } else {
                EqualityResult::CanNarrow(narrowed_union.build())
            }
        }

        (Type::Intersection(intersection), other) | (other, Type::Intersection(intersection)) => {
            let mut builder = IntersectionBuilder::new(db);
            for element in intersection.positive(db) {
                match equality_special_case(db, *element, other, is_positive) {
                    EqualityResult::AlwaysEqual => {
                        return if is_positive {
                            EqualityResult::AlwaysEqual
                        } else {
                            EqualityResult::AlwaysUnequal
                        };
                    }
                    EqualityResult::AlwaysUnequal => {
                        return if is_positive {
                            EqualityResult::AlwaysUnequal
                        } else {
                            EqualityResult::AlwaysEqual
                        };
                    }
                    EqualityResult::Ambiguous => {
                        builder = builder.add_positive(*element);
                    }
                    EqualityResult::CanNarrow(narrowed_element) => {
                        builder = builder.add_positive(narrowed_element);
                    }
                }
            }
            for element in intersection.negative(db) {
                builder = builder.add_negative(*element);
            }
            EqualityResult::CanNarrow(builder.build())
        }

        (Type::Callable(callable), other) | (other, Type::Callable(callable)) => {
            if callable.is_function_like(db) {
                equality_special_case(
                    db,
                    other,
                    KnownClass::FunctionType.to_instance(db),
                    is_positive,
                )
            } else {
                EqualityResult::Ambiguous
            }
        }

        (Type::BooleanLiteral(b), other) | (other, Type::BooleanLiteral(b)) => {
            equality_special_case(db, Type::IntLiteral(i64::from(b)), other, is_positive)
        }

        (Type::IntLiteral(l), Type::IntLiteral(r)) => EqualityResult::from(l == r),

        (Type::BytesLiteral(b1), Type::BytesLiteral(b2)) => EqualityResult::from(b1 == b2),

        (Type::EnumLiteral(l), Type::EnumLiteral(r)) => {
            let left_instance = l.enum_class_instance(db);
            let right_instance = r.enum_class_instance(db);
            let Some(left_equality_semantics) =
                KnownEqualitySemantics::for_final_instance(db, left_instance)
            else {
                return EqualityResult::Ambiguous;
            };
            let Some(right_equality_semantics) =
                KnownEqualitySemantics::for_final_instance(db, right_instance)
            else {
                return EqualityResult::Ambiguous;
            };
            if left_equality_semantics == right_equality_semantics {
                if left_equality_semantics == KnownEqualitySemantics::Object {
                    EqualityResult::from(l == r)
                } else {
                    EqualityResult::from(l.value(db) == r.value(db))
                }
            } else {
                equality_special_case(db, left_instance, right_instance, is_positive)
            }
        }

        (Type::IntLiteral(int), Type::EnumLiteral(e))
        | (Type::EnumLiteral(e), Type::IntLiteral(int)) => {
            match KnownEqualitySemantics::for_final_instance(db, e.enum_class_instance(db)) {
                Some(KnownEqualitySemantics::Int) => {
                    EqualityResult::from(e.value(db) == Type::IntLiteral(int))
                }
                Some(
                    KnownEqualitySemantics::Bytes
                    | KnownEqualitySemantics::Object
                    | KnownEqualitySemantics::Tuple
                    | KnownEqualitySemantics::Str,
                ) => EqualityResult::AlwaysUnequal,
                None => EqualityResult::Ambiguous,
            }
        }

        (Type::BytesLiteral(b), Type::EnumLiteral(e))
        | (Type::EnumLiteral(e), Type::BytesLiteral(b)) => {
            match KnownEqualitySemantics::for_final_instance(db, e.enum_class_instance(db)) {
                Some(KnownEqualitySemantics::Bytes) => {
                    EqualityResult::from(e.value(db) == Type::BytesLiteral(b))
                }
                Some(
                    KnownEqualitySemantics::Int
                    | KnownEqualitySemantics::Object
                    | KnownEqualitySemantics::Tuple
                    | KnownEqualitySemantics::Str,
                ) => EqualityResult::AlwaysUnequal,
                None => EqualityResult::Ambiguous,
            }
        }

        (Type::StringLiteral(s), Type::EnumLiteral(e))
        | (Type::EnumLiteral(e), Type::StringLiteral(s)) => {
            match KnownEqualitySemantics::for_final_instance(db, e.enum_class_instance(db)) {
                Some(KnownEqualitySemantics::Str) => {
                    EqualityResult::from(e.value(db) == Type::StringLiteral(s))
                }
                Some(
                    KnownEqualitySemantics::Bytes
                    | KnownEqualitySemantics::Int
                    | KnownEqualitySemantics::Tuple
                    | KnownEqualitySemantics::Object,
                ) => EqualityResult::AlwaysUnequal,
                None => EqualityResult::Ambiguous,
            }
        }

        (Type::LiteralString, Type::EnumLiteral(e))
        | (Type::EnumLiteral(e), Type::LiteralString) => {
            match KnownEqualitySemantics::for_final_instance(db, e.enum_class_instance(db)) {
                Some(KnownEqualitySemantics::Str) => {
                    if let Type::StringLiteral(string) = e.value(db) {
                        let can_narrow_to = [Type::StringLiteral(string), Type::EnumLiteral(e)];
                        EqualityResult::CanNarrow(
                            UnionType::from_elements(db, can_narrow_to).negate_if(db, !is_positive),
                        )
                    } else {
                        EqualityResult::Ambiguous
                    }
                }
                Some(
                    KnownEqualitySemantics::Bytes
                    | KnownEqualitySemantics::Int
                    | KnownEqualitySemantics::Tuple
                    | KnownEqualitySemantics::Object,
                ) => EqualityResult::AlwaysUnequal,
                None => EqualityResult::Ambiguous,
            }
        }

        (Type::TypedDict(_), Type::EnumLiteral(e)) | (Type::EnumLiteral(e), Type::TypedDict(_)) => {
            if KnownEqualitySemantics::for_final_instance(db, e.enum_class_instance(db)).is_some() {
                EqualityResult::AlwaysUnequal
            } else {
                EqualityResult::Ambiguous
            }
        }

        (Type::NominalInstance(instance), Type::StringLiteral(s))
        | (Type::StringLiteral(s), Type::NominalInstance(instance)) => {
            let class = instance.class(db).class_literal(db);
            if let Some(enum_metadata) = enum_metadata(db, class) {
                match KnownEqualitySemantics::for_final_instance(
                    db,
                    Type::NominalInstance(instance),
                ) {
                    Some(KnownEqualitySemantics::Str) => {
                        if let Some((name, _)) = enum_metadata
                            .members
                            .iter()
                            .find(|(_, value)| **value == Type::StringLiteral(s))
                        {
                            let enum_member =
                                Type::EnumLiteral(EnumLiteralType::new(db, class, name));
                            let can_narrow_to = [Type::StringLiteral(s), enum_member];
                            EqualityResult::CanNarrow(
                                UnionType::from_elements(db, can_narrow_to)
                                    .negate_if(db, !is_positive),
                            )
                        } else {
                            EqualityResult::Ambiguous
                        }
                    }
                    None => EqualityResult::Ambiguous,
                    Some(_) => EqualityResult::AlwaysUnequal,
                }
            } else if instance.class(db).is_final(db) || instance.tuple_spec(db).is_some() {
                match KnownEqualitySemantics::for_final_instance(
                    db,
                    Type::NominalInstance(instance),
                ) {
                    Some(KnownEqualitySemantics::Str) | None => EqualityResult::Ambiguous,
                    Some(_) => EqualityResult::AlwaysUnequal,
                }
            } else if !is_positive {
                EqualityResult::CanNarrow(Type::StringLiteral(s).negate(db))
            } else {
                EqualityResult::Ambiguous
            }
        }

        (Type::StringLiteral(l), Type::StringLiteral(r)) => EqualityResult::from(l == r),

        (string @ Type::StringLiteral(_), Type::LiteralString)
        | (Type::LiteralString, string @ Type::StringLiteral(_)) => {
            EqualityResult::CanNarrow(string.negate_if(db, !is_positive))
        }

        (Type::StringLiteral(_), other) | (other, Type::StringLiteral(_)) => {
            equality_special_case(db, Type::LiteralString, other, is_positive)
        }

        (Type::LiteralString, Type::LiteralString) => EqualityResult::Ambiguous,

        (Type::DataclassDecorator(_), Type::DataclassDecorator(_)) => EqualityResult::Ambiguous,

        (Type::FunctionLiteral(l), Type::FunctionLiteral(r)) => {
            EqualityResult::from(l.literal(db) == r.literal(db))
        }

        (Type::FunctionLiteral(_) | Type::DataclassDecorator(_), other)
        | (other, Type::FunctionLiteral(_) | Type::DataclassDecorator(_)) => {
            // will unnecessarily return `None` in many instances if `FunctionType` is not `@final`.
            debug_assert!(
                KnownClass::FunctionType
                    .to_class_literal(db)
                    .expect_class_literal()
                    .is_final(db)
            );
            equality_special_case(
                db,
                KnownClass::FunctionType.to_instance(db),
                other,
                is_positive,
            )
        }

        (Type::BoundMethod(l), Type::BoundMethod(r)) => {
            if l.function(db).literal(db) != r.function(db).literal(db) {
                EqualityResult::AlwaysUnequal
            } else {
                EqualityResult::Ambiguous
            }
        }

        (Type::BoundMethod(_), other) | (other, Type::BoundMethod(_)) => {
            // will unnecessarily return `None` in many instances if `MethodType` is not `@final`.
            debug_assert!(
                KnownClass::MethodType
                    .to_class_literal(db)
                    .expect_class_literal()
                    .is_final(db)
            );
            equality_special_case(
                db,
                KnownClass::MethodType.to_instance(db),
                other,
                is_positive,
            )
        }

        (Type::WrapperDescriptor(l), Type::WrapperDescriptor(r)) => {
            if l == r {
                EqualityResult::AlwaysEqual
            } else {
                EqualityResult::Ambiguous
            }
        }

        (Type::WrapperDescriptor(_), other) | (other, Type::WrapperDescriptor(_)) => {
            // will unnecessarily return `None` in many instances if `WrapperDescriptorType` is not `@final`.
            debug_assert!(
                KnownClass::WrapperDescriptorType
                    .to_class_literal(db)
                    .expect_class_literal()
                    .is_final(db)
            );
            equality_special_case(
                db,
                KnownClass::WrapperDescriptorType.to_instance(db),
                other,
                is_positive,
            )
        }

        (Type::BoundSuper(l), Type::BoundSuper(r)) => {
            if l.owner(db) != r.owner(db) && l.pivot_class(db) != r.pivot_class(db) {
                EqualityResult::AlwaysUnequal
            } else {
                EqualityResult::Ambiguous
            }
        }

        // We could do better here but it's unclear if it's worth it.
        // There's no point delegating to `KnownClass::Super.to_instance()`
        // because `super` is not a `@final` class.
        (Type::BoundSuper(_), _) | (_, Type::BoundSuper(_)) => EqualityResult::Ambiguous,

        (Type::SpecialForm(l), Type::SpecialForm(r)) => EqualityResult::from(l == r),

        (Type::SpecialForm(form), other) | (other, Type::SpecialForm(form)) => {
            equality_special_case(db, form.instance_fallback(db), other, is_positive)
        }

        (Type::ModuleLiteral(l), Type::ModuleLiteral(r)) => {
            EqualityResult::from(l.module(db) == r.module(db))
        }

        // We might be able to do better here in some cases, but it's unclear if it's worth it
        (Type::ModuleLiteral(_), _) | (_, Type::ModuleLiteral(_)) => EqualityResult::Ambiguous,

        (Type::ClassLiteral(l), Type::ClassLiteral(r)) => {
            if KnownEqualitySemantics::for_final_instance(db, l.metaclass_instance_type(db))
                == Some(KnownEqualitySemantics::Object)
            {
                EqualityResult::from(l == r)
            } else {
                EqualityResult::Ambiguous
            }
        }

        // we might be able to do better here after https://github.com/astral-sh/ty/issues/1859 etc. are resolved
        (Type::GenericAlias(_), _) | (_, Type::GenericAlias(_)) => EqualityResult::Ambiguous,

        // Complicated to get right in its entirety (need to recurse into inner variants);
        // unclear if the maintenance effort is worth it
        (Type::KnownBoundMethod(_), Type::KnownBoundMethod(_)) => EqualityResult::Ambiguous,

        // We could do better here too but it's unclear if it's worth it
        (Type::KnownBoundMethod(m), other) | (other, Type::KnownBoundMethod(m)) => {
            // will unnecessarily return `None` in many instances if `WrapperDescriptorType` is not `@final`.
            debug_assert!(
                m.class()
                    .to_class_literal(db)
                    .expect_class_literal()
                    .is_final(db)
            );
            equality_special_case(db, m.class().to_instance(db), other, is_positive)
        }

        // We could possibly do better for `closed=True` `TypedDict`s?
        (Type::TypedDict(_), Type::TypedDict(_)) => EqualityResult::Ambiguous,

        // We might be able to do better here in some cases, but it's unclear if it's worth it
        (Type::KnownInstance(i), other) | (other, Type::KnownInstance(i)) => {
            equality_special_case(db, i.instance_fallback(db), other, is_positive)
        }

        // We might be able to do better here in some cases, but it's unclear if it's worth it.
        // There's no point delegating to `KnownClass::property.to_instance()`
        // because `property` is not a `@final` class.
        (Type::PropertyInstance(_), _) | (_, Type::PropertyInstance(_)) => {
            EqualityResult::Ambiguous
        }

        // We should probably do better here,
        // but we need to be careful to respect the difference between instances of `type` and generic-alias instances.
        // We also need to make sure we respect the fact that metaclasses can override `__eq__` and `__ne__`.
        (Type::SubclassOf(_), _) | (_, Type::SubclassOf(_)) => EqualityResult::Ambiguous,

        (
            Type::IntLiteral(_),
            Type::BytesLiteral(_)
            | Type::ClassLiteral(_)
            | Type::TypedDict(_)
            | Type::LiteralString,
        )
        | (
            Type::BytesLiteral(_)
            | Type::ClassLiteral(_)
            | Type::TypedDict(_)
            | Type::LiteralString,
            Type::IntLiteral(_),
        ) => EqualityResult::AlwaysUnequal,

        (
            Type::LiteralString,
            Type::BytesLiteral(_) | Type::ClassLiteral(_) | Type::TypedDict(_),
        )
        | (
            Type::BytesLiteral(_) | Type::ClassLiteral(_) | Type::TypedDict(_),
            Type::LiteralString,
        ) => EqualityResult::AlwaysUnequal,

        (Type::BytesLiteral(_), Type::ClassLiteral(_) | Type::TypedDict(_))
        | (Type::ClassLiteral(_) | Type::TypedDict(_), Type::BytesLiteral(_)) => {
            EqualityResult::AlwaysUnequal
        }

        (Type::ClassLiteral(_), Type::TypedDict(_))
        | (Type::TypedDict(_), Type::ClassLiteral(_)) => EqualityResult::AlwaysUnequal,

        (Type::ClassLiteral(c), other @ (Type::NominalInstance(_) | Type::EnumLiteral(_)))
        | (other @ (Type::NominalInstance(_) | Type::EnumLiteral(_)), Type::ClassLiteral(c)) => {
            equality_special_case(db, c.metaclass_instance_type(db), other, is_positive)
        }

        (Type::NominalInstance(instance), Type::IntLiteral(i))
        | (Type::IntLiteral(i), Type::NominalInstance(instance)) => {
            let class = instance.class(db).class_literal(db);
            if class.is_known(db, KnownClass::Bool) {
                return match i {
                    0 | 1 => {
                        let result = if is_positive { i == 1 } else { i != 1 };
                        EqualityResult::CanNarrow(Type::BooleanLiteral(result))
                    }
                    _ => EqualityResult::AlwaysUnequal,
                };
            }

            if let Some(enum_metadata) = enum_metadata(db, class) {
                return match KnownEqualitySemantics::for_final_instance(
                    db,
                    Type::NominalInstance(instance),
                ) {
                    Some(KnownEqualitySemantics::Int) => {
                        if let Some((name, _)) = enum_metadata
                            .members
                            .iter()
                            .find(|(_, value)| **value == Type::IntLiteral(i))
                        {
                            let enum_member =
                                Type::EnumLiteral(EnumLiteralType::new(db, class, name));
                            let can_narrow_to = [Type::IntLiteral(i), enum_member];
                            EqualityResult::CanNarrow(
                                UnionType::from_elements(db, can_narrow_to)
                                    .negate_if(db, !is_positive),
                            )
                        } else {
                            EqualityResult::Ambiguous
                        }
                    }
                    None => EqualityResult::Ambiguous,
                    Some(_) => EqualityResult::AlwaysUnequal,
                };
            }

            if instance.class(db).is_final(db) || instance.tuple_spec(db).is_some() {
                return match KnownEqualitySemantics::for_final_instance(
                    db,
                    Type::NominalInstance(instance),
                ) {
                    Some(KnownEqualitySemantics::Int) | None => EqualityResult::Ambiguous,
                    Some(_) => EqualityResult::AlwaysUnequal,
                };
            }

            if !is_positive {
                return EqualityResult::CanNarrow(Type::IntLiteral(i).negate(db));
            }

            EqualityResult::Ambiguous
        }

        (Type::NominalInstance(instance), Type::LiteralString)
        | (Type::LiteralString, Type::NominalInstance(instance)) => {
            let class = instance.class(db);
            if class.is_final(db) || instance.tuple_spec(db).is_some() {
                match KnownEqualitySemantics::for_final_instance(
                    db,
                    Type::NominalInstance(instance),
                ) {
                    Some(KnownEqualitySemantics::Str) | None => EqualityResult::Ambiguous,
                    Some(_) => EqualityResult::AlwaysUnequal,
                }
            } else {
                EqualityResult::Ambiguous
            }
        }

        (Type::NominalInstance(instance), Type::BytesLiteral(b))
        | (Type::BytesLiteral(b), Type::NominalInstance(instance)) => {
            let class = instance.class(db).class_literal(db);
            if let Some(enum_metadata) = enum_metadata(db, class) {
                match KnownEqualitySemantics::for_final_instance(
                    db,
                    Type::NominalInstance(instance),
                ) {
                    Some(KnownEqualitySemantics::Bytes) => {
                        if let Some((name, _)) = enum_metadata
                            .members
                            .iter()
                            .find(|(_, value)| **value == Type::BytesLiteral(b))
                        {
                            let enum_member =
                                Type::EnumLiteral(EnumLiteralType::new(db, class, name));
                            let can_narrow_to = [Type::BytesLiteral(b), enum_member];
                            EqualityResult::CanNarrow(
                                UnionType::from_elements(db, can_narrow_to)
                                    .negate_if(db, !is_positive),
                            )
                        } else {
                            EqualityResult::Ambiguous
                        }
                    }
                    None => EqualityResult::Ambiguous,
                    Some(_) => EqualityResult::AlwaysUnequal,
                }
            } else if instance.class(db).is_final(db) || instance.tuple_spec(db).is_some() {
                match KnownEqualitySemantics::for_final_instance(
                    db,
                    Type::NominalInstance(instance),
                ) {
                    Some(KnownEqualitySemantics::Bytes) | None => EqualityResult::Ambiguous,
                    Some(_) => EqualityResult::AlwaysUnequal,
                }
            } else if !is_positive {
                EqualityResult::CanNarrow(Type::BytesLiteral(b).negate(db))
            } else {
                EqualityResult::Ambiguous
            }
        }

        (Type::NominalInstance(instance), Type::EnumLiteral(e))
        | (Type::EnumLiteral(e), Type::NominalInstance(instance)) => {
            let class = instance.class(db).class_literal(db);
            if class.is_known(db, KnownClass::Bool)
                && KnownEqualitySemantics::for_final_instance(db, Type::EnumLiteral(e))
                    == Some(KnownEqualitySemantics::Int)
            {
                match e.value(db) {
                    Type::IntLiteral(i @ (0 | 1)) => {
                        let can_narrow_to = [Type::BooleanLiteral(i == 1), Type::EnumLiteral(e)];
                        EqualityResult::CanNarrow(
                            UnionType::from_elements(db, can_narrow_to).negate_if(db, !is_positive),
                        )
                    }
                    Type::IntLiteral(_) => EqualityResult::AlwaysUnequal,
                    _ => EqualityResult::Ambiguous,
                }
            } else if e.enum_class(db) == class
                && KnownEqualitySemantics::for_final_instance(db, Type::NominalInstance(instance))
                    .is_some()
            {
                EqualityResult::CanNarrow(Type::EnumLiteral(e).negate_if(db, !is_positive))
            } else {
                equality_special_case(
                    db,
                    Type::NominalInstance(instance),
                    e.enum_class_instance(db),
                    is_positive,
                )
            }
        }

        // All inhabitants of a `TypedDict` are instances of `dict` at runtime,
        // but there's no point falling back to `KnownClass::Dict.to_instance()` (`dict` is not `@final`!).
        (Type::NominalInstance(_), Type::TypedDict(_))
        | (Type::TypedDict(_), Type::NominalInstance(_)) => EqualityResult::Ambiguous,

        (Type::NominalInstance(l), Type::NominalInstance(r)) => {
            if left.is_singleton(db)
                && KnownEqualitySemantics::for_final_instance(db, left).is_some()
            {
                return if r.class(db).is_final(db)
                    && KnownEqualitySemantics::for_final_instance(db, right).is_some()
                {
                    EqualityResult::from(l == r)
                } else if !is_positive {
                    EqualityResult::CanNarrow(left.negate(db))
                } else {
                    EqualityResult::Ambiguous
                };
            }

            if right.is_singleton(db)
                && KnownEqualitySemantics::for_final_instance(db, right).is_some()
            {
                return if l.class(db).is_final(db)
                    && KnownEqualitySemantics::for_final_instance(db, left).is_some()
                {
                    EqualityResult::from(l == r)
                } else if !is_positive {
                    EqualityResult::CanNarrow(right.negate(db))
                } else {
                    EqualityResult::Ambiguous
                };
            }

            let left_class = l.class(db);
            let right_class = r.class(db);
            if (left_class.is_final(db) || l.tuple_spec(db).is_some())
                && (right_class.is_final(db) || r.tuple_spec(db).is_some())
            {
                let Some(left_equality_semantics) =
                    KnownEqualitySemantics::for_final_instance(db, left)
                else {
                    return EqualityResult::Ambiguous;
                };
                let Some(right_equality_semantics) =
                    KnownEqualitySemantics::for_final_instance(db, right)
                else {
                    return EqualityResult::Ambiguous;
                };
                if (left_equality_semantics != right_equality_semantics
                    || (left_equality_semantics == KnownEqualitySemantics::Object))
                    && left_class.class_literal(db) != right_class.class_literal(db)
                {
                    return EqualityResult::AlwaysUnequal;
                }
            }
            EqualityResult::Ambiguous
        }
    }
}

fn lookup_dunder<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    name: &'static str,
) -> PlaceAndQualifiers<'db> {
    ty.member_lookup_with_policy(
        db,
        Name::new_static(name),
        MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK,
    )
}

fn lookup_dunder_eq<'db>(db: &'db dyn Db, ty: Type<'db>) -> PlaceAndQualifiers<'db> {
    lookup_dunder(db, ty, "__eq__")
}

fn lookup_dunder_ne<'db>(db: &'db dyn Db, ty: Type<'db>) -> PlaceAndQualifiers<'db> {
    lookup_dunder(db, ty, "__ne__")
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum KnownEqualitySemantics {
    Object,
    Int,
    Str,
    Bytes,
    Tuple,
}

impl KnownEqualitySemantics {
    fn for_final_instance<'db>(db: &'db dyn Db, instance: Type<'db>) -> Option<Self> {
        let class = instance.to_meta_type(db);
        let eq = lookup_dunder_eq(db, class);
        let ne = lookup_dunder_ne(db, class);
        if eq.place.is_undefined() && ne.place.is_undefined() {
            return Some(KnownEqualitySemantics::Object);
        }
        let int_class = KnownClass::Int.to_class_literal(db);
        if eq == lookup_dunder_eq(db, int_class) && ne == lookup_dunder_ne(db, int_class) {
            return Some(KnownEqualitySemantics::Int);
        }
        let str_class = KnownClass::Str.to_class_literal(db);
        if eq == lookup_dunder_eq(db, str_class) && ne == lookup_dunder_ne(db, str_class) {
            return Some(KnownEqualitySemantics::Str);
        }
        let bytes_class = KnownClass::Bytes.to_class_literal(db);
        if eq == lookup_dunder_eq(db, bytes_class) && ne == lookup_dunder_ne(db, bytes_class) {
            return Some(KnownEqualitySemantics::Bytes);
        }
        let tuple_class = KnownClass::Tuple.to_class_literal(db);
        if eq == lookup_dunder_eq(db, tuple_class) && ne == lookup_dunder_ne(db, tuple_class) {
            return Some(KnownEqualitySemantics::Tuple);
        }
        None
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) enum EqualityResult<'db> {
    /// The two types always compare equal.
    ///
    /// This does not necessarily indicate anything about whether the two types are
    /// the same type, or even whether they have any subtyping/assignability relationship!
    /// For example, an object of type `Literal[1]` will always compare equal to an object
    /// of type `Literal[Foo.X]` in the following example, despite the fact that `Literal[1]`
    /// is *disjoint* from `Literal[Foo.X]`:
    ///
    /// ```py
    /// from enum import IntEnum
    ///
    /// class Foo(IntEnum):
    ///     X = 1
    /// ```
    AlwaysEqual,

    /// (In)equality between the two types indicates that both sides can be narrowed to the
    /// wrapped type.
    ///
    /// For example, if an object of type `LiteralString` compares equal to an object of type
    /// `Literal["foo"]`, we can safely narrow the type of both operands to `Literal["foo"]`.
    CanNarrow(Type<'db>),

    /// The two types may compare equal or unequal, depending on runtime values.
    Ambiguous,

    /// The two types always compare unequal.
    ///
    /// Similar to [`AlwaysEqual`], this does not necessarily indicate anything about
    /// whether the two types are disjoint!
    AlwaysUnequal,
}

impl From<bool> for EqualityResult<'_> {
    fn from(value: bool) -> Self {
        if value {
            EqualityResult::AlwaysEqual
        } else {
            EqualityResult::AlwaysUnequal
        }
    }
}
