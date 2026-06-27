use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::Truthiness;
use ty_python_core::predicate::{
    ClassPatternPredicateKind, MappingPatternPredicateKind, PatternPredicateKind,
    SequencePatternPredicateKind,
};

use crate::Db;
use crate::place::{DefinedPlace, Place};
use crate::types::callable::{CallableFunctionProvenance, CallableTypeKind};
use crate::types::equality::{evaluate_type_equality, is_same_enum_domain};
use crate::types::signatures::CallableSignature;
use crate::types::tuple::TupleType;
use crate::types::visitor::any_over_type;
use crate::types::{
    CallableType, ClassBase, ClassLiteral, EnumLiteralType, IntersectionBuilder, KnownClass,
    Parameter, Parameters, Signature, SpecialFormType, Type, TypeContext,
    TypeVarBoundOrConstraints, TypedDictType, UnionType, binding_type, equality_truthiness,
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

/// Return whether every value in `ty` belongs to a `TypedDict` domain accepted by `predicate`.
fn typed_dict_pattern_domain_satisfies<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    predicate: &impl Fn(TypedDictType<'db>) -> bool,
) -> bool {
    match ty.resolve_type_alias(db) {
        Type::TypedDict(typed_dict) => predicate(typed_dict),
        Type::TypeVar(typevar) => match typevar.typevar(db).bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                typed_dict_pattern_domain_satisfies(db, bound, predicate)
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => constraints
                .elements(db)
                .iter()
                .all(|constraint| typed_dict_pattern_domain_satisfies(db, *constraint, predicate)),
            None => false,
        },
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| typed_dict_pattern_domain_satisfies(db, *element, predicate)),
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|element| typed_dict_pattern_domain_satisfies(db, *element, predicate)),
        _ => false,
    }
}

/// Return whether every value in `ty` is represented by a `TypedDict` schema at runtime.
fn is_typed_dict_pattern_domain(db: &dyn Db, ty: Type<'_>) -> bool {
    typed_dict_pattern_domain_satisfies(db, ty, &|_| true)
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

/// Return whether every value in `subject_ty` is statically guaranteed to match this class pattern.
///
/// Attribute subpatterns are checked recursively against their statically known member types. The
/// subject's class can provide members that are absent from the class named in the pattern.
///
/// ```python
/// class Base: ...
///
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
        is_typed_dict_pattern_domain(db, subject_ty) && typed_dict_matches_class_pattern(db, class);
    if !is_typed_dict_match && !subject_ty.is_subtype_of(db, class_instance_ty) {
        return false;
    }

    if kind.is_empty() {
        return true;
    }

    if !kind.keywords.iter().all(|keyword| {
        member_pattern_is_exhaustive(db, subject_ty, keyword.attr.as_str(), &keyword.pattern)
    }) {
        return false;
    }

    let positional_sources = class_pattern_positional_sources(db, class, kind.positional.len());
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

/// The static lookup result for a pattern class's `__match_args__` member.
///
/// `PossiblyUndefined` is distinct from `Undefined` because a conditional definition can take
/// precedence over match-self behavior at runtime, so that behavior cannot be assumed.
enum ClassMatchArgs<'db> {
    /// The class and its metaclass hierarchy do not define `__match_args__`.
    Undefined,
    /// `__match_args__` is definitely bound.
    Defined {
        /// The semantic member type used to validate positional subpatterns.
        member_type: Type<'db>,
        /// The type used to resolve positional attribute names.
        positional_source_type: Type<'db>,
    },
    /// `__match_args__` is defined along some control-flow paths but not others.
    PossiblyUndefined,
}

/// The value supplied to one positional subpattern in a class pattern.
#[derive(Clone)]
pub(crate) enum ClassPatternPositionalSource {
    /// The complete subject, as used by Python's special built-in class patterns.
    MatchSelf,
    /// The named attribute extracted from the subject according to `__match_args__`.
    Attribute(Name),
    /// No value source can be determined statically, so the subpattern is not exhaustive.
    Unknown,
}

