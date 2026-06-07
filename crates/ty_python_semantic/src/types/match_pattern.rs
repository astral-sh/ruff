use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::predicate::{
    ClassPatternKind, PatternPredicateKind, SequencePatternPredicateKind,
};

use crate::Db;
use crate::types::callable::{CallableFunctionProvenance, CallableTypeKind};
use crate::types::signatures::CallableSignature;
use crate::types::tuple::TupleType;
use crate::types::{
    CallableType, ClassBase, ClassLiteral, IntersectionBuilder, KnownClass, Parameter, Parameters,
    Signature, SpecialFormType, Type, TypeContext, UnionType, infer_same_file_expression_type,
};

pub(crate) fn singleton_pattern_type(db: &dyn Db, singleton: ast::Singleton) -> Type<'_> {
    let ty = match singleton {
        ast::Singleton::None => Type::none(db),
        ast::Singleton::True => Type::bool_literal(true),
        ast::Singleton::False => Type::bool_literal(false),
    };
    debug_assert!(ty.is_singleton(db));
    ty
}

pub(crate) fn mapping_pattern_type(db: &dyn Db) -> Type<'_> {
    KnownClass::Mapping.to_instance(db).top_materialization(db)
}

pub(crate) fn callable_pattern_type(db: &dyn Db) -> Type<'_> {
    Type::Callable(CallableType::unknown(db)).top_materialization(db)
}

pub(crate) fn sequence_pattern_type_builder(db: &dyn Db) -> IntersectionBuilder<'_> {
    IntersectionBuilder::new(db)
        .add_positive(KnownClass::Sequence.to_instance(db).top_materialization(db))
        // `str`, `bytes`, and `bytearray` are sequences, but Python sequence
        // patterns explicitly do not match them or their subclasses.
        .add_negative(KnownClass::Str.to_instance(db))
        .add_negative(KnownClass::Bytes.to_instance(db))
        .add_negative(KnownClass::Bytearray.to_instance(db))
}

pub(crate) fn class_pattern_is_irrefutable(
    db: &dyn Db,
    class: ClassLiteral<'_>,
    kind: ClassPatternKind,
) -> bool {
    match kind {
        ClassPatternKind::Irrefutable => true,
        ClassPatternKind::SinglePositionalIrrefutable => {
            class_match_args_are_undefined(db, class) && class_uses_match_self(db, class)
        }
        ClassPatternKind::IrrefutableArguments | ClassPatternKind::Refutable => false,
    }
}

pub(crate) fn class_pattern_is_exhaustive(
    db: &dyn Db,
    class: ClassLiteral<'_>,
    kind: ClassPatternKind,
) -> bool {
    match kind {
        ClassPatternKind::Irrefutable | ClassPatternKind::IrrefutableArguments => true,
        ClassPatternKind::SinglePositionalIrrefutable => {
            if class_match_args_are_undefined(db, class) {
                class_uses_match_self(db, class)
            } else {
                !class_uses_match_self(db, class)
            }
        }
        ClassPatternKind::Refutable => false,
    }
}

fn class_match_args_are_undefined(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
    Type::ClassLiteral(class)
        .member(db, "__match_args__")
        .place
        .is_undefined()
}

fn class_uses_match_self(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
    class
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .any(|base| {
            matches!(
                base.class_literal(db).known(db),
                Some(
                    KnownClass::Bool
                        | KnownClass::Bytearray
                        | KnownClass::Bytes
                        | KnownClass::Dict
                        | KnownClass::Float
                        | KnownClass::FrozenSet
                        | KnownClass::Int
                        | KnownClass::List
                        | KnownClass::Set
                        | KnownClass::Str
                        | KnownClass::Tuple
                )
            )
        })
}

