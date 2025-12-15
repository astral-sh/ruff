//! This module contains quickcheck-based property tests for `Type`s.
//!
//! These tests are disabled by default, as they are non-deterministic and slow. You can
//! run them explicitly using:
//!
//! ```sh
//! cargo test -p ty_python_semantic -- --ignored types::property_tests::stable
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
//! while cargo test --release -p ty_python_semantic -- \
//!   --ignored types::property_tests::stable; do :; done
//! ```
mod setup;
mod type_generation;

use type_generation::{intersection, union};

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
macro_rules! type_property_test {
    ($test_name:ident, $db:ident, forall types $($types:ident),+ . $property:expr) => {
        #[quickcheck_macros::quickcheck]
        #[ignore]
        fn $test_name($($types: crate::types::property_tests::type_generation::Ty),+) -> bool {
            let $db = &crate::types::property_tests::setup::get_cached_db();
            $(let $types = $types.into_type($db);)+
            let result = $property;

            if !result {
                println!("\nFailing types were:");
                $(println!("{}", $types.display($db));)+
            }

            result
        }
    };

    ($test_name:ident, $db:ident, forall fully_static_types $($types:ident),+ . $property:expr) => {
        #[quickcheck_macros::quickcheck]
        #[ignore]
        fn $test_name($($types: crate::types::property_tests::type_generation::FullyStaticTy),+) -> bool {
            let $db = &crate::types::property_tests::setup::get_cached_db();
            $(let $types = $types.into_type($db);)+
            let result = $property;

            if !result {
                println!("\nFailing types were:");
                $(println!("{}", $types.display($db));)+
            }

            result
        }
    };

    // A property test with a logical implication.
    ($name:ident, $db:ident, forall $typekind:ident $($types:ident),+ . $premise:expr => $conclusion:expr) => {
        type_property_test!($name, $db, forall $typekind $($types),+ . !($premise) || ($conclusion));
    };
}

mod stable {
    use super::union;
    use crate::types::{CallableType, KnownClass, Type};

    // Reflexivity: `T` is equivalent to itself.
    type_property_test!(
        equivalent_to_is_reflexive, db,
        forall types t. t.is_equivalent_to(db, t)
    );

    // Symmetry: If `S` is equivalent to `T`, then `T` must be equivalent to `S`.
    type_property_test!(
        equivalent_to_is_symmetric, db,
        forall types s, t. s.is_equivalent_to(db, t) => t.is_equivalent_to(db, s)
    );

