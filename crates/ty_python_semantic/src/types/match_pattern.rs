use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::Truthiness;
use ty_python_core::predicate::{
    ClassPatternPredicateKind, PatternPredicateKind, SequencePatternPredicateKind,
};

use crate::Db;
use crate::place::{DefinedPlace, Place};
use crate::types::callable::{CallableFunctionProvenance, CallableTypeKind};
use crate::types::signatures::CallableSignature;
use crate::types::tuple::TupleType;
use crate::types::{
    CallableType, ClassBase, ClassLiteral, IntersectionBuilder, KnownClass, Parameter, Parameters,
    Signature, SpecialFormType, Type, TypeContext, UnionType, binding_type, equality_truthiness,
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

/// Return whether every runtime value represented by a `TypedDict` satisfies `class`.
///
/// `TypedDict` is not a nominal subtype of `dict` in the static type system, but every runtime
/// value is a dictionary. A `TypedDict` therefore matches class patterns such as `dict()`,
/// `Mapping()`, and `MutableMapping()`.
pub(crate) fn typed_dict_matches_class_pattern(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
    let Some(dict) = KnownClass::Dict.to_class_literal(db).as_class_literal() else {
        return false;
    };
    Type::instance(db, dict.top_materialization(db))
        .is_subtype_of(db, Type::instance(db, class.top_materialization(db)))
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

/// Return whether every value in `subject_ty` is statically guaranteed to match this class pattern.
///
/// Attribute subpatterns are checked recursively against their statically known member types. A
/// proper non-final subtype remains refutable because it can override attribute access, while a
/// final subtype can supply members absent from the class named in the pattern.
///
/// ```python
/// class Base: ...
///
/// @final
/// class Child(Base):
///     x: int
///
/// # Exhaustive for a `Child` subject because `Child.x` is definitely bound.
/// case Base(x=_): ...
/// ```
fn class_pattern_is_exhaustive(
    db: &dyn Db,
    class: ClassLiteral<'_>,
    subject_ty: Type<'_>,
    kind: &ClassPatternPredicateKind<'_>,
) -> bool {
    let class_instance_ty = Type::instance(db, class.top_materialization(db));
    let is_typed_dict_match =
        matches!(subject_ty, Type::TypedDict(_)) && typed_dict_matches_class_pattern(db, class);
    if !is_typed_dict_match && !subject_ty.is_subtype_of(db, class_instance_ty) {
        return false;
    }

    let is_protocol = class.is_protocol(db);
    if kind.is_argumentless() && !is_protocol {
        return true;
    }

    let is_proper_non_final_subtype = if is_typed_dict_match {
        false
    } else {
        let Some(subject_class) = subject_ty.nominal_class(db) else {
            return false;
        };
        let subject_class_literal = subject_class.class_literal(db);
        subject_class_literal != class && !subject_class_literal.is_final(db)
    };

    // TODO: A non-final subject class also admits subclasses that can override attribute access.
    // Decide whether it should remain exhaustive under the static member model or be treated like
    // a proper non-final subtype.
    if kind.is_argumentless() {
        return !is_proper_non_final_subtype;
    }

    let positional_sources =
        class_pattern_positional_sources(db, Some(class), kind.positional.len());
    let extracts_attribute = !kind.keywords.is_empty()
        || positional_sources
            .iter()
            .any(|source| !matches!(source, ClassPatternPositionalSource::MatchSelf));
    if is_proper_non_final_subtype && (is_protocol || extracts_attribute) {
        return false;
    }

    if !kind.keywords.iter().all(|keyword| {
        member_pattern_is_exhaustive(db, subject_ty, keyword.attr.as_str(), &keyword.pattern)
    }) {
        return false;
    }

    kind.positional
        .iter()
        .zip(positional_sources)
        .all(|(pattern, source)| match source {
            ClassPatternPositionalSource::MatchSelf => {
                pattern_is_exhaustive_for_subject(db, pattern, subject_ty)
            }
            ClassPatternPositionalSource::Attribute(name) => {
                member_pattern_is_exhaustive(db, subject_ty, name.as_str(), pattern)
            }
            ClassPatternPositionalSource::Unknown => false,
        })
}

enum ClassMatchArgs<'db> {
    Undefined,
    Defined(Type<'db>),
    PossiblyUndefined,
}

#[derive(Clone)]
pub(crate) enum ClassPatternPositionalSource {
    MatchSelf,
    Attribute(Name),
    Unknown,
}

/// Resolve `__match_args__` through the pattern class, including its metaclass.
///
/// Inferred assignments retain their literal binding type, while an explicit annotation remains
/// authoritative. `PossiblyUndefined` is distinct from `Undefined` because only a truly absent
/// `__match_args__` enables match-self behavior.
fn class_match_args_type<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> ClassMatchArgs<'db> {
    match Type::ClassLiteral(class).member(db, "__match_args__").place {
        Place::Defined(
            place @ DefinedPlace {
                ty,
                origin,
                provenance,
                ..
            },
        ) if place.is_definitely_defined() => ClassMatchArgs::Defined(if origin.is_declared() {
            ty
        } else {
            provenance
                .definition()
                .map_or(ty, |definition| binding_type(db, definition))
        }),
        Place::Defined(_) => ClassMatchArgs::PossiblyUndefined,
        Place::Undefined => ClassMatchArgs::Undefined,
    }
}

fn class_has_match_self_flag(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
    class
        .iter_mro(db)
        .filter_map(ClassBase::into_class)
        .any(|base| {
            base.class_literal(db)
                .known(db)
                .is_some_and(KnownClass::has_match_self_flag)
        })
}

pub(crate) fn class_pattern_positional_sources(
    db: &dyn Db,
    class: Option<ClassLiteral<'_>>,
    positional_count: usize,
) -> Vec<ClassPatternPositionalSource> {
    let Some(class) = class else {
        return vec![ClassPatternPositionalSource::Unknown; positional_count];
    };

    let fixed = match class_match_args_type(db, class) {
        ClassMatchArgs::Undefined if class_has_match_self_flag(db, class) => {
            return (0..positional_count)
                .map(|index| {
                    if index == 0 {
                        ClassPatternPositionalSource::MatchSelf
                    } else {
                        ClassPatternPositionalSource::Unknown
                    }
                })
                .collect();
        }
        ClassMatchArgs::Defined(match_args) => match_args
            .exact_tuple_instance_spec(db)
            .and_then(|tuple| tuple.as_fixed_length().cloned()),
        ClassMatchArgs::Undefined | ClassMatchArgs::PossiblyUndefined => None,
    };

    (0..positional_count)
        .map(|index| {
            fixed
                .as_ref()
                .and_then(|tuple| tuple.elements_slice().get(index))
                .and_then(|ty| ty.as_string_literal())
                .map(|literal| {
                    ClassPatternPositionalSource::Attribute(Name::new(literal.value(db)))
                })
                .unwrap_or(ClassPatternPositionalSource::Unknown)
        })
        .collect()
}

/// Return whether `name` is definitely bound and `pattern` consumes its entire static member type.
fn member_pattern_is_exhaustive(
    db: &dyn Db,
    instance_ty: Type<'_>,
    name: &str,
    pattern: &PatternPredicateKind<'_>,
) -> bool {
    let place = instance_ty.member(db, name).place;
    place.is_definitely_bound()
        && place
            .raw_type()
            .is_some_and(|member_ty| pattern_is_exhaustive_for_subject(db, pattern, member_ty))
}

/// Return whether `pattern` is statically guaranteed to match every value in `subject_ty`.
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

/// Return whether an exact tuple subject is fully consumed by a sequence pattern.
///
/// Each aligned element is checked with the subject-aware matcher so nested class patterns use the
/// tuple element's actual static type.
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

/// Return the subset of `subject_ty` that is statically guaranteed to match `kind`.
///
/// This is an under-approximation used for negative narrowing and ordered alternatives: callers
/// may subtract the result under ty's static member model. Class patterns need the current subject
/// type because a proper non-final subtype is conservative, while an exact or final subtype can
/// make member extraction exhaustive. This treats access to a definitely bound descriptor as
/// successful even though the descriptor could raise at runtime. The same rule is propagated
/// through nested sequence, `or`, and `as` patterns.
///
/// ```python
/// # For a `tuple[Child]` subject, this evaluates the class pattern against `Child`, not `Base`.
/// case [Base(x=_)]: ...
/// ```
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
        PatternPredicateKind::Class(kind) => {
            let class_ty = infer_same_file_expression_type(db, kind.class, TypeContext::default());
            match class_ty {
                Type::ClassLiteral(class) => {
                    if class_pattern_is_exhaustive(db, class, resolved_subject_ty, kind) {
                        return subject_ty;
                    }
                }
                Type::SpecialForm(SpecialFormType::CollectionsAbcCallable)
                    if kind.is_argumentless()
                        && subject_ty.is_subtype_of(db, callable_pattern_type(db)) =>
                {
                    return callable_pattern_type(db);
                }
                _ => {}
            }
        }
        PatternPredicateKind::Sequence(kind) => {
            return if sequence_pattern_is_exhaustive_for_subject(db, kind, resolved_subject_ty) {
                subject_ty
            } else {
                // A nested subject-dependent pattern rejected the context-free approximation.
                // Reusing that approximation for the surrounding sequence would reintroduce the
                // values that the recursive analysis deliberately excluded.
                Type::Never
            };
        }
        PatternPredicateKind::Or(patterns) => {
            return UnionType::from_elements(
                db,
                patterns.iter().map(|pattern| {
                    definite_match_pattern_type_for_subject(db, pattern, subject_ty)
                }),
            );
        }
        PatternPredicateKind::As(Some(pattern), _) => {
            return definite_match_pattern_type_for_subject(db, pattern, subject_ty);
        }
        _ => return Type::Never,
    }

    let subject_independent_ty = definite_match_pattern_type(db, kind);
    // The subject-aware checks above can reject an otherwise exhaustive-looking pattern. Do not
    // let the less precise fallback reintroduce that conclusion.
    if subject_ty.is_subtype_of(db, subject_independent_ty) {
        return Type::Never;
    }

    IntersectionBuilder::new(db)
        .add_positive(subject_ty)
        .add_positive(subject_independent_ty)
        .build()
}