/// Resolve `__match_args__` through the pattern class, including its metaclass.
///
/// The semantic member type is used for validation. When mapping positional subpatterns to
/// attributes, inferred assignments retain their literal binding type while an explicit annotation
/// remains authoritative. `PossiblyUndefined` is distinct from `Undefined` because only a truly
/// absent `__match_args__` enables match-self behavior.
fn class_match_args_type<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> ClassMatchArgs<'db> {
    match Type::ClassLiteral(class).member(db, "__match_args__").place {
        Place::Defined(
            place @ DefinedPlace {
                ty,
                origin,
                provenance,
                ..
            },
        ) if place.is_definitely_defined() => ClassMatchArgs::Defined {
            member_type: ty,
            positional_source_type: if origin.is_declared() {
                ty
            } else {
                provenance
                    .definition()
                    .map_or(ty, |definition| binding_type(db, definition))
            },
        },
        Place::Defined(_) => ClassMatchArgs::PossiblyUndefined,
        Place::Undefined => ClassMatchArgs::Undefined,
    }
}

/// Return whether `class` inherits Python's special match-self class-pattern behavior.
///
/// Callers must first establish that `__match_args__` is statically absent. A definite definition
/// overrides match-self behavior, while a conditional definition makes it runtime-dependent;
/// neither case should consult this flag.
fn class_has_match_self_flag(db: &dyn Db, class: ClassLiteral<'_>) -> bool {
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

/// The statically known result of validating positional subpatterns for a class pattern.
pub(crate) enum ClassPatternPositionalResult<'db> {
    /// The maximum number of positional subpatterns accepted by the class.
    Limit(usize),
    /// A statically known non-tuple value used for `__match_args__`.
    InvalidType(Type<'db>),
}

/// Validate positional subpatterns against a statically known `__match_args__` type.
pub(crate) fn class_pattern_positional_result<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
) -> Option<ClassPatternPositionalResult<'db>> {
    match class_match_args_type(db, class) {
        ClassMatchArgs::Undefined if class_has_match_self_flag(db, class) => {
            Some(ClassPatternPositionalResult::Limit(1))
        }
        ClassMatchArgs::Undefined
            if class.known(db).is_some()
                || class
                    .as_static()
                    .is_some_and(|class| !class.body_scope(db).file(db).is_stub(db)) =>
        {
            Some(ClassPatternPositionalResult::Limit(0))
        }
        ClassMatchArgs::Defined { member_type, .. } => {
            let match_args = member_type.resolve_type_alias(db);
            if let Some(limit) = match_args.exact_tuple_instance_spec(db).and_then(|tuple| {
                tuple
                    .as_fixed_length()
                    .map(super::tuple::FixedLengthTuple::len)
            }) {
                Some(ClassPatternPositionalResult::Limit(limit))
            } else {
                match_args
                    .is_disjoint_from(db, Type::homogeneous_tuple(db, Type::unknown()))
                    .then_some(ClassPatternPositionalResult::InvalidType(match_args))
            }
        }
        ClassMatchArgs::Undefined | ClassMatchArgs::PossiblyUndefined => None,
    }
}

