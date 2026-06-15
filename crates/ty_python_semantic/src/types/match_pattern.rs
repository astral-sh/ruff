use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::Truthiness;
use ty_python_core::predicate::{
    ClassPatternKind, PatternPredicateKind, SequencePatternPredicateKind,
};

use crate::Db;
use crate::types::callable::{CallableFunctionProvenance, CallableTypeKind};
use crate::types::signatures::CallableSignature;
use crate::types::tuple::TupleType;
use crate::types::{
    CallableType, IntersectionBuilder, KnownClass, Parameter, Parameters, Signature,
    SpecialFormType, Type, TypeContext, UnionType, equality_truthiness,
    infer_same_file_expression_type,
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

fn pattern_is_exhaustive_for_subject(
    db: &dyn Db,
    pattern: &PatternPredicateKind<'_>,
    subject_ty: Type<'_>,
) -> bool {
    subject_ty.is_subtype_of(
        db,
        definite_match_pattern_type_for_subject(db, pattern, subject_ty),
    )
}

fn sequence_pattern_is_exhaustive_for_subject(
    db: &dyn Db,
    kind: &SequencePatternPredicateKind<'_>,
    subject_ty: Type<'_>,
) -> bool {
    if !subject_ty.is_subtype_of(db, sequence_pattern_type_builder(db).build()) {
        return false;
    }

    if kind.is_irrefutable() {
        return true;
    }

    let Some(tuple) = subject_ty.exact_tuple_instance_spec(db) else {
        return false;
    };
    let Some(tuple) = tuple.as_fixed_length() else {
        return false;
    };
    let elements = tuple.all_elements();

    let Some((prefix, suffix)) = kind.split_around_star() else {
        return elements.len() == kind.patterns.len()
            && elements
                .iter()
                .zip(kind.patterns.iter())
                .all(|(element, pattern)| {
                    pattern_is_exhaustive_for_subject(db, pattern, *element)
                });
    };
    if elements.len() < prefix.len() + suffix.len() {
        return false;
    }

    elements
        .iter()
        .zip(prefix)
        .chain(elements.iter().rev().zip(suffix.iter().rev()))
        .all(|(element, pattern)| pattern_is_exhaustive_for_subject(db, pattern, *element))
}

fn subject_independent_definite_match_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
) -> Option<Type<'db>> {
    match kind {
        PatternPredicateKind::Singleton(_)
        | PatternPredicateKind::Class(_, ClassPatternKind::Irrefutable)
        | PatternPredicateKind::Mapping(ClassPatternKind::Irrefutable) => {
            Some(definite_match_pattern_type(db, kind))
        }
        PatternPredicateKind::Sequence(sequence) if sequence.is_irrefutable() => {
            Some(definite_match_pattern_type(db, kind))
        }
        PatternPredicateKind::Or(patterns) => {
            let patterns = patterns
                .iter()
                .map(|pattern| subject_independent_definite_match_pattern_type(db, pattern))
                .collect::<Option<Vec<_>>>()?;
            Some(UnionType::from_elements(db, patterns))
        }
        PatternPredicateKind::As(Some(pattern), _) => {
            subject_independent_definite_match_pattern_type(db, pattern)
        }
        PatternPredicateKind::As(None, _) | PatternPredicateKind::Star(_) => Some(Type::object()),
        PatternPredicateKind::Value(_)
        | PatternPredicateKind::Class(_, ClassPatternKind::Refutable)
        | PatternPredicateKind::Mapping(ClassPatternKind::Refutable)
        | PatternPredicateKind::Sequence(_) => None,
    }
}

/// Return the values in `subject_ty` that are statically guaranteed to match `kind`.
///
/// Unlike [`definite_match_pattern_type`], this can recognize guarantees that depend on the
/// current subject. For example, both `Literal[True]` and `Literal[1]` are guaranteed to match the
/// value pattern `1` because match value patterns use equality.
pub(crate) fn definite_match_pattern_type_for_subject<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
    subject_ty: Type<'db>,
) -> Type<'db> {
    if let Some(subject_independent_ty) = subject_independent_definite_match_pattern_type(db, kind)
    {
        return subject_independent_ty;
    }

    let resolved_subject_ty = subject_ty.resolve_type_alias(db);
    if let Type::Union(union) = resolved_subject_ty {
        return UnionType::from_elements(
            db,
            union
                .elements(db)
                .iter()
                .map(|element| definite_match_pattern_type_for_subject(db, kind, *element)),
        );
    }

    match kind {
        PatternPredicateKind::Value(value) => {
            let value_ty = infer_same_file_expression_type(db, *value, TypeContext::default());
            if equality_truthiness(db, resolved_subject_ty, value_ty) == Truthiness::AlwaysTrue {
                subject_ty
            } else {
                IntersectionBuilder::new(db)
                    .add_positive(subject_ty)
                    .add_positive(definite_match_pattern_type(db, kind))
                    .build()
            }
        }
        PatternPredicateKind::Sequence(kind) => {
            if sequence_pattern_is_exhaustive_for_subject(db, kind, resolved_subject_ty) {
                subject_ty
            } else {
                IntersectionBuilder::new(db)
                    .add_positive(subject_ty)
                    .add_positive(definite_sequence_pattern_type(db, kind))
                    .build()
            }
        }
        PatternPredicateKind::Or(patterns) => UnionType::from_elements(
            db,
            patterns
                .iter()
                .map(|pattern| definite_match_pattern_type_for_subject(db, pattern, subject_ty)),
        ),
        PatternPredicateKind::As(Some(pattern), _) => {
            definite_match_pattern_type_for_subject(db, pattern, subject_ty)
        }
        PatternPredicateKind::As(None, _) | PatternPredicateKind::Star(_) => subject_ty,
        _ => IntersectionBuilder::new(db)
            .add_positive(subject_ty)
            .add_positive(definite_match_pattern_type(db, kind))
            .build(),
    }
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
            // Only return the type if it's single-valued and guaranteed to match itself.
            // Otherwise, we can't definitively exclude it from subsequent patterns.
            if ty.is_single_valued(db) && equality_truthiness(db, ty, ty) == Truthiness::AlwaysTrue
            {
                ty
            } else {
                Type::Never
            }
        }
        PatternPredicateKind::Class(class_expr, kind) => {
            if kind.is_irrefutable() {
                match infer_same_file_expression_type(db, *class_expr, TypeContext::default()) {
                    Type::ClassLiteral(class) => Type::instance(db, class.top_materialization(db)),
                    Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) => {
                        callable_pattern_type(db)
                    }
                    _ => Type::Never,
                }
            } else {
                Type::Never
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

/// Return the values that are guaranteed to match a sequence pattern.
pub(crate) fn definite_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
) -> Type<'db> {
    if kind.is_irrefutable() {
        return sequence_pattern_type_builder(db).build();
    }

    if let Some((prefix, suffix)) = kind.split_around_star() {
        return Type::tuple(TupleType::mixed(
            db,
            prefix
                .iter()
                .map(|pattern| definite_match_pattern_type(db, pattern)),
            Type::object(),
            suffix
                .iter()
                .map(|pattern| definite_match_pattern_type(db, pattern)),
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
