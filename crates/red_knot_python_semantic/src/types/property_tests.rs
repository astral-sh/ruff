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

use super::tests::Ty;
use crate::db::tests::{setup_db, TestDb};
use crate::types::KnownClass;
use quickcheck::{Arbitrary, Gen};

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
        // This is incredibly naive. We can do much better here by
        // trying various subsets of the elements in unions, tuples,
        // and intersections. For now, we only try to shrink by
        // reducing unions/tuples/intersections to a single element.
        match self.clone() {
            Ty::Union(types) => Box::new(types.into_iter()),
            Ty::Tuple(types) => Box::new(types.into_iter()),
            Ty::Intersection { pos, neg } => Box::new(pos.into_iter().chain(neg)),
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
        fn $test_name($($types: crate::types::tests::Ty),+) -> bool {
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

mod stable {
    // `T` is equivalent to itself.
    type_property_test!(
        equivalent_to_is_reflexive, db,
        forall types t. t.is_fully_static(db) => t.is_equivalent_to(db, t)
    );

    // `T` is a subtype of itself.
    type_property_test!(
        subtype_of_is_reflexive, db,
        forall types t. t.is_fully_static(db) => t.is_subtype_of(db, t)
    );

    // `S <: T` and `T <: U` implies that `S <: U`.
    type_property_test!(
        subtype_of_is_transitive, db,
        forall types s, t, u. s.is_subtype_of(db, t) && t.is_subtype_of(db, u) => s.is_subtype_of(db, u)
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

    // `T` can be assigned to itself.
    type_property_test!(
        assignable_to_is_reflexive, db,
        forall types t. t.is_assignable_to(db, t)
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
}

/// This module contains property tests that currently lead to many false positives.
///
/// The reason for this is our insufficient understanding of equivalence of types. For
/// example, we currently consider `int | str` and `str | int` to be different types.
/// Similar issues exist for intersection types. Once this is resolved, we can move these
/// tests to the `stable` section. In the meantime, it can still be useful to run these
/// tests (using [`types::property_tests::flaky`]), to see if there are any new obvious bugs.
mod flaky {
    // `S <: T` and `T <: S` implies that `S` is equivalent to `T`.
    type_property_test!(
        subtype_of_is_antisymmetric, db,
        forall types s, t. s.is_subtype_of(db, t) && t.is_subtype_of(db, s) => s.is_equivalent_to(db, t)
    );

    // Negating `T` twice is equivalent to `T`.
    type_property_test!(
        double_negation_is_identity, db,
        forall types t. t.negate(db).negate(db).is_equivalent_to(db, t)
    );
}