/// Resolve the value supplied to each positional subpattern, preserving source order and length.
///
/// A fixed tuple of string literals in `__match_args__` maps positions to subject attributes.
/// Python's special built-in classes map only the first position to the complete subject. Missing,
/// extra, widened, or conditionally defined positions resolve to
/// [`ClassPatternPositionalSource::Unknown`].
///
/// ```python
/// class Point:
///     __match_args__ = ("x",)
///     x: int
///
/// match point:
///     case Point(_):  # The positional subpattern receives `point.x`.
///         pass
///
/// match number:
///     case int(_):  # The positional subpattern receives `number` itself.
///         pass
/// ```
pub(crate) fn class_pattern_positional_sources(
    db: &dyn Db,
    class: ClassLiteral<'_>,
    positional_count: usize,
) -> Vec<ClassPatternPositionalSource> {
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
        ClassMatchArgs::Defined {
            positional_source_type,
            ..
        } => positional_source_type
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

/// Return whether every value in `subject_ty` is guaranteed to match a mapping pattern.
///
/// A nonempty mapping pattern is exhaustive for a `TypedDict` only when every key names a required
/// field and every nested pattern exhausts that field's declared type. Other mapping types do not
/// guarantee that a particular key is present.
fn mapping_pattern_is_exhaustive(
    db: &dyn Db,
    kind: &MappingPatternPredicateKind<'_>,
    subject_ty: Type<'_>,
) -> bool {
    typed_dict_pattern_domain_satisfies(db, subject_ty, &|typed_dict| {
        kind.entries.iter().all(|entry| {
            let key_ty = infer_same_file_expression_type(db, entry.key, TypeContext::default());
            let Some(key) = key_ty.as_string_literal() else {
                return false;
            };
            typed_dict.item(db, key.value(db)).is_some_and(|field| {
                field.is_required()
                    && pattern_is_exhaustive_for_subject(db, &entry.pattern, field.declared_ty)
            })
        })
    })
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

/// Return the values that are statically guaranteed to match `kind`, using `subject_ty` when the
/// answer depends on the subject.
///
/// This is an under-approximation used for negative narrowing and ordered alternatives: callers
/// may subtract the result from `subject_ty`. A subject-independent pattern can return a type wider
/// than `subject_ty`; for example, `case Base()` returns `Base` even for a `Child` subject. Class
/// patterns need the current subject type when member extraction depends on the subject's statically
/// known members.
/// A subject-independent pattern can return its context-free definite-match type directly. A
/// mapping pattern can use required `TypedDict` fields to establish that every subject value
/// contains its keys.
/// This treats access to a definitely bound descriptor as successful even though the descriptor
/// could raise at runtime. The same rule is propagated through nested sequence, `or`, and `as`
/// patterns.
///
/// ```python
/// class Base:
///     x: int
///
/// class Child(Base):
///     pass
///
/// # For a `tuple[Child]` subject, this checks `x` on `Child`, not only on `Base`.
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
        PatternPredicateKind::Value(value) => {
            let value_ty = infer_same_file_expression_type(db, *value, TypeContext::default());
            if equality_truthiness(db, resolved_subject_ty, value_ty) == Truthiness::AlwaysTrue {
                return subject_ty;
            }
        }
        PatternPredicateKind::Class(kind) => {
            let class_ty = infer_same_file_expression_type(db, kind.class, TypeContext::default());
            match class_ty {
                Type::ClassLiteral(class) => {
                    if class_pattern_is_exhaustive(db, class, resolved_subject_ty, kind) {
                        return subject_ty;
                    }
                }
                Type::SpecialForm(SpecialFormType::CollectionsAbcCallable)
                    if kind.is_empty()
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
        PatternPredicateKind::Mapping(kind) => {
            return if mapping_pattern_is_exhaustive(db, kind, resolved_subject_ty) {
                subject_ty
            } else {
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

    IntersectionBuilder::new(db)
        .add_positive(subject_ty)
        .add_positive(definite_match_pattern_type(db, kind))
        .build()
}

/// Return the part of `subject_ty` that can reach a later alternative after `kind` fails.
///
/// Value patterns use Python equality. Reusing the equality constraint here both accounts for
/// cross-type equal values and avoids checking every member of a large literal union separately:
///
/// ```python
/// from typing import Literal
///
/// def f(value: Literal[True, 1, 2]) -> None:
///     match value:
///         case 1:
///             pass
///         case other:
///             reveal_type(other)  # Literal[2]
/// ```
pub(crate) fn pattern_fallthrough_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
    subject_ty: Type<'db>,
) -> Type<'db> {
    if let PatternPredicateKind::Value(value) = kind {
        let value_ty = infer_same_file_expression_type(db, *value, TypeContext::default());
        // A subject confined to the same enum cannot contain cross-type values that compare equal
        // to the pattern, so direct subtraction avoids repeated equality evaluation in large enum
        // matches. This includes narrowed intersections containing `Self` or another type variable
        // whose upper bound is that enum.
        if let Some(enum_literal) = value_ty.as_enum_literal()
            && is_same_enum_pattern_domain(db, subject_ty, enum_literal)
            && equality_truthiness(db, value_ty, value_ty) == Truthiness::AlwaysTrue
        {
            return IntersectionBuilder::new(db)
                .add_positive(subject_ty)
                .add_negative(value_ty)
                .build();
        }
        if let Some(constraint) = evaluate_type_equality(db, subject_ty, value_ty, false) {
            return IntersectionBuilder::new(db)
                .add_positive(subject_ty)
                .add_positive(constraint)
                .build();
        }
    }

    IntersectionBuilder::new(db)
        .add_positive(subject_ty)
        .add_negative(definite_match_pattern_type_for_subject(
            db, kind, subject_ty,
        ))
        .build()
}

/// Return the fallthrough type for a binding that can reach a later match case.
///
/// Failure of a sequence pattern establishes length and indexed-element facts at the instant of
/// matching, but those facts can become stale for mutable or stateful sequences. Exact tuples are
/// immutable, so they retain normal sequence-pattern fallthrough narrowing.
///
/// ```python
/// def f(value: tuple[int | str, int | str]) -> None:
///     match value:
///         case [int(), str()]:
///             pass
///         case _:
///             # tuple[str, int | str] | tuple[int | str, int]
///             reveal_type(value)
/// ```
pub(crate) fn pattern_binding_fallthrough_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
    subject_ty: Type<'db>,
) -> Type<'db> {
    let mut budget = ExactTuplePatternExpansionBudget::default();
    try_pattern_binding_fallthrough_type(db, kind, subject_ty, &mut budget)
        .unwrap_or_else(|()| conservative_pattern_binding_fallthrough_type(db, kind, subject_ty))
}

/// Compute binding fallthrough while charging every nested exact-tuple expansion to `budget`.
///
/// An error means that the caller must discard the partially expanded type and recompute the
/// complete pattern conservatively.
fn try_pattern_binding_fallthrough_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
    subject_ty: Type<'db>,
    budget: &mut ExactTuplePatternExpansionBudget,
) -> Result<Type<'db>, ()> {
    match kind {
        PatternPredicateKind::Sequence(sequence) => {
            try_sequence_pattern_binding_fallthrough_type(db, sequence, subject_ty, budget)
        }
        PatternPredicateKind::Or(patterns) => {
            patterns.iter().try_fold(subject_ty, |remaining, pattern| {
                try_pattern_binding_fallthrough_type(db, pattern, remaining, budget)
            })
        }
        PatternPredicateKind::As(Some(pattern), _) => {
            try_pattern_binding_fallthrough_type(db, pattern, subject_ty, budget)
        }
        _ => Ok(pattern_fallthrough_type(db, kind, subject_ty)),
    }
}

