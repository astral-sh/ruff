use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::predicate::{
    ClassPatternPredicateKind, PatternPredicateKind, SequencePatternPredicateKind,
};

use crate::Db;
use crate::types::callable::{CallableFunctionProvenance, CallableTypeKind};
use crate::types::signatures::CallableSignature;
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{
    CallableType, ClassLiteral, IntersectionBuilder, KnownClass, MemberLookupPolicy, Parameter,
    Parameters, Signature, SpecialFormType, Type, TypeContext, UnionType,
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

/// A resolved target for one subpattern inside a class pattern.
///
/// The target captures what the subpattern is matched against after applying class-pattern
/// semantics: either the subject itself for match-self builtins like `int(x)`, or an attribute
/// selected by `__match_args__` / an explicit keyword. It stores a subpattern id rather than
/// borrowing the pattern so narrowing and inference can both map the target back to their own
/// class-pattern representation.
#[derive(Debug)]
pub(crate) enum ClassPatternSubpatternTarget {
    /// A subpattern matched directly against the match subject itself, as in `case int(x)`.
    Subject {
        subpattern: ClassPatternSubpatternId,
    },
    /// A subpattern matched against an attribute of the match subject.
    ///
    /// The attribute name comes either from `__match_args__` for positional subpatterns, or from
    /// the explicit keyword name in the pattern.
    Attribute {
        name: Name,
        subpattern: ClassPatternSubpatternId,
    },
}

/// Identifies the source slot of a class-pattern subpattern.
///
/// Positional ids index the class pattern's positional subpatterns, and keyword ids index its
/// keyword subpatterns. Callers use the id to retrieve the concrete pattern node or predicate that
/// corresponds to a [`ClassPatternSubpatternTarget`].
#[derive(Debug, Clone, Copy)]
pub(crate) enum ClassPatternSubpatternId {
    /// A subpattern from the positional pattern list.
    Positional(usize),
    /// A subpattern from the keyword pattern list.
    Keyword(usize),
}

/// Return the semantic target for each subpattern in a class pattern.
///
/// ```text
/// case Point(0, y=1)
///            ^  ^
///            |  +-- targets subject.y
///            +----- targets the attribute named by Point.__match_args__[0]
///
/// case int(x)
///          ^
///          +-- targets the match subject itself
/// ```
pub(crate) fn class_pattern_targets<'db>(
    db: &'db dyn Db,
    class_pattern: &ClassPatternPredicateKind<'db>,
    class_type: Type<'db>,
) -> Vec<ClassPatternSubpatternTarget> {
    class_pattern_targets_from_parts(
        db,
        class_type,
        class_pattern.positional_patterns.len(),
        class_pattern
            .keyword_patterns
            .iter()
            .enumerate()
            .map(|(index, keyword)| (index, keyword.name.clone())),
    )
}

pub(crate) fn class_pattern_targets_from_parts<'db>(
    db: &'db dyn Db,
    class_type: Type<'db>,
    positional_count: usize,
    keyword_patterns: impl IntoIterator<Item = (usize, Name)>,
) -> Vec<ClassPatternSubpatternTarget> {
    let mut positional_targets = class_pattern_positional_targets(db, class_type, positional_count);

    let keyword_targets = class_pattern_keyword_targets(keyword_patterns);

    positional_targets.extend(keyword_targets);
    positional_targets
}

/// Return the targets for a set of keyword sub-patterns.
///
/// Targets are resolved just via attribute name.
fn class_pattern_keyword_targets(
    keyword_patterns: impl IntoIterator<Item = (usize, Name)>,
) -> Vec<ClassPatternSubpatternTarget> {
    keyword_patterns
        .into_iter()
        .map(|(index, name)| ClassPatternSubpatternTarget::Attribute {
            name,
            subpattern: ClassPatternSubpatternId::Keyword(index),
        })
        .collect()
}