/// Return the definite-match type when it does not depend on the current subject type.
///
/// `None` means that callers must use subject-aware analysis instead of falling back to the
/// context-free approximation. In particular, protocol and attribute class patterns can be
/// refutable for a proper non-final subtype even when static subtyping says otherwise.
fn subject_independent_definite_match_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
) -> Option<Type<'db>> {
    match kind {
        PatternPredicateKind::Class(kind) => {
            match infer_same_file_expression_type(db, kind.class, TypeContext::default()) {
                Type::ClassLiteral(class)
                    if kind.is_argumentless()
                        && !class.is_protocol(db)
                        && !typed_dict_matches_class_pattern(db, class) =>
                {
                    Some(Type::instance(db, class.top_materialization(db)))
                }
                Type::ClassLiteral(_) => None,
                Type::SpecialForm(SpecialFormType::CollectionsAbcCallable)
                    if kind.is_argumentless() =>
                {
                    Some(callable_pattern_type(db))
                }
                _ => Some(Type::Never),
            }
        }
        PatternPredicateKind::Sequence(kind) => {
            build_definite_sequence_pattern_type(db, kind, |pattern| {
                subject_independent_definite_match_pattern_type(db, pattern)
            })
        }
        PatternPredicateKind::Or(patterns) => patterns
            .iter()
            .map(|pattern| subject_independent_definite_match_pattern_type(db, pattern))
            .collect::<Option<Vec<_>>>()
            .map(|types| UnionType::from_elements(db, types)),
        PatternPredicateKind::As(Some(pattern), _) => {
            subject_independent_definite_match_pattern_type(db, pattern)
        }
        _ => Some(definite_match_pattern_type(db, kind)),
    }
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
            // Only return the type if it's single-valued and guaranteed to match itself.
            // Otherwise, we can't definitively exclude it from subsequent patterns.
            if ty.is_single_valued(db) && equality_truthiness(db, ty, ty) == Truthiness::AlwaysTrue
            {
                ty
            } else {
                Type::Never
            }
        }
        PatternPredicateKind::Class(kind) => {
            match infer_same_file_expression_type(db, kind.class, TypeContext::default()) {
                Type::ClassLiteral(class) if kind.is_argumentless() => {
                    Type::instance(db, class.top_materialization(db))
                }
                Type::SpecialForm(SpecialFormType::CollectionsAbcCallable)
                    if kind.is_argumentless() =>
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

/// Return the values that are guaranteed to match a sequence pattern.
fn definite_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
) -> Type<'db> {
    build_definite_sequence_pattern_type(db, kind, |pattern| {
        Some(definite_match_pattern_type(db, pattern))
    })
    .unwrap_or(Type::Never)
}

fn build_definite_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
    mut element_type: impl FnMut(&PatternPredicateKind<'db>) -> Option<Type<'db>>,
) -> Option<Type<'db>> {
    if kind.is_irrefutable() {
        return Some(sequence_pattern_type_builder(db).build());
    }

    if let Some((prefix, suffix)) = kind.split_around_star() {
        let prefix_types = prefix
            .iter()
            .map(&mut element_type)
            .collect::<Option<Vec<_>>>()?;
        let suffix_types = suffix
            .iter()
            .map(&mut element_type)
            .collect::<Option<Vec<_>>>()?;
        return Some(Type::tuple(TupleType::mixed(
            db,
            prefix_types,
            Type::object(),
            suffix_types,
        )));
    }

    let element_types: Vec<_> = kind
        .patterns
        .iter()
        .map(element_type)
        .collect::<Option<_>>()?;

    if element_types.iter().any(Type::is_never) {
        Some(Type::Never)
    } else {
        Some(exact_sequence_pattern_type(db, element_types.into_iter()))
    }
}