/// Compute binding fallthrough without expanding exact tuples.
///
/// This preserves the recursive handling of `Or` and `As` patterns while providing the fallback
/// used when the precise traversal exceeds its expansion budget.
fn conservative_pattern_binding_fallthrough_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
    subject_ty: Type<'db>,
) -> Type<'db> {
    match kind {
        PatternPredicateKind::Or(patterns) => {
            patterns.iter().fold(subject_ty, |remaining, pattern| {
                conservative_pattern_binding_fallthrough_type(db, pattern, remaining)
            })
        }
        PatternPredicateKind::As(Some(pattern), _) => {
            conservative_pattern_binding_fallthrough_type(db, pattern, subject_ty)
        }
        _ => pattern_fallthrough_type(db, kind, subject_ty),
    }
}

/// Apply sequence-pattern binding fallthrough, expanding immutable exact tuples within `budget`.
///
/// The budget is shared by unions, intersections, and nested patterns so that their cumulative
/// expansion cannot exceed the configured limits.
fn try_sequence_pattern_binding_fallthrough_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
    subject_ty: Type<'db>,
    budget: &mut ExactTuplePatternExpansionBudget,
) -> Result<Type<'db>, ()> {
    let resolved = subject_ty.resolve_type_alias(db);
    let narrowed = match resolved {
        Type::Union(union) => union
            .try_map(db, |element| {
                try_sequence_pattern_binding_fallthrough_type(db, kind, *element, budget).ok()
            })
            .ok_or(())?,
        Type::Intersection(intersection) => {
            let mut failed = false;
            let narrowed = intersection.map_positive(db, |element| {
                try_sequence_pattern_binding_fallthrough_type(db, kind, *element, budget)
                    .unwrap_or_else(|()| {
                        failed = true;
                        *element
                    })
            });
            if failed {
                return Err(());
            }
            narrowed
        }
        Type::TypeVar(typevar)
            if typevar.typevar(db).upper_bound(db).is_some_and(|bound| {
                pattern_fallthrough_type(db, &PatternPredicateKind::Sequence(kind.clone()), bound)
                    .is_never()
            }) =>
        {
            Type::Never
        }
        _ if resolved.exact_tuple_instance_spec(db).is_some() => {
            exact_tuple_sequence_pattern_fallthrough_type(db, kind, resolved, budget)?
                .unwrap_or_else(|| {
                    pattern_fallthrough_type(
                        db,
                        &PatternPredicateKind::Sequence(kind.clone()),
                        resolved,
                    )
                })
        }
        // An irrefutable sequence pattern can only fail if the subject is not eligible for sequence
        // matching. Unlike length and indexed-element facts, eligibility is unaffected by mutation.
        _ if kind.is_irrefutable() => IntersectionBuilder::new(db)
            .add_positive(resolved)
            .add_negative(sequence_pattern_type_builder(db).build())
            .build(),
        _ => resolved,
    };

    if narrowed == resolved {
        Ok(subject_ty)
    } else {
        Ok(narrowed)
    }
}

