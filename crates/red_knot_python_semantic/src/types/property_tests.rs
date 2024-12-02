//! This module contains quickcheck-based property tests for `Type`s.
//!
//! These tests are feature-gated and disabled by default. You can run them using:
//!
//! ```sh
//! cargo test -p red_knot_python_semantic --features property_tests types::property_tests
//! ```
//!
//! You can control the number of tests (default: 100) by setting the `QUICKCHECK_TESTS`
//! variable. For example:
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
//! while cargo test --release -p red_knot_python_semantic \
//!   --features property_tests types::property_tests; do :; done
//! ```

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use super::tests::{setup_db, Ty};
use super::Type;
use crate::db::tests::TestDb;
use crate::types::KnownClass;
use crate::Db;
use quickcheck::{Arbitrary, Gen};
use quickcheck_macros::quickcheck;

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

impl<'db> Type<'db> {
    /// Checks if a type contains `Never` in its tree.
    ///
    /// This is currently needed since we currently don't propagate/simplify types containing
    /// `Never`. For example, the type `tuple[int, Never]` is equivalent to `Never`. Similarly,
    /// we simplify `Never` in unions, but we would not simplify `str | tuple[int, Never]`, so
    /// we need to descent into unions and intersections as well.
    fn contains_never(&self, db: &'db dyn Db) -> bool {
        match self {
            Type::Never => true,
            Type::Union(types) => types.elements(db).iter().any(|t| t.contains_never(db)),
            Type::Tuple(types) => types.elements(db).iter().any(|t| t.contains_never(db)),
            Type::Intersection(inner) => {
                inner.positive(db).iter().any(|t| t.contains_never(db))
                    || inner.negative(db).iter().any(|t| t.contains_never(db))
            }
            _ => false,
        }
    }
}

static CACHED_DB: OnceLock<Arc<Mutex<TestDb>>> = OnceLock::new();

fn get_cached_db() -> MutexGuard<'static, TestDb> {
    let db = CACHED_DB.get_or_init(|| Arc::new(Mutex::new(setup_db())));
    db.lock().unwrap()
}

macro_rules! type_property_test {
    ($name:ident, $db:ident, ($($types:ident),+), $body:expr) => {
        #[quickcheck]
        fn $name($($types: Ty),+) -> bool {
            let db_cached = get_cached_db();
            let $db = &*db_cached;
            $(let $types = $types.into_type($db);)+

            $body
        }
    };
    // A property test with a logical implication.
    ($name:ident, $db:ident, ($($types:ident),+), $premise:expr => $conclusion:expr) => {
        type_property_test!($name, $db, ($($types),+), !($premise) || ($conclusion));
    };
    // A property test with a single type argument
    ($name:ident, $db:ident, $type:ident, $body:expr) => {
        type_property_test!($name, $db, ($type), $body);
    };
    // Same, for the implication branch
    ($name:ident, $db:ident, $type:ident, $premise:expr => $conclusion:expr) => {
        type_property_test!($name, $db, ($type), $premise => $conclusion);
    };
}

// `T` is equivalent to itself.
type_property_test!(equivalent_to_is_reflexive, db, t, t.is_equivalent_to(db, t));

// `T` is a subtype of itself.
type_property_test!(subtype_of_is_reflexive, db, t, t.is_subtype_of(db, t));

// `S <: T` and `T <: S` implies that `S` is equivalent to `T`.
type_property_test!(
    subtype_of_is_antisymmetric,
    db,
    (s, t),
    s.is_subtype_of(db, t) && t.is_subtype_of(db, s) => s.is_equivalent_to(db, t)
);

// `S <: T` and `T <: U` implies that `S <: U`.
type_property_test!(
    subtype_of_is_transitive,
    db,
    (s, t, u),
    s.is_subtype_of(db, t) && t.is_subtype_of(db, u) => s.is_subtype_of(db, u)
);

// `T` is not disjoint from itself, unless `T` is `Never`.
type_property_test!(
    disjoint_from_is_irreflexive,
    db,
    t,
    t.is_disjoint_from(db, t) => t.contains_never(db)
);

// `S` is disjoint from `T` implies that `T` is disjoint from `S`.
type_property_test!(
    disjoint_from_is_symmetric,
    db,
    (s, t),
    s.is_disjoint_from(db, t) == t.is_disjoint_from(db, s)
);

// `S <: T` implies that `S` is not disjoint from `T`, unless `S` is `Never`.
type_property_test!(subtype_of_implies_not_disjoint_from, db, (s, t),
    s.is_subtype_of(db, t) => !s.is_disjoint_from(db, t) || s.contains_never(db)
);

// `T` can be assigned to itself.
type_property_test!(assignable_to_is_reflexive, db, t, t.is_assignable_to(db, t));

// `S <: T` implies that `S` can be assigned to `T`.
type_property_test!(
    subtype_of_implies_assignable_to,
    db,
    (s, t),
    s.is_subtype_of(db, t) => s.is_assignable_to(db, t)
);

// If `T` is a singleton, it is also single-valued.
type_property_test!(
    singleton_implies_single_valued,
    db,
    t,
    t.is_singleton(db) => t.is_single_valued(db)
);

// Negating `T` twice is equivalent to `T`.
type_property_test!(
    double_negation_is_identity,
    db,
    t,
    t.negate(db).negate(db).is_equivalent_to(db, t)
);