fn sequence_pattern_getitem_method<'db>(
    db: &'db dyn Db,
    indexed_element_types: impl IntoIterator<Item = (i64, Type<'db>)>,
    fallback_return_type: Option<Type<'db>>,
) -> CallableType<'db> {
    let self_parameter = || Parameter::positional_only(Some(Name::new_static("self")));

    let overloads = indexed_element_types
        .into_iter()
        .map(|(index, element_type)| {
            Signature::new(
                Parameters::new(
                    db,
                    [
                        self_parameter(),
                        Parameter::positional_only(Some(Name::new_static("index")))
                            .with_annotated_type(Type::int_literal(index)),
                    ],
                ),
                element_type,
            )
        });
    let fallback_overload = fallback_return_type.map(|fallback_return_type| {
        Signature::new(
            Parameters::new(
                db,
                [
                    self_parameter(),
                    Parameter::positional_only(Some(Name::new_static("index")))
                        .with_annotated_type(KnownClass::Int.to_instance(db)),
                ],
            ),
            fallback_return_type,
        )
    });

    CallableType::new(
        db,
        CallableSignature::from_overloads(overloads.chain(fallback_overload)),
        CallableTypeKind::FunctionLike,
        CallableFunctionProvenance::None,
    )
}

/// Build the structural type used for a fixed-length sequence pattern.
///
/// For a pattern like:
///
/// ```python
/// match value:
///     case [int(), str()]:
///         ...
/// ```
///
/// this returns the sequence-pattern runtime type plus a synthesized protocol
/// whose `__len__` and indexed `__getitem__` methods encode the fixed length
/// and element types.
pub(crate) fn exact_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    element_types: impl ExactSizeIterator<Item = Type<'db>>,
) -> Type<'db> {
    let Ok(length) = i64::try_from(element_types.len()) else {
        return sequence_pattern_type_builder(db).build();
    };

    // `False == 0` and `True == 1`, so the protocol must accept both literals.
    let length_type = match length {
        0 => UnionType::from_two_elements(db, Type::int_literal(0), Type::bool_literal(false)),
        1 => UnionType::from_two_elements(db, Type::int_literal(1), Type::bool_literal(true)),
        _ => Type::int_literal(length),
    };

    let self_parameter = || Parameter::positional_only(Some(Name::new_static("self")));

    let len_signature = Signature::new(Parameters::new(db, [self_parameter()]), length_type);
    let len_method = CallableType::function_like(db, len_signature);

    let getitem_method = (element_types.len() > 0).then(|| {
        (
            "__getitem__",
            sequence_pattern_getitem_method(db, (0..length).zip(element_types), None),
        )
    });

    let protocol = Type::protocol_with_methods(
        db,
        std::iter::once(("__len__", len_method)).chain(getitem_method),
    );

    sequence_pattern_type_builder(db)
        .add_positive(protocol)
        .build()
}

/// Build the structural type used for a sequence pattern containing `*rest`.
///
/// Fixed prefix elements use non-negative indices and fixed suffix elements use
/// negative indices. Other integer indices retain the sequence's element type.
pub(crate) fn starred_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    prefix_element_types: impl ExactSizeIterator<Item = Type<'db>>,
    suffix_element_types: impl ExactSizeIterator<Item = Type<'db>>,
) -> Type<'db> {
    if prefix_element_types.len() == 0 && suffix_element_types.len() == 0 {
        return sequence_pattern_type_builder(db).build();
    }

    let Ok(suffix_length) = i64::try_from(suffix_element_types.len()) else {
        return sequence_pattern_type_builder(db).build();
    };

    let indexed_element_types = (0_i64..)
        .zip(prefix_element_types)
        .chain((-suffix_length..0).zip(suffix_element_types));
    let getitem_method =
        sequence_pattern_getitem_method(db, indexed_element_types, Some(Type::object()));
    let protocol = Type::protocol_with_methods(db, [("__getitem__", getitem_method)]);

    sequence_pattern_type_builder(db)
        .add_positive(protocol)
        .build()
}