const MAX_EXACT_TUPLE_PATTERN_ALTERNATIVES: usize = 64;
const MAX_EXACT_TUPLE_PATTERN_ELEMENTS: usize = 4_096;

/// Limits the cumulative alternatives and element slots created by one pattern traversal.
#[derive(Default)]
struct ExactTuplePatternExpansionBudget {
    alternatives: usize,
    elements: usize,
}

impl ExactTuplePatternExpansionBudget {
    fn add_alternative(&mut self, elements: usize) -> Result<(), ()> {
        self.alternatives += 1;
        self.elements = self.elements.saturating_add(elements);
        if self.alternatives > MAX_EXACT_TUPLE_PATTERN_ALTERNATIVES
            || self.elements > MAX_EXACT_TUPLE_PATTERN_ELEMENTS
        {
            Err(())
        } else {
            Ok(())
        }
    }
}

/// Return the part of an exact fixed-length tuple that can remain after a sequence pattern fails.
///
/// A pattern fails if any aligned element pattern fails. Represent that as a union with one tuple
/// alternative per element. Large expansions and gradual tuples keep the synthesized-protocol
/// representation used by the general fallthrough path.
fn exact_tuple_sequence_pattern_fallthrough_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
    subject_ty: Type<'db>,
    budget: &mut ExactTuplePatternExpansionBudget,
) -> Result<Option<Type<'db>>, ()> {
    if kind.split_around_star().is_some() {
        return Ok(None);
    }

    let Some(tuple) = subject_ty.exact_tuple_instance_spec(db) else {
        return Ok(None);
    };
    let Some(tuple) = tuple.as_fixed_length() else {
        return Ok(None);
    };
    if tuple
        .all_elements()
        .iter()
        .any(|element| any_over_type(db, *element, true, |ty| ty.is_dynamic()))
    {
        return Ok(None);
    }
    if tuple.len() != kind.patterns.len() {
        return Ok(Some(subject_ty));
    }
    let mut alternatives = Vec::new();
    for (index, (element, pattern)) in tuple
        .iter_all_elements()
        .zip(kind.patterns.iter())
        .enumerate()
    {
        let remaining = try_pattern_binding_fallthrough_type(db, pattern, element, budget)?;
        if remaining == element {
            return Ok(Some(subject_ty));
        }
        if remaining.is_never() {
            continue;
        }

        budget.add_alternative(tuple.len())?;
        let mut elements = tuple.all_elements().to_vec();
        elements[index] = remaining;
        alternatives.push(Type::heterogeneous_tuple(db, elements));
    }

    Ok(Some(UnionType::from_elements(db, alternatives)))
}