/// Return the target(s) for a set of positional sub-patterns
///
/// The targets are determined in 2 different ways:
/// 1. If there is a `__match_args__` class member defined with a tuple of strings referencing
///    attribute names, those are returned.
/// 2. If the class is a builtin (ie `int`, `str`, etc), the target is `self`.
///    From the PEP: "a single positional sub-pattern is allowed to be passed to the call.
///    Rather than being matched against any particular attribute on the subject,
///    it is instead matched against the subject itself".
fn class_pattern_positional_targets<'db>(
    db: &'db dyn Db,
    class_type: Type<'db>,
    positional_count: usize,
) -> Vec<ClassPatternSubpatternTarget> {
    if positional_count == 0 {
        return Vec::new();
    }

    let Type::ClassLiteral(class) = class_type else {
        return Vec::new();
    };

    if let Some(match_args_ty) = class
        .class_member(db, "__match_args__", MemberLookupPolicy::default())
        .ignore_possibly_undefined()
    {
        let Some(names) = match_args_attribute_names(db, match_args_ty, positional_count) else {
            return Vec::new();
        };

        return names
            .into_iter()
            .enumerate()
            .map(|(index, name)| ClassPatternSubpatternTarget::Attribute {
                name,
                subpattern: ClassPatternSubpatternId::Positional(index),
            })
            .collect();
    }

    if is_match_self_class(db, class) && positional_count == 1 {
        return vec![ClassPatternSubpatternTarget::Subject {
            subpattern: ClassPatternSubpatternId::Positional(0),
        }];
    }

    Vec::new()
}

fn match_args_attribute_names<'db>(
    db: &'db dyn Db,
    match_args_ty: Type<'db>,
    positional_count: usize,
) -> Option<Vec<Name>> {
    let tuple_spec = match_args_ty.exact_tuple_instance_spec(db)?;
    let TupleSpec::Fixed(match_args) = tuple_spec.as_ref() else {
        return None;
    };
    if match_args.len() < positional_count {
        return None;
    }

    match_args
        .elements_slice()
        .iter()
        .take(positional_count)
        .map(|element| {
            element
                .as_string_literal()
                .map(|literal| Name::new(literal.value(db)))
        })
        .collect()
}

fn is_match_self_class<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> bool {
    let class_instance = Type::instance(db, class.default_specialization(db));

    [
        KnownClass::Bool,
        KnownClass::Bytearray,
        KnownClass::Bytes,
        KnownClass::Dict,
        KnownClass::Float,
        KnownClass::FrozenSet,
        KnownClass::Int,
        KnownClass::List,
        KnownClass::Set,
        KnownClass::Str,
        KnownClass::Tuple,
    ]
    .into_iter()
    .any(|known_class| {
        known_class
            .to_class_literal(db)
            .to_class_type(db)
            .is_some_and(|target| class_instance.is_subtype_of(db, Type::instance(db, target)))
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
            // Only return the type if it's single-valued. For non-single-valued types
            // (like `str`), we can't definitively exclude any specific type from
            // subsequent patterns because the pattern could match any value of that type.
            if ty.is_single_valued(db) {
                ty
            } else {
                Type::Never
            }
        }
        PatternPredicateKind::Class(class_pattern) => {
            if class_pattern.kind.is_irrefutable() {
                match infer_same_file_expression_type(
                    db,
                    class_pattern.class,
                    TypeContext::default(),
                ) {
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
        PatternPredicateKind::MatchStar => Type::Never,
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

    if kind.is_exact_length() {
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
    } else {
        let Some((prefix, suffix)) = kind.split_around_star() else {
            return Type::Never;
        };

        Type::tuple(TupleType::mixed(
            db,
            prefix
                .iter()
                .map(|pattern| definite_match_pattern_type(db, pattern)),
            Type::object(),
            suffix
                .iter()
                .map(|pattern| definite_match_pattern_type(db, pattern)),
        ))
    }
}
