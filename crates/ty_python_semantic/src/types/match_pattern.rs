use ruff_python_ast as ast;
use ruff_python_ast::name::Name;
use ty_python_core::predicate::{PatternPredicateKind, SequencePatternPredicateKind};

use crate::Db;
use crate::types::callable::{CallableFunctionProvenance, CallableTypeKind};
use crate::types::signatures::CallableSignature;
use crate::types::{
    CallableType, IntersectionBuilder, KnownClass, Parameter, Parameters, Signature, Type,
    TypeContext, UnionType, infer_same_file_expression_type,
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

pub(crate) fn sequence_pattern_type(db: &dyn Db) -> Type<'_> {
    IntersectionBuilder::new(db)
        .add_positive(KnownClass::Sequence.to_instance(db).top_materialization(db))
        // `str`, `bytes`, and `bytearray` are sequences, but Python sequence
        // patterns explicitly do not match them or their subclasses.
        .add_negative(KnownClass::Str.to_instance(db))
        .add_negative(KnownClass::Bytes.to_instance(db))
        .add_negative(KnownClass::Bytearray.to_instance(db))
        .build()
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
    element_types: &[Type<'db>],
) -> Type<'db> {
    let Ok(length) = i64::try_from(element_types.len()) else {
        return sequence_pattern_type(db);
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

    let mut methods = vec![("__len__", len_method)];

    if !element_types.is_empty() {
        let getitem_overloads = (0..length).zip(element_types).map(|(index, element_type)| {
            Signature::new(
                Parameters::new(
                    db,
                    [
                        self_parameter(),
                        Parameter::positional_only(Some(Name::new_static("index")))
                            .with_annotated_type(Type::int_literal(index)),
                    ],
                ),
                *element_type,
            )
        });

        methods.push((
            "__getitem__",
            CallableType::new(
                db,
                CallableSignature::from_overloads(getitem_overloads),
                CallableTypeKind::FunctionLike,
                CallableFunctionProvenance::None,
            ),
        ));
    }

    let protocol = Type::protocol_with_methods(db, methods);

    IntersectionBuilder::new(db)
        .add_positive(sequence_pattern_type(db))
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
        PatternPredicateKind::Class(class_expr, kind) => {
            if kind.is_irrefutable() {
                infer_same_file_expression_type(db, *class_expr, TypeContext::default())
                    .to_instance(db)
                    .unwrap_or(Type::Never)
                    .top_materialization(db)
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
        PatternPredicateKind::Unsupported => Type::Never,
    }
}

/// Return the values that are guaranteed to match a sequence pattern.
pub(crate) fn definite_sequence_pattern_type<'db>(
    db: &'db dyn Db,
    kind: &SequencePatternPredicateKind<'db>,
) -> Type<'db> {
    if kind.is_irrefutable() {
        return sequence_pattern_type(db);
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
            exact_sequence_pattern_type(db, &element_types)
        }
    } else {
        Type::Never
    }
}