/// Return whether every possible value of `ty` belongs to the same enum as `right`, including
/// bounded type variables nested inside unions or intersections.
fn is_same_enum_pattern_domain<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
    right: EnumLiteralType<'db>,
) -> bool {
    if is_same_enum_domain(db, ty, right) {
        return true;
    }

    match ty.resolve_type_alias(db) {
        Type::TypeVar(typevar) => typevar
            .typevar(db)
            .upper_bound(db)
            .is_some_and(|bound| is_same_enum_domain(db, bound, right)),
        Type::Union(union) => union
            .elements(db)
            .iter()
            .all(|element| is_same_enum_pattern_domain(db, *element, right)),
        Type::Intersection(intersection) => intersection
            .positive(db)
            .iter()
            .any(|element| is_same_enum_pattern_domain(db, *element, right)),
        _ => false,
    }
}

/// Return the definite-match type when it does not depend on the current subject type.
///
/// `None` means that callers must use subject-aware analysis instead of falling back to the
/// context-free approximation. In particular, attribute class patterns can depend on members of
/// the static subject type.
fn subject_independent_definite_match_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &PatternPredicateKind<'db>,
) -> Option<Type<'db>> {
    match kind {
        PatternPredicateKind::Class(kind) => {
            match infer_same_file_expression_type(db, kind.class, TypeContext::default()) {
                Type::ClassLiteral(class) if kind.is_empty() => {
                    let class_instance_ty = Type::instance(db, class.top_materialization(db));
                    let typed_dict_adds_runtime_matches =
                        typed_dict_matches_class_pattern(db, class)
                            && !Type::object().is_subtype_of(db, class_instance_ty);
                    (!typed_dict_adds_runtime_matches).then_some(class_instance_ty)
                }
                Type::ClassLiteral(_) => None,
                Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) if kind.is_empty() => {
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
        PatternPredicateKind::Mapping(kind) => {
            if kind.is_irrefutable() {
                Some(mapping_pattern_type(db))
            } else {
                None
            }
        }
        PatternPredicateKind::Or(patterns) => patterns
            .iter()
            .map(|pattern| subject_independent_definite_match_pattern_type(db, pattern))
            .collect::<Option<Vec<_>>>()
            .map(|types| UnionType::from_elements(db, types)),
        PatternPredicateKind::As(Some(pattern), _) => {
            subject_independent_definite_match_pattern_type(db, pattern)
        }
        PatternPredicateKind::Value(_) => None,
        _ => Some(definite_match_pattern_type(db, kind)),
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
        PatternPredicateKind::Class(kind) => {
            match infer_same_file_expression_type(db, kind.class, TypeContext::default()) {
                Type::ClassLiteral(class) if kind.is_empty() => {
                    Type::instance(db, class.top_materialization(db))
                }
                Type::SpecialForm(SpecialFormType::CollectionsAbcCallable) if kind.is_empty() => {
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
