//! This module contains quickcheck-based property tests for `Type`s.
//!
//! These tests are disabled by default, as they are non-deterministic and slow. You can
//! run them explicitly using:
//!
//! ```sh
//! cargo test -p red_knot_python_semantic -- --ignored types::property_tests::stable
//! ```
//!
//! The number of tests (default: 100) can be controlled by setting the `QUICKCHECK_TESTS`
//! environment variable. For example:
//!
//! ```sh
//! QUICKCHECK_TESTS=10000 cargo test …
//! ```
//!
//! If you want to run these tests for a longer period of time, it's advisable to run them
//! in release mode. As some tests are slower than others, it's advisable to run them in a
//! loop until they fail:
//!
//! ```sh
//! export QUICKCHECK_TESTS=100000
//! while cargo test --release -p red_knot_python_semantic -- \
//!   --ignored types::property_tests::stable; do :; done
//! ```

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use crate::db::tests::{setup_db, TestDb};
use crate::types::{
    builtins_symbol, known_module_symbol, IntersectionBuilder, KnownClass, KnownInstanceType,
    SubclassOfType, TupleType, Type, UnionType,
};
use crate::KnownModule;
use quickcheck::{Arbitrary, Gen};

/// A test representation of a type that can be transformed unambiguously into a real Type,
/// given a db.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Ty {
    Never,
    Unknown,
    None,
    Any,
    IntLiteral(i64),
    BooleanLiteral(bool),
    StringLiteral(&'static str),
    LiteralString,
    BytesLiteral(&'static str),
    // BuiltinInstance("str") corresponds to an instance of the builtin `str` class
    BuiltinInstance(&'static str),
    /// Members of the `abc` stdlib module
    AbcInstance(&'static str),
    AbcClassLiteral(&'static str),
    TypingLiteral,
    // BuiltinClassLiteral("str") corresponds to the builtin `str` class object itself
    BuiltinClassLiteral(&'static str),
    KnownClassInstance(KnownClass),
    Union(Vec<Ty>),
    Intersection {
        pos: Vec<Ty>,
        neg: Vec<Ty>,
    },
    Tuple(Vec<Ty>),
    SubclassOfAny,
    SubclassOfBuiltinClass(&'static str),
    SubclassOfAbcClass(&'static str),
    AlwaysTruthy,
    AlwaysFalsy,
}

impl Ty {
    pub(crate) fn into_type(self, db: &TestDb) -> Type<'_> {
        match self {
            Ty::Never => Type::Never,
            Ty::Unknown => Type::unknown(),
            Ty::None => Type::none(db),
            Ty::Any => Type::any(),
            Ty::IntLiteral(n) => Type::IntLiteral(n),
            Ty::StringLiteral(s) => Type::string_literal(db, s),
            Ty::BooleanLiteral(b) => Type::BooleanLiteral(b),
            Ty::LiteralString => Type::LiteralString,
            Ty::BytesLiteral(s) => Type::bytes_literal(db, s.as_bytes()),
            Ty::BuiltinInstance(s) => builtins_symbol(db, s).expect_type().to_instance(db),
            Ty::AbcInstance(s) => known_module_symbol(db, KnownModule::Abc, s)
                .expect_type()
                .to_instance(db),
            Ty::AbcClassLiteral(s) => known_module_symbol(db, KnownModule::Abc, s).expect_type(),
            Ty::TypingLiteral => Type::KnownInstance(KnownInstanceType::Literal),
            Ty::BuiltinClassLiteral(s) => builtins_symbol(db, s).expect_type(),
            Ty::KnownClassInstance(known_class) => known_class.to_instance(db),
            Ty::Union(tys) => {
                UnionType::from_elements(db, tys.into_iter().map(|ty| ty.into_type(db)))
            }
            Ty::Intersection { pos, neg } => {
                let mut builder = IntersectionBuilder::new(db);
                for p in pos {
                    builder = builder.add_positive(p.into_type(db));
                }
                for n in neg {
                    builder = builder.add_negative(n.into_type(db));
                }
                builder.build()
            }
            Ty::Tuple(tys) => {
                let elements = tys.into_iter().map(|ty| ty.into_type(db));
                TupleType::from_elements(db, elements)
            }
            Ty::SubclassOfAny => SubclassOfType::subclass_of_any(),
            Ty::SubclassOfBuiltinClass(s) => SubclassOfType::from(
                db,
                builtins_symbol(db, s)
                    .expect_type()
                    .expect_class_literal()
                    .class,
            ),
            Ty::SubclassOfAbcClass(s) => SubclassOfType::from(
                db,
                known_module_symbol(db, KnownModule::Abc, s)
                    .expect_type()
                    .expect_class_literal()
                    .class,
            ),
            Ty::AlwaysTruthy => Type::AlwaysTruthy,
            Ty::AlwaysFalsy => Type::AlwaysFalsy,
        }
    }
}

fn arbitrary_core_type(g: &mut Gen) -> Ty {
    // We could select a random integer here, but this would make it much less
    // likely to explore interesting edge cases:
    let int_lit = Ty::IntLiteral(*g.choose(&[-2, -1, 0, 1, 2]).unwrap());
    let bool_lit = Ty::BooleanLiteral(bool::arbitrary(g));
    g.choose(&[
        Ty::Never,
        Ty::Unknown,
        Ty::None,
        Ty::Any,
        int_lit,
        bool_lit,
        Ty::StringLiteral(""),
        Ty::StringLiteral("a"),
        Ty::LiteralString,
        Ty::BytesLiteral(""),
        Ty::BytesLiteral("\x00"),
        Ty::KnownClassInstance(KnownClass::Object),
        Ty::KnownClassInstance(KnownClass::Str),
        Ty::KnownClassInstance(KnownClass::Int),
        Ty::KnownClassInstance(KnownClass::Bool),
        Ty::KnownClassInstance(KnownClass::List),
        Ty::KnownClassInstance(KnownClass::Tuple),
        Ty::KnownClassInstance(KnownClass::FunctionType),
        Ty::KnownClassInstance(KnownClass::SpecialForm),
        Ty::KnownClassInstance(KnownClass::TypeVar),
        Ty::KnownClassInstance(KnownClass::TypeAliasType),
        Ty::KnownClassInstance(KnownClass::NoDefaultType),
        Ty::TypingLiteral,
        Ty::BuiltinClassLiteral("str"),
        Ty::BuiltinClassLiteral("int"),
        Ty::BuiltinClassLiteral("bool"),
        Ty::BuiltinClassLiteral("object"),
        Ty::BuiltinInstance("type"),
        Ty::AbcInstance("ABC"),
        Ty::AbcInstance("ABCMeta"),
        Ty::SubclassOfAny,
        Ty::SubclassOfBuiltinClass("object"),
        Ty::SubclassOfBuiltinClass("str"),
        Ty::SubclassOfBuiltinClass("type"),
        Ty::AbcClassLiteral("ABC"),
        Ty::AbcClassLiteral("ABCMeta"),
        Ty::SubclassOfAbcClass("ABC"),
        Ty::SubclassOfAbcClass("ABCMeta"),
        Ty::AlwaysTruthy,
        Ty::AlwaysFalsy,
    ])
    .unwrap()
    .clone()
}

/// Constructs an arbitrary type.
///
/// The `size` parameter controls the depth of the type tree. For example,
/// a simple type like `int` has a size of 0, `Union[int, str]` has a size
/// of 1, `tuple[int, Union[str, bytes]]` has a size of 2, etc.
fn arbitrary_type(g: &mut Gen, size: u32) -> Ty {
    if size == 0 {
        arbitrary_core_type(g)
    } else {
        match u32::arbitrary(g) % 4 {
            0 => arbitrary_core_type(g),
            1 => Ty::Union(
                (0..*g.choose(&[2, 3]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
            ),
            2 => Ty::Tuple(
                (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
            ),
            3 => Ty::Intersection {
                pos: (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
                neg: (0..*g.choose(&[0, 1, 2]).unwrap())
                    .map(|_| arbitrary_type(g, size - 1))
                    .collect(),
            },
            _ => unreachable!(),
        }
    }
}

impl Arbitrary for Ty {
    fn arbitrary(g: &mut Gen) -> Ty {
        const MAX_SIZE: u32 = 2;
        arbitrary_type(g, MAX_SIZE)
    }

    fn shrink(&self) -> Box<dyn Iterator<Item = Self>> {
        match self.clone() {
            Ty::Union(types) => Box::new(types.shrink().filter_map(|elts| match elts.len() {
                0 => None,
                1 => Some(elts.into_iter().next().unwrap()),
                _ => Some(Ty::Union(elts)),
            })),
            Ty::Tuple(types) => Box::new(types.shrink().filter_map(|elts| match elts.len() {
                0 => None,
                1 => Some(elts.into_iter().next().unwrap()),
                _ => Some(Ty::Tuple(elts)),
            })),
            Ty::Intersection { pos, neg } => {
                // Shrinking on intersections is not exhaustive!
                //
                // We try to shrink the positive side or the negative side,
                // but we aren't shrinking both at the same time.
                //
                // This should remove positive or negative constraints but
                // won't shrink (A & B & ~C & ~D) to (A & ~C) in one shrink
                // iteration.
                //
                // Instead, it hopes that (A & B & ~C) or (A & ~C & ~D) fails
                // so that shrinking can happen there.
                let pos_orig = pos.clone();
                let neg_orig = neg.clone();
                Box::new(
                    // we shrink negative constraints first, as
                    // intersections with only negative constraints are
                    // more confusing
                    neg.shrink()
                        .map(move |shrunk_neg| Ty::Intersection {
                            pos: pos_orig.clone(),
                            neg: shrunk_neg,
                        })
                        .chain(pos.shrink().map(move |shrunk_pos| Ty::Intersection {
                            pos: shrunk_pos,
                            neg: neg_orig.clone(),
                        }))
                        .filter_map(|ty| {
                            if let Ty::Intersection { pos, neg } = &ty {
                                match (pos.len(), neg.len()) {
                                    // an empty intersection does not mean
                                    // anything
                                    (0, 0) => None,
                                    // a single positive element should be
                                    // unwrapped
                                    (1, 0) => Some(pos[0].clone()),
                                    _ => Some(ty),
                                }
                            } else {
                                unreachable!()
                            }
                        }),
                )
            }
            _ => Box::new(std::iter::empty()),
        }
    }
}

static CACHED_DB: OnceLock<Arc<Mutex<TestDb>>> = OnceLock::new();

fn get_cached_db() -> MutexGuard<'static, TestDb> {
    let db = CACHED_DB.get_or_init(|| Arc::new(Mutex::new(setup_db())));
    db.lock().unwrap()
}

/// A macro to define a property test for types.
///
/// The `$test_name` identifier specifies the name of the test function. The `$db` identifier
/// is used to refer to the salsa database in the property to be tested. The actual property is
/// specified using the syntax:
///
///     forall types t1, t2, ..., tn . <property>`
///
/// where `t1`, `t2`, ..., `tn` are identifiers that represent arbitrary types, and `<property>`
/// is an expression using these identifiers.
///
macro_rules! type_property_test {
    ($test_name:ident, $db:ident, forall types $($types:ident),+ . $property:expr) => {
        #[quickcheck_macros::quickcheck]
        #[ignore]
        fn $test_name($($types: super::Ty),+) -> bool {
            let db_cached = super::get_cached_db();
            let $db = &*db_cached;
            $(let $types = $types.into_type($db);)+

            $property
        }
    };
    // A property test with a logical implication.
    ($name:ident, $db:ident, forall types $($types:ident),+ . $premise:expr => $conclusion:expr) => {
        type_property_test!($name, $db, forall types $($types),+ . !($premise) || ($conclusion));
    };
}

fn intersection<'db>(db: &'db TestDb, tys: impl IntoIterator<Item = Type<'db>>) -> Type<'db> {
    let mut builder = IntersectionBuilder::new(db);
    for ty in tys {
        builder = builder.add_positive(ty);
    }
    builder.build()
}

fn union<'db>(db: &'db TestDb, tys: impl IntoIterator<Item = Type<'db>>) -> Type<'db> {
    UnionType::from_elements(db, tys)
}

mod stable {
    use super::union;
    use crate::types::{KnownClass, Type};

    // Reflexivity: `T` is equivalent to itself.
    type_property_test!(
        equivalent_to_is_reflexive, db,
        forall types t. t.is_fully_static(db) => t.is_equivalent_to(db, t)
    );

    // Symmetry: If `S` is equivalent to `T`, then `T` must be equivalent to `S`.
    // Note that this (trivially) holds true for gradual types as well.
    type_property_test!(
        equivalent_to_is_symmetric, db,
        forall types s, t. s.is_equivalent_to(db, t) => t.is_equivalent_to(db, s)
    );

    // Transitivity: If `S` is equivalent to `T` and `T` is equivalent to `U`, then `S` must be equivalent to `U`.
    type_property_test!(
        equivalent_to_is_transitive, db,
        forall types s, t, u. s.is_equivalent_to(db, t) && t.is_equivalent_to(db, u) => s.is_equivalent_to(db, u)
    );

    // Symmetry: If `S` is gradual equivalent to `T`, `T` is gradual equivalent to `S`.
    type_property_test!(
        gradual_equivalent_to_is_symmetric, db,
        forall types s, t. s.is_gradual_equivalent_to(db, t) => t.is_gradual_equivalent_to(db, s)
    );

    // A fully static type `T` is a subtype of itself.
    type_property_test!(
        subtype_of_is_reflexive, db,
        forall types t. t.is_fully_static(db) => t.is_subtype_of(db, t)
    );

    // `S <: T` and `T <: U` implies that `S <: U`.
    type_property_test!(
        subtype_of_is_transitive, db,
        forall types s, t, u. s.is_subtype_of(db, t) && t.is_subtype_of(db, u) => s.is_subtype_of(db, u)
    );

    // `S <: T` and `T <: S` implies that `S` is equivalent to `T`.
    type_property_test!(
        subtype_of_is_antisymmetric, db,
        forall types s, t. s.is_subtype_of(db, t) && t.is_subtype_of(db, s) => s.is_equivalent_to(db, t)
    );

    // `T` is not disjoint from itself, unless `T` is `Never`.
    type_property_test!(
        disjoint_from_is_irreflexive, db,
        forall types t. t.is_disjoint_from(db, t) => t.is_never()
    );

    // `S` is disjoint from `T` implies that `T` is disjoint from `S`.
    type_property_test!(
        disjoint_from_is_symmetric, db,
        forall types s, t. s.is_disjoint_from(db, t) == t.is_disjoint_from(db, s)
    );

    // `S <: T` implies that `S` is not disjoint from `T`, unless `S` is `Never`.
    type_property_test!(
        subtype_of_implies_not_disjoint_from, db,
        forall types s, t. s.is_subtype_of(db, t) => !s.is_disjoint_from(db, t) || s.is_never()
    );

    // `S <: T` implies that `S` can be assigned to `T`.
    type_property_test!(
        subtype_of_implies_assignable_to, db,
        forall types s, t. s.is_subtype_of(db, t) => s.is_assignable_to(db, t)
    );

    // If `T` is a singleton, it is also single-valued.
    type_property_test!(
        singleton_implies_single_valued, db,
        forall types t. t.is_singleton(db) => t.is_single_valued(db)
    );

    // If `T` contains a gradual form, it should not participate in equivalence
    type_property_test!(
        non_fully_static_types_do_not_participate_in_equivalence, db,
        forall types s, t. !s.is_fully_static(db) => !s.is_equivalent_to(db, t) && !t.is_equivalent_to(db, s)
    );

    // If `T` contains a gradual form, it should not participate in subtyping
    type_property_test!(
        non_fully_static_types_do_not_participate_in_subtyping, db,
        forall types s, t. !s.is_fully_static(db) => !s.is_subtype_of(db, t) && !t.is_subtype_of(db, s)
    );

    // All types should be assignable to `object`
    type_property_test!(
        all_types_assignable_to_object, db,
        forall types t. t.is_assignable_to(db, KnownClass::Object.to_instance(db))
    );

    // And for fully static types, they should also be subtypes of `object`
    type_property_test!(
        all_fully_static_types_subtype_of_object, db,
        forall types t. t.is_fully_static(db) => t.is_subtype_of(db, KnownClass::Object.to_instance(db))
    );

    // Never should be assignable to every type
    type_property_test!(
        never_assignable_to_every_type, db,
        forall types t. Type::Never.is_assignable_to(db, t)
    );

    // And it should be a subtype of all fully static types
    type_property_test!(
        never_subtype_of_every_fully_static_type, db,
        forall types t. t.is_fully_static(db) => Type::Never.is_subtype_of(db, t)
    );

    // For any two fully static types, each type in the pair must be a subtype of their union.
    type_property_test!(
        all_fully_static_type_pairs_are_subtype_of_their_union, db,
        forall types s, t.
            s.is_fully_static(db) && t.is_fully_static(db)
            => s.is_subtype_of(db, union(db, [s, t])) && t.is_subtype_of(db, union(db, [s, t]))
    );

    // A fully static type does not have any materializations.
    // Thus, two equivalent (fully static) types are also gradual equivalent.
    type_property_test!(
        two_equivalent_types_are_also_gradual_equivalent, db,
        forall types s, t. s.is_equivalent_to(db, t) => s.is_gradual_equivalent_to(db, t)
    );

    // Two gradual equivalent fully static types are also equivalent.
    type_property_test!(
        two_gradual_equivalent_fully_static_types_are_also_equivalent, db,
        forall types s, t.
            s.is_fully_static(db) && s.is_gradual_equivalent_to(db, t) => s.is_equivalent_to(db, t)
    );

    // `T` can be assigned to itself.
    type_property_test!(
        assignable_to_is_reflexive, db,
        forall types t. t.is_assignable_to(db, t)
    );

    // For *any* pair of types, whether fully static or not,
    // each of the pair should be assignable to the union of the two.
    type_property_test!(
        all_type_pairs_are_assignable_to_their_union, db,
        forall types s, t. s.is_assignable_to(db, union(db, [s, t])) && t.is_assignable_to(db, union(db, [s, t]))
    );
}

/// This module contains property tests that currently lead to many false positives.
///
/// The reason for this is our insufficient understanding of equivalence of types. For
/// example, we currently consider `int | str` and `str | int` to be different types.
/// Similar issues exist for intersection types. Once this is resolved, we can move these
/// tests to the `stable` section. In the meantime, it can still be useful to run these
/// tests (using [`types::property_tests::flaky`]), to see if there are any new obvious bugs.
mod flaky {
    use itertools::Itertools;

    use super::{intersection, union};

    // Negating `T` twice is equivalent to `T`.
    type_property_test!(
        double_negation_is_identity, db,
        forall types t. t.negate(db).negate(db).is_equivalent_to(db, t)
    );

    // ~T should be disjoint from T
    type_property_test!(
        negation_is_disjoint, db,
        forall types t. t.is_fully_static(db) => t.negate(db).is_disjoint_from(db, t)
    );

    // If `S <: T`, then `~T <: ~S`.
    type_property_test!(
        negation_reverses_subtype_order, db,
        forall types s, t. s.is_subtype_of(db, t) => t.negate(db).is_subtype_of(db, s.negate(db))
    );

    // For two fully static types, their intersection must be a subtype of each type in the pair.
    type_property_test!(
        all_fully_static_type_pairs_are_supertypes_of_their_intersection, db,
        forall types s, t.
            s.is_fully_static(db) && t.is_fully_static(db)
            => intersection(db, [s, t]).is_subtype_of(db, s) && intersection(db, [s, t]).is_subtype_of(db, t)
    );

    // And for non-fully-static types, the intersection of a pair of types
    // should be assignable to both types of the pair.
    // Currently fails due to https://github.com/astral-sh/ruff/issues/14899
    type_property_test!(
        all_type_pairs_can_be_assigned_from_their_intersection, db,
        forall types s, t. intersection(db, [s, t]).is_assignable_to(db, s) && intersection(db, [s, t]).is_assignable_to(db, t)
    );

    // Equal element sets of intersections implies equivalence
    // flaky at least in part because of https://github.com/astral-sh/ruff/issues/15513
    type_property_test!(
        intersection_equivalence_not_order_dependent, db,
        forall types s, t, u.
            s.is_fully_static(db) && t.is_fully_static(db) && u.is_fully_static(db)
            => [s, t, u]
                .into_iter()
                .permutations(3)
                .map(|trio_of_types| intersection(db, trio_of_types))
                .permutations(2)
                .all(|vec_of_intersections| vec_of_intersections[0].is_equivalent_to(db, vec_of_intersections[1]))
    );

    // Equal element sets of unions implies equivalence
    // flaky at laest in part because of https://github.com/astral-sh/ruff/issues/15513
    type_property_test!(
        union_equivalence_not_order_dependent, db,
        forall types s, t, u.
            s.is_fully_static(db) && t.is_fully_static(db) && u.is_fully_static(db)
            => [s, t, u]
                .into_iter()
                .permutations(3)
                .map(|trio_of_types| union(db, trio_of_types))
                .permutations(2)
                .all(|vec_of_unions| vec_of_unions[0].is_equivalent_to(db, vec_of_unions[1]))
    );

    // `S | T` is always a supertype of `S`.
    // Thus, `S` is never disjoint from `S | T`.
    type_property_test!(
        constituent_members_of_union_is_not_disjoint_from_that_union, db,
        forall types s, t.
            !s.is_disjoint_from(db, union(db, [s, t])) && !t.is_disjoint_from(db, union(db, [s, t]))
    );
}