    // Transitivity: If `S` is equivalent to `T` and `T` is equivalent to `U`, then `S` must be equivalent to `U`.
    type_property_test!(
        equivalent_to_is_transitive, db,
        forall types s, t, u. s.is_equivalent_to(db, t) && t.is_equivalent_to(db, u) => s.is_equivalent_to(db, u)
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

    // All types should be assignable to `object`
    type_property_test!(
        all_types_assignable_to_object, db,
        forall types t. t.is_assignable_to(db, Type::object())
    );

    // And all types should be subtypes of `object`
    type_property_test!(
        all_types_subtype_of_object, db,
        forall types t. t.is_subtype_of(db, Type::object())
    );

    // Never should be assignable to every type
    type_property_test!(
        never_assignable_to_every_type, db,
        forall types t. Type::Never.is_assignable_to(db, t)
    );

    // And it should be a subtype of all types
    type_property_test!(
        never_subtype_of_every_type, db,
        forall types t. Type::Never.is_subtype_of(db, t)
    );

    // Similar to `Never`, a "bottom" callable type should be a subtype of all callable types
    type_property_test!(
        bottom_callable_is_subtype_of_all_callable, db,
        forall types t. t.is_callable_type()
            => Type::Callable(CallableType::bottom(db)).is_subtype_of(db, t)
    );

    // `T` can be assigned to itself.
    type_property_test!(
        assignable_to_is_reflexive, db,
        forall types t. t.is_assignable_to(db, t)
    );

    // For *any* pair of types, each of the pair should be assignable to the union of the two.
    type_property_test!(
        all_type_pairs_are_assignable_to_their_union, db,
        forall types s, t. s.is_assignable_to(db, union(db, [s, t])) && t.is_assignable_to(db, union(db, [s, t]))
    );

    // Only `Never` is a subtype of `Any`.
    type_property_test!(
        only_never_is_subtype_of_any, db,
        forall types s. !s.is_equivalent_to(db, Type::Never) => !s.is_subtype_of(db, Type::any())
    );

    // Only `object` is a supertype of `Any`.
    type_property_test!(
        only_object_is_supertype_of_any, db,
        forall types t. !t.is_equivalent_to(db, Type::object()) => !Type::any().is_subtype_of(db, t)
    );

    // Equivalence is commutative.
    type_property_test!(
        equivalent_to_is_commutative, db,
        forall types s, t. s.is_equivalent_to(db, t) == t.is_equivalent_to(db, s)
    );

    // A fully static type `T` is a subtype of itself. (This is not true for non-fully-static
    // types; `Any` is not a subtype of `Any`, only `Never` is.)
    type_property_test!(
        subtype_of_is_reflexive_for_fully_static_types, db,
        forall fully_static_types t. t.is_subtype_of(db, t)
    );

    // For any two fully static types, each type in the pair must be a subtype of their union.
    // (This is clearly not true for non-fully-static types, since their subtyping is not
    // reflexive.)
    type_property_test!(
        all_fully_static_type_pairs_are_subtype_of_their_union, db,
        forall fully_static_types s, t. s.is_subtype_of(db, union(db, [s, t])) && t.is_subtype_of(db, union(db, [s, t]))
    );

    // Any type assignable to `Iterable[object]` should be considered iterable.
    //
    // Note that the inverse is not true, due to the fact that we recognize the old-style
    // iteration protocol as well as the new-style iteration protocol: not all objects that
    // we consider iterable are assignable to `Iterable[object]`.
    //
    // Note also that (like other property tests in this module),
    // this invariant will only hold true for Liskov-compliant types assignable to `Iterable`.
    // Since protocols can participate in nominal assignability/subtyping as well as
    // structural assignability/subtyping, it is possible to construct types that a type
    // checker must consider to be subtypes of `Iterable` even though they are not in fact
    // iterable (as long as the user `type: ignore`s any type-checker errors stemming from
    // the Liskov violation). All you need to do is to create a class that subclasses
    // `Iterable` but assigns `__iter__ = None` in the class body (or similar).
    type_property_test!(
        all_type_assignable_to_iterable_are_iterable, db,
        forall types t. t.is_assignable_to(db, KnownClass::Iterable.to_specialized_instance(db, [Type::object()])) => t.try_iterate(db).is_ok()
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

    // For any fully static type `T`, `T` should be disjoint from `~T`.
    // https://github.com/astral-sh/ty/issues/216
    type_property_test!(
        negation_of_fully_static_types_is_disjoint, db,
        forall fully_static_types t. t.negate(db).is_disjoint_from(db, t)
    );

    // For two types, their intersection must be a subtype of each type in the pair.
    type_property_test!(
        all_type_pairs_are_supertypes_of_their_intersection, db,
        forall types s, t.
            intersection(db, [s, t]).is_subtype_of(db, s) && intersection(db, [s, t]).is_subtype_of(db, t)
    );

    // And the intersection of a pair of types
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
            [s, t, u]
                .into_iter()
                .permutations(3)
                .map(|trio_of_types| intersection(db, trio_of_types))
                .permutations(2)
                .all(|vec_of_intersections| vec_of_intersections[0].is_equivalent_to(db, vec_of_intersections[1]))
    );

    // Equal element sets of unions implies equivalence
    // flaky at least in part because of https://github.com/astral-sh/ruff/issues/15513
    type_property_test!(
        union_equivalence_not_order_dependent, db,
        forall types s, t, u.
            [s, t, u]
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

    // If `S <: T`, then `~T <: ~S`.
    //
    // DO NOT STABILISE this test until the mdtests here pass:
    // https://github.com/astral-sh/ruff/blob/2711e08eb8eb38d1ce323aae0517fede371cba15/crates/ty_python_semantic/resources/mdtest/type_properties/is_subtype_of.md?plain=1#L276-L315
    //
    // This test has flakes relating to those subtyping and simplification tests
    // (see https://github.com/astral-sh/ruff/issues/16913), but it is hard to
    // reliably trigger the flakes when running this test manually as the flakes
    // occur very rarely (even running the test with several million seeds does
    // not always reliably reproduce the flake).
    type_property_test!(
        negation_reverses_subtype_order, db,
        forall types s, t. s.is_subtype_of(db, t) => t.negate(db).is_subtype_of(db, s.negate(db))
    );

    // Both the top and bottom materialization tests are flaky in part due to various failures that
    // it discovers in the current implementation of assignability of the types.
    // TODO: Create a issue with some example failures to keep track of it

    // `T'`, the top materialization of `T`, should be assignable to `T`.
    type_property_test!(
        top_materialization_of_type_is_assignable_to_type, db,
        forall types t. t.top_materialization(db).is_assignable_to(db, t)
    );

    // Similarly, `T'`, the bottom materialization of `T`, should also be assignable to `T`.
    type_property_test!(
        bottom_materialization_of_type_is_assigneble_to_type, db,
        forall types t. t.bottom_materialization(db).is_assignable_to(db, t)
    );
}