/// Return the values that are guaranteed to match `kind`.
///
/// Reachability and negative narrowing can only subtract this under-approximation.
pub(crate) fn definite_match_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
) -> Type<'db> {
    match kind {
        PatternPredicateKind::Singleton(singleton) => singleton_pattern_type(db, *singleton),
        PatternPredicateKind::Value(value) => {
            let ty = infer_same_file_expression_type(db, *value, TypeContext::default());
            // Only return the type if it's single-valued and equality is guaranteed to be
            // reflexive. Otherwise, we can't definitively exclude it from subsequent patterns.
            if ty.is_single_valued(db) && !ty.equality_may_not_be_reflexive(db) {
                ty
            } else {
                Type::Never
            }
        }
        PatternPredicateKind::Class(class_expr, kind) => {
            match infer_same_file_expression_type(db, *class_expr, TypeContext::default()) {
                Type::ClassLiteral(class) if class_pattern_is_irrefutable(db, class, *kind) => {
                    Type::instance(db, class.top_materialization(db))
                }
                Type::SpecialForm(SpecialFormType::CollectionsAbcCallable)
                    if kind.is_irrefutable() =>
                {
                    callable_pattern_type(db)
                }
                _ => Type::Never,
            }
        }
        PatternPredicateKind::Mapping(kind) => {
            if kind.is_irrefutable() {
                mapping_pattern_type(db)
            } else {
                Type::Never
            }
        }
        PatternPredicateKind::Sequence(kind) => definite_sequence_pattern_type(db, kind),
        PatternPredicateKind::Or(predicates) => UnionType::from_elements(
            db,
            predicates
                .iter()
                .map(|p| definite_match_pattern_type(db, p)),
        ),
        PatternPredicateKind::As(pattern, _) => pattern
            .as_deref()
            .map(|p| definite_match_pattern_type(db, p))
            .unwrap_or_else(Type::object),
        PatternPredicateKind::Star(_) => Type::object(),
    }
}

/// Return the values that exhaustiveness checking treats as consumed by `kind`.
///
/// Direct class patterns with irrefutable subpatterns are exhaustive for a known class, even
/// though attribute lookup can fail at runtime. Ordered alternatives and nested sequence patterns
/// use [`definite_match_pattern_type`] instead so they do not discard a later matching alternative.
pub(crate) fn exhaustive_match_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
) -> Type<'db> {
    match kind {
        PatternPredicateKind::Class(class_expr, kind) => {
            let Some(class) =
                infer_same_file_expression_type(db, *class_expr, TypeContext::default())
                    .as_class_literal()
            else {
                return Type::Never;
            };

            if class_pattern_is_exhaustive(db, class, *kind) {
                Type::instance(db, class.top_materialization(db))
            } else {
                Type::Never
            }
        }
        PatternPredicateKind::Or(predicates) => UnionType::from_elements(
            db,
            predicates
                .iter()
                .map(|predicate| exhaustive_match_pattern_type(db, predicate)),
        ),
        PatternPredicateKind::As(pattern, _) => pattern
            .as_deref()
            .map(|pattern| exhaustive_match_pattern_type(db, pattern))
            .unwrap_or_else(Type::object),
        _ => definite_match_pattern_type(db, kind),
    }
}

/// Return the values that are guaranteed to match a sequence pattern.
pub(crate) fn definite_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
) -> Type<'db> {
    if kind.is_irrefutable() {
        return sequence_pattern_type_builder(db).build();
    }

    if let Some((prefix, suffix)) = kind.split_around_star() {
        let prefix_types: Vec<_> = prefix
            .iter()
            .map(|pattern| definite_match_pattern_type(db, pattern))
            .collect();
        let suffix_types: Vec<_> = suffix
            .iter()
            .map(|pattern| definite_match_pattern_type(db, pattern))
            .collect();

        if prefix_types.iter().chain(&suffix_types).any(Type::is_never) {
            return Type::Never;
        }

        return Type::tuple(TupleType::mixed(
            db,
            prefix_types,
            Type::object(),
            suffix_types,
        ));
    }

    let element_types: Vec<_> = kind
        .patterns
        .iter()
        .map(|pattern| definite_match_pattern_type(db, pattern))
        .collect();

    if element_types.iter().any(Type::is_never) {
        Type::Never
    } else {
        exact_sequence_pattern_type(db, element_types.into_iter())
    }
}
