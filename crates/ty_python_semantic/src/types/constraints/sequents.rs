//! The [`SequentMap`] and related functionality

use std::cell::Cell;
use std::fmt::{Debug, Display};

use crate::types::constraints::variables::{
    ConcreteEquivalenceBound, ConcreteLowerBound, ConcreteUpperBound, Constraint,
    ConstraintProvenance, ProvidesConcreteBound, ProvidesConcreteLowerBound,
    ProvidesConcreteUpperBound, ProvidesTypeVarEquivalenceBound, ProvidesTypeVarRangeBound,
    TypeVarEquivalenceBound, TypeVarRangeBound,
};
use crate::types::constraints::{
    ALWAYS_FALSE, ConstraintId, ConstraintSetBuilder, ConstraintSetStorage, Node,
    OwnedConstraintSet, max_constructor_and_typevar_depth,
};
use crate::types::typevar::TypeVarSet;
use crate::types::variance::VarianceInferable;
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{BoundTypeVarInstance, IntersectionType, Type, TypeVarVariance, UnionType};
use crate::{Db, Program, ProgramEnvironment};

/// A collection of _sequents_ that describe how the constraints mentioned in a BDD relate to each
/// other. These are used in several BDD operations that need to know about "derived facts" even if
/// they are not mentioned in the BDD directly. These operations involve walking one or more paths
/// from the root node to a terminal node. Each sequent describes paths that are invalid (which are
/// pruned from the search), and new constraints that we can assume to be true even if we haven't
/// seen them directly.
///
/// Sequent maps are primarily used when walking a BDD path with a
/// [`PathAssignments`][super::paths::PathAssignments]. The
/// `PathAssignments` will hold a sequent map containing all of the constraints that are
/// encountered during the walk. It builds up its sequent map lazily, so that it only has to
/// include sequents for the constraints that are actually encountered. However, we also don't want
/// to perform duplicate work if we perform multiple BDD walks on the same constraint set. The
/// [`for_constraint`][Self::for_constraint] and [`for_constraint_pair`][Self::for_constraint_pair]
/// methods are salsa-tracked, to ensure that we only perform them once for any particular
/// constraint or pair of constraints. `PathAssignments` invokes these methods when it encounters a
/// new constraint, and then merges those cached sequents into its own sequent map. (That means we
/// also share the work of calculating the sequent map across `PathAssignments` for _different_
/// constraint sets.)
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct SequentMap<C> {
    pub(super) sequents: Vec<Sequent<C>>,
}

impl<C> Default for SequentMap<C> {
    fn default() -> Self {
        Self {
            sequents: Vec::default(),
        }
    }
}

/// Describes one rule for deriving new implicit constraints from existing constraints in a BDD
/// path.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum Sequent<C> {
    /// Sequent of the form `¬C → false`
    ///
    /// This indicates that `C` is always true. Any path that assumes it is false is impossible and
    /// can be pruned.
    SingleTautology { ante: C },

    /// Sequent of the form `C₁ ∧ C₂ → false`
    ///
    /// This indicates that `C₁` and `C₂` are disjoint: it is not possible for both to hold. Any
    /// path that assumes both is impossible and can be pruned.
    PairImpossibility { ante1: C, ante2: C },

    /// Sequent of the form `C₁ ∧ C₂ ∧ C₃ → false`
    ///
    /// This indicates that `C₁`, `C₂` and `C₃` are mutually disjoint: it is not possible for all
    /// three to hold. Any path that assumes all three is impossible and can be pruned.
    #[expect(unused)]
    TripleImpossibility { ante1: C, ante2: C, ante3: C },

    /// Sequent of the form `C → D`
    ///
    /// This indicates that `C` on its own is enough to imply `D`. For any path that assumes `C`
    /// holds, we can add `D` to the path even if it doesn't appear in the BDD.
    SingleImplication { ante: C, post: C },

    /// Sequent of the form `C₁ ∧ C₂ → D`
    ///
    /// This indicates that if `C₁` and `C₂` are both true, then `D` is guaranteed to be true as
    /// well. For any path that assumes both `C₁` and `C₂` hold, we can add `D` to the path even if
    /// it doesn't appear in the BDD.
    PairImplication { ante1: C, ante2: C, post: C },
}

impl SequentMap<ConstraintId> {
    #[expect(dead_code)] // Keep this around for debugging purposes
    fn display<'db, 'a>(
        &'a self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        storage: &'a ConstraintSetStorage<'db>,
        prefix: &'a dyn Display,
    ) -> impl Display + 'a {
        std::fmt::from_fn(move |f| {
            let mut first = true;
            let mut maybe_write_prefix = |f: &mut std::fmt::Formatter<'_>| {
                if first {
                    first = false;
                    Ok(())
                } else {
                    write!(f, "\n{prefix}")
                }
            };

            for sequent in &self.sequents {
                match sequent {
                    Sequent::SingleTautology { .. } => {}

                    Sequent::PairImpossibility { ante1, ante2 } => {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} ∧ {} → false",
                            ante1.display(db, env, storage),
                            ante2.display(db, env, storage),
                        )?;
                    }

                    Sequent::TripleImpossibility {
                        ante1,
                        ante2,
                        ante3,
                    } => {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} ∧ {} ∧ {} → false",
                            ante1.display(db, env, storage),
                            ante2.display(db, env, storage),
                            ante3.display(db, env, storage),
                        )?;
                    }

                    Sequent::PairImplication { ante1, ante2, post } => {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} ∧ {} → {}",
                            ante1.display(db, env, storage),
                            ante2.display(db, env, storage),
                            post.display(db, env, storage),
                        )?;
                    }

                    Sequent::SingleImplication { ante, post } => {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} → {}",
                            ante.display(db, env, storage),
                            post.display(db, env, storage)
                        )?;
                    }
                }
            }

            if first {
                f.write_str("[no sequents]")?;
            }
            Ok(())
        })
    }
}

#[expect(dead_code)]
impl<'db> SequentMap<Constraint<'db>> {
    fn add_single_tautology(&mut self, ante: Constraint<'db>) {
        self.sequents.push(Sequent::SingleTautology { ante });
    }

    fn add_pair_impossibility(&mut self, ante1: Constraint<'db>, ante2: Constraint<'db>) {
        self.sequents
            .push(Sequent::PairImpossibility { ante1, ante2 });
    }

    #[expect(unused)]
    fn add_triple_impossibility(
        &mut self,
        ante1: Constraint<'db>,
        ante2: Constraint<'db>,
        ante3: Constraint<'db>,
    ) {
        self.sequents.push(Sequent::TripleImpossibility {
            ante1,
            ante2,
            ante3,
        });
    }

    fn add_pair_implication(
        &mut self,
        ante1: Constraint<'db>,
        ante2: Constraint<'db>,
        post: Constraint<'db>,
    ) {
        self.sequents
            .push(Sequent::PairImplication { ante1, ante2, post });
    }

    fn add_single_implication(&mut self, ante: Constraint<'db>, post: Constraint<'db>) {
        self.sequents
            .push(Sequent::SingleImplication { ante, post });
    }

    /// Returns a sequent map containing the sequents that we can infer from a single constraint in
    /// isolation. This method is cached so that we only perform this work once per
    /// constraint.
    pub(super) fn for_constraint(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraint: Constraint<'db>,
    ) -> &'db Self {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _, _| SequentMap::default(),
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn for_constraint_inner<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            constraint: Constraint<'db>,
        ) -> SequentMap<Constraint<'db>> {
            let env = &ProgramEnvironment::from_program(program);
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                constraint = %constraint.display(db, env, Some(true)),
                "add sequents for constraint",
            );
            let mut map = SequentMap::<Constraint<'db>>::default();
            constraint.add_sequents(db, env, &mut map);
            map.sequents.shrink_to_fit();
            map
        }

        for_constraint_inner(db, env.program(db), constraint)
    }

    /// Returns a sequent map containing the sequents that we can infer from a pair of constraints.
    /// This method is cached so that we only perform this work once per constraint pair.
    ///
    /// (Note that this method is _not_ commutative; you should provide `left` and `right` in the
    /// order that they appear in the source code, so that we can construct derived constraints
    /// that retain that ordering.)
    pub(super) fn for_constraint_pair(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> &'db Self {
        #[salsa::tracked(
            returns(ref),
            cycle_initial=|_, _, _, _, _| SequentMap::default(),
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn for_constraint_pair_inner<'db>(
            db: &'db dyn Db,
            program: Program<'db>,
            left: Constraint<'db>,
            right: Constraint<'db>,
        ) -> SequentMap<Constraint<'db>> {
            let env = &ProgramEnvironment::from_program(program);
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left.display(db, env, Some(true)),
                right = %right.display(db, env, Some(true)),
                "add sequents for constraint pair",
            );
            let mut map = SequentMap::<Constraint<'db>>::default();
            left.add_sequents_with(db, env, &mut map, right);
            map.sequents.shrink_to_fit();
            map
        }

        for_constraint_pair_inner(db, env.program(db), left, right)
    }

    /// Quickly determines whether two constraints cannot possibly produce any sequents when passed
    /// to [`for_constraint_pair`][Self::for_constraint_pair]. If this returns `true`, it is safe
    /// to skip calling `for_constraint_pair` for this pair of constraints.
    pub(super) fn pair_cannot_produce_sequents(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left: Constraint<'db>,
        right: Constraint<'db>,
    ) -> bool {
        // Currently, the only pattern we look for is when two concrete lower-bound constraints
        // have disjoint bounds. Given `l₁ ≤ T ∧ l₂ ≤ T`, the only sequent we could theoretically
        // produce is `(l₁ | l₂) ≤ T`. But we don't store that as a single constraint; we always
        // break that apart into the two smaller constraints that we started with.

        let Constraint::ConcreteLower(left) = left else {
            return false;
        };
        let Constraint::ConcreteLower(right) = right else {
            return false;
        };
        if !left.typevar.is_same_typevar_as(db, right.typevar) {
            return false;
        }

        let builder = ConstraintSetBuilder::new();
        left.bound
            .when_trivially_disjoint_from(db, env, right.bound, &builder, TypeVarSet::None)
            .is_trivially_always_satisfied()
    }
}

impl<'db> Constraint<'db> {
    fn add_sequents(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
    ) {
        match self {
            Constraint::ConcreteLower(this) => this.add_sequents(db, env, map),
            Constraint::ConcreteUpper(this) => this.add_sequents(db, env, map),
            Constraint::ConcreteEquivalence(this) => this.add_sequents(db, env, map),
            Constraint::TypeVarRange(this) => this.add_sequents(db, env, map),
            Constraint::TypeVarEquivalence(this) => this.add_sequents(db, env, map),
        }
    }

    fn add_sequents_with(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: Self,
    ) {
        match (self, other) {
            (Constraint::ConcreteLower(this), Constraint::ConcreteLower(other)) => {
                this.add_sequents_with_concrete_lower(db, env, map, other, false);
            }
            (Constraint::ConcreteLower(this), Constraint::ConcreteUpper(other)) => {
                this.add_sequents_with_concrete_upper(db, env, map, other, false);
            }
            (Constraint::ConcreteUpper(other), Constraint::ConcreteLower(this)) => {
                this.add_sequents_with_concrete_upper(db, env, map, other, true);
            }
            (Constraint::ConcreteLower(this), Constraint::ConcreteEquivalence(other)) => {
                this.add_sequents_with_concrete_equivalence(db, env, map, other, false);
            }
            (Constraint::ConcreteEquivalence(other), Constraint::ConcreteLower(this)) => {
                this.add_sequents_with_concrete_equivalence(db, env, map, other, true);
            }
            (Constraint::ConcreteLower(this), Constraint::TypeVarRange(other)) => {
                this.add_sequents_with_typevar_range(db, env, map, other, false);
            }
            (Constraint::TypeVarRange(other), Constraint::ConcreteLower(this)) => {
                this.add_sequents_with_typevar_range(db, env, map, other, true);
            }
            (Constraint::ConcreteLower(this), Constraint::TypeVarEquivalence(other)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, false);
            }
            (Constraint::TypeVarEquivalence(other), Constraint::ConcreteLower(this)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, true);
            }

            (Constraint::ConcreteUpper(this), Constraint::ConcreteUpper(other)) => {
                this.add_sequents_with_concrete_upper(db, env, map, other, false);
            }
            (Constraint::ConcreteUpper(this), Constraint::ConcreteEquivalence(other)) => {
                this.add_sequents_with_concrete_equivalence(db, env, map, other, false);
            }
            (Constraint::ConcreteEquivalence(other), Constraint::ConcreteUpper(this)) => {
                this.add_sequents_with_concrete_equivalence(db, env, map, other, true);
            }
            (Constraint::ConcreteUpper(this), Constraint::TypeVarRange(other)) => {
                this.add_sequents_with_typevar_range(db, env, map, other, false);
            }
            (Constraint::TypeVarRange(other), Constraint::ConcreteUpper(this)) => {
                this.add_sequents_with_typevar_range(db, env, map, other, true);
            }
            (Constraint::ConcreteUpper(this), Constraint::TypeVarEquivalence(other)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, false);
            }
            (Constraint::TypeVarEquivalence(other), Constraint::ConcreteUpper(this)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, true);
            }

            (Constraint::ConcreteEquivalence(this), Constraint::ConcreteEquivalence(other)) => {
                this.add_sequents_with_concrete_equivalence(db, env, map, other, false);
            }
            (Constraint::ConcreteEquivalence(this), Constraint::TypeVarRange(other)) => {
                this.add_sequents_with_typevar_range(db, env, map, other, false);
            }
            (Constraint::TypeVarRange(other), Constraint::ConcreteEquivalence(this)) => {
                this.add_sequents_with_typevar_range(db, env, map, other, true);
            }
            (Constraint::ConcreteEquivalence(this), Constraint::TypeVarEquivalence(other)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, false);
            }
            (Constraint::TypeVarEquivalence(other), Constraint::ConcreteEquivalence(this)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, true);
            }

            (Constraint::TypeVarRange(this), Constraint::TypeVarRange(other)) => {
                this.add_sequents_with_typevar_range(db, env, map, other, false);
            }
            (Constraint::TypeVarRange(this), Constraint::TypeVarEquivalence(other)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, false);
            }
            (Constraint::TypeVarEquivalence(other), Constraint::TypeVarRange(this)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, true);
            }

            (Constraint::TypeVarEquivalence(this), Constraint::TypeVarEquivalence(other)) => {
                this.add_sequents_with_typevar_equivalence(db, env, map, other, false);
            }
        }
    }

    fn add_sequents_for_range(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        lower: impl ProvidesConcreteLowerBound<'db>,
        upper: impl ProvidesConcreteUpperBound<'db>,
    ) {
        // Given constraints `α ≤ T` and `T ≤ β`, `α ≤ β` must also hold. If those bounds contain
        // other typevars, we can infer additional constraints. (α and U won't be bare typevars,
        // since those will be modeled by `TypeVarRange` bounds.)
        //
        //   1. `(Covariant[S] ≤ T) ∧ (T ≤ Covariant[U]) → (S ≤ U)`
        //      `(Covariant[S] ≤ T) ∧ (T ≤ Covariant[τ]) → (S ≤ τ)`
        //      `(Covariant[τ] ≤ T) ∧ (T ≤ Covariant[U]) → (τ ≤ U)`
        //
        //   2. `(Contravariant[S] ≤ T) ∧ (T ≤ Contravariant[U]) → (U ≤ S)`
        //      `(Contravariant[S] ≤ T) ∧ (T ≤ Contravariant[τ]) → (τ ≤ S)`
        //      `(Contravariant[τ] ≤ T) ∧ (T ≤ Contravariant[U]) → (U ≤ τ)`
        //
        //   3. `(Invariant[S] ≤ T) ∧ (T ≤ Invariant[U]) → (S = U)`
        //      `(Invariant[S] ≤ T) ∧ (T ≤ Invariant[τ]) → (S = τ)`
        //      `(Invariant[τ] ≤ T) ∧ (T ≤ Invariant[U]) → (τ = U)`
        //
        // and whenever the bounds are assignable, even if they don't mention exactly the same
        // types:
        //
        //   class Sub(Covariant[int]): ...
        //
        //   4. `(Covariant[S] ≤ T ≤ Sub) → (S ≤ int)`
        //      `(Sub ≤ T ≤ Covariant[U]) → (int ≤ U)`
        //
        // To handle all of these cases, we perform a constraint set assignability check to see
        // when `α ≤ β`. This gives us a constraint set, which should be the rhs of the sequent
        // implication. (That is, this check directly encodes `(α ≤ T) ∧ (T ≤ β) → (α ≤ β)` as an
        // implication.)

        let lower_constraint = lower.into();
        let lower = lower.into_lower_bound();
        let upper_constraint = upper.into();
        let upper = upper.into_upper_bound();

        // Skip trivial cases where the assignability check won't produce useful results.
        if lower.bound() == lower.typevar().domain(db).bottom(db)
            || upper.bound() == upper.typevar().domain(db).top(db)
        {
            return;
        }

        let when = lower
            .bound()
            .when_constraint_set_assignable_to_owned(db, env, upper.bound());
        Self::add_constraint_set_implication(
            map,
            lower_constraint,
            upper_constraint,
            when.as_ref(),
        );
    }

    fn add_sequents_for_equivalence(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        lower: impl ProvidesConcreteLowerBound<'db>,
        upper: impl ProvidesConcreteUpperBound<'db>,
    ) {
        // Given constraints `T = α` and `T = β`, `α = β` must also hold. If those bounds contain
        // other typevars, we can infer additional constraints.

        let lower_constraint = lower.into();
        let lower = lower.into_lower_bound();
        let upper_constraint = upper.into();
        let upper = upper.into_upper_bound();

        let when = lower
            .bound()
            .when_constraint_set_equivalent_to_owned(db, env, upper.bound());
        Self::add_constraint_set_implication(
            map,
            lower_constraint,
            upper_constraint,
            when.as_ref(),
        );
    }

    fn add_constraint_set_implication(
        map: &mut SequentMap<Constraint<'db>>,
        lower_constraint: Self,
        upper_constraint: Self,
        when: &OwnedConstraintSet<'db>,
    ) {
        when.query(|builder, when| {
            // If the relation _never_ holds, these constraints are contradictory.
            if when.is_trivially_never_satisfied() {
                map.add_pair_impossibility(lower_constraint, upper_constraint);
                return;
            }

            // Fast path: If the relation _always_, there are no derived constraints
            // that we can infer. This would be handled correctly by the logic below, but this is a
            // useful early return. Since we only use this check as an early return happy path, we can
            // accept false negatives. That lets us use the simpler and cheaper check against
            // ALWAYS_TRUE, rather than a more expensive is_always_satisfiable call.
            if when.is_trivially_always_satisfied() {
                return;
            }

            // Technically, we've just calculated a _constraint set_ as the rhs of this implication.
            // Unfortunately, our sequent map can currently only store implications where the rhs is a
            // single constraint.
            //
            // If the constraint set that we get represents a single conjunction, we can still shoehorn
            // it into this shape, since we can "break apart" a conjunction on the rhs of an
            // implication:
            //
            //   a → b ∧ c ∧ d
            //
            // becomes
            //
            //   a → b
            //   a → c
            //   a → d
            //
            // That takes care of breaking apart the rhs conjunction: we can add each positive
            // constraint as a separate single_implication.
            //
            // We can also handle _negative_ constraints, because those turn into impossibilities:
            //
            //   a → ¬b
            //
            // becomes
            //
            //   a ∧ b → false
            //
            // TODO: This should handle the most common cases. In the future, we could handle arbitrary
            // rhs constraint sets by moving this logic into PathAssignments::walk_path, and performing
            // it once for _every_ root→always path in the BDD. (That would require resetting the
            // PathAssignments state for each of those paths, which is why the logic would have to
            // move.)
            let mut storage = builder.storage.borrow_mut();
            let mut node = when.node;
            if !node.is_single_conjunction(&mut storage) {
                return;
            }

            loop {
                match node.node() {
                    Node::AlwaysTrue | Node::AlwaysFalse => break,
                    Node::Interior(interior) => {
                        let interior = storage.interior_node_data(interior.node());
                        let derived = storage.constraint_data(interior.constraint);
                        if interior.if_true != ALWAYS_FALSE {
                            map.add_pair_implication(lower_constraint, upper_constraint, derived);
                            node = interior.if_true;
                        } else {
                            map.add_triple_impossibility(
                                lower_constraint,
                                upper_constraint,
                                derived,
                            );
                            node = interior.if_false;
                        }
                    }
                }
            }
        });
    }

    /// Substitutes `replacement_bound` for `replacement_typevar` as long as it does not
    /// recursively deepen the bound on `needle_typevar`.
    ///
    /// A replacement containing `replacement_typevar`, such as substituting `G[U]` for `U`, can be
    /// fed back into the same substitution to produce `G[G[U]]`. A replacement containing
    /// `needle_typevar` can produce the same cycle across the two constraints. Repeatedly
    /// following either pattern does not reach a fixed point, so we skip both.
    fn substitute_if_not_recursive(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        needle_typevar: BoundTypeVarInstance<'db>,
        needle_bound: Type<'db>,
        replacement_typevar: BoundTypeVarInstance<'db>,
        replacement_bound: Type<'db>,
    ) -> Option<Type<'db>> {
        // Gradual assignability is not transitive. Substituting a dynamic replacement into another
        // bound would let an uncertain relationship participate in an arbitrarily long sequent
        // chain.
        if !replacement_bound.is_static_sequent_eligible(db, env) {
            return None;
        }

        // A self-referential bound can consume another bound on the same typevar repeatedly. For
        // example, combining `F[U] ≤ U` with `M ≤ U` would first produce `F[M] ≤ U`, then
        // `F[F[M]] ≤ U`, and so on.
        if needle_typevar.is_same_typevar_as(db, replacement_typevar) {
            return None;
        }

        // For a concrete replacement nested inside a non-set-theoretic type, require constructor
        // nesting to decrease. This gives recursive chains a well-founded measure: replacing `U`
        // in `F[U]` with `F[M]` would otherwise produce `F[F[M]]`, which can be fed back into the
        // same substitution. Unions and intersections do not add runtime constructor nesting, so
        // their transitive simplification is unaffected.
        if !replacement_bound.is_type_var()
            && !matches!(needle_bound, Type::Union(_) | Type::Intersection(_))
        {
            let (needle_constructor_depth, _) =
                max_constructor_and_typevar_depth(db, env, needle_bound);
            let (replacement_constructor_depth, _) =
                max_constructor_and_typevar_depth(db, env, replacement_bound);
            if replacement_constructor_depth >= needle_constructor_depth {
                return None;
            }
        }

        if let Type::TypeVar(replacement) = replacement_bound
            && (replacement.is_same_typevar_as(db, needle_typevar)
                || replacement.is_same_typevar_as(db, replacement_typevar))
        {
            return None;
        }

        if replacement_bound
            .variance_of(db, env, needle_typevar.identity(db))
            .evaluate(db)
            != TypeVarVariance::Bivariant
        {
            return None;
        }
        if replacement_bound
            .variance_of(db, env, replacement_typevar.identity(db))
            .evaluate(db)
            != TypeVarVariance::Bivariant
        {
            return None;
        }

        Some(needle_bound.substitute_one_typevar(db, env, replacement_typevar, replacement_bound))
    }

    fn add_covariant_lower_tightened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteLowerBound<'db>,
        right: impl ProvidesConcreteLowerBound<'db>,
    ) {
        let left_constraint = left.into();
        let left = left.into_lower_bound();
        let right_constraint = right.into();
        let right = right.into_lower_bound();

        // Given `α ≤ T` and `β ≤ U`, if α contains U covariantly, we can substitute β for U:
        //
        //   (Co[U] ≤ T) ∧ (β ≤ U) ⇒ (Co[β] ≤ T)
        if left
            .bound()
            .variance_of(db, env, right.typevar().identity(db))
            .evaluate(db)
            != TypeVarVariance::Covariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.typevar(),
            right.bound(),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left_constraint, right_constraint, derived.into());
    }

    fn add_covariant_upper_tightened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteUpperBound<'db>,
        right: impl ProvidesConcreteUpperBound<'db>,
    ) {
        let left_constraint = left.into();
        let left = left.into_upper_bound();
        let right_constraint = right.into();
        let right = right.into_upper_bound();

        // Given `T ≤ α` and `U ≤ β`, if α contains U covariantly, we can substitute β for U:
        //
        //   (T ≤ Co[U]) ∧ (U ≤ β) ⇒ (T ≤ Co[β])
        if left
            .bound()
            .variance_of(db, env, right.typevar().identity(db))
            .evaluate(db)
            != TypeVarVariance::Covariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.typevar(),
            right.bound(),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left_constraint, right_constraint, derived.into());
    }

    fn add_covariant_equivalence_tightened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: ConcreteEquivalenceBound<'db>,
        right: ConcreteEquivalenceBound<'db>,
    ) {
        // Given `T = α` and `U = β`, if α contains U covariantly, we can substitute β for U:
        //
        //   (T = Co[U]) ∧ (U = β) ⇒ (T = Co[β])
        if left
            .bound()
            .variance_of(db, env, right.typevar().identity(db))
            .evaluate(db)
            != TypeVarVariance::Covariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.typevar(),
            right.bound(),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left.into(), right.into(), derived.into());
    }

    fn add_contravariant_tightened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        lower: impl ProvidesConcreteLowerBound<'db>,
        upper: impl ProvidesConcreteUpperBound<'db>,
    ) {
        let lower_constraint = lower.into();
        let lower = lower.into_lower_bound();
        let upper_constraint = upper.into();
        let upper = upper.into_upper_bound();
        let provenance = ConstraintProvenance::derived(lower.provenance(), upper.provenance());

        // Given `α ≤ T` and `U ≤ β`, if α contains U contravariantly, substitute β for U:
        //
        //   (Contra[U] ≤ T) ∧ (U ≤ β) ⇒ (Contra[β] ≤ T)
        if lower
            .bound()
            .variance_of(db, env, upper.typevar().identity(db))
            .evaluate(db)
            == TypeVarVariance::Contravariant
            && let Some(replacement) = Constraint::substitute_if_not_recursive(
                db,
                env,
                lower.typevar(),
                lower.bound(),
                upper.typevar(),
                upper.bound(),
            )
        {
            let derived = lower.map(provenance, replacement);
            map.add_pair_implication(lower_constraint, upper_constraint, derived.into());
        }

        // If β contains T contravariantly, substitute α for T:
        //
        //   (α ≤ T) ∧ (U ≤ Contra[T]) ⇒ (U ≤ Contra[α])
        if upper
            .bound()
            .variance_of(db, env, lower.typevar().identity(db))
            .evaluate(db)
            == TypeVarVariance::Contravariant
            && let Some(replacement) = Constraint::substitute_if_not_recursive(
                db,
                env,
                upper.typevar(),
                upper.bound(),
                lower.typevar(),
                lower.bound(),
            )
        {
            let derived = upper.map(provenance, replacement);
            map.add_pair_implication(lower_constraint, upper_constraint, derived.into());
        }
    }

    fn add_invariant_tightened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteBound<'db>,
        right: ConcreteEquivalenceBound<'db>,
    ) {
        // Given `T ~ α` and `U = β`, if α contains U invariantly, we can substitute β for U. For
        // instance,
        //
        //   (T ~ In[U]) ∧ (U = β) ⇒ (T ~ In[β])
        if left
            .bound()
            .variance_of(db, env, right.typevar().identity(db))
            .evaluate(db)
            != TypeVarVariance::Invariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.typevar(),
            right.bound(),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left.into(), right.into(), derived.into());
    }

    fn add_covariant_lower_weakened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteLowerBound<'db>,
        right: impl ProvidesTypeVarRangeBound<'db>,
    ) {
        let left_constraint = left.into();
        let left = left.into_lower_bound();

        // Given `α ≤ T` and `S ≤ U`, if α contains U covariantly, we can substitute S for U. For
        // instance,
        //
        //   (Co[U] ≤ T) ∧ (S ≤ U) ⇒ (Co[S] ≤ T)
        if left
            .bound()
            .variance_of(db, env, right.right().identity(db))
            .evaluate(db)
            != TypeVarVariance::Covariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.right(),
            Type::TypeVar(right.left()),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left_constraint, right.into(), derived.into());
    }

    fn add_covariant_upper_weakened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteUpperBound<'db>,
        right: impl ProvidesTypeVarRangeBound<'db>,
    ) {
        let left_constraint = left.into();
        let left = left.into_upper_bound();

        // Given `T ≤ α` and `U ≤ S`, if α contains U covariantly, we can substitute S for U. For
        // instance,
        //
        //   (T ≤ Co[U]) ∧ (U ≤ S) ⇒ (T ≤ Co[S])
        if left
            .bound()
            .variance_of(db, env, right.left().identity(db))
            .evaluate(db)
            != TypeVarVariance::Covariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.left(),
            Type::TypeVar(right.right()),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left_constraint, right.into(), derived.into());
    }

    fn add_covariant_equivalence_weakened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: ConcreteEquivalenceBound<'db>,
        right: impl ProvidesTypeVarEquivalenceBound<'db>,
    ) {
        // Given `T = α` and `S = U`, if α contains S covariantly, we can substitute U for S. For
        // instance,
        //
        //   (T = Co[S]) ∧ (S = U) ⇒ (T = Co[U])
        if left
            .bound()
            .variance_of(db, env, right.left().identity(db))
            .evaluate(db)
            != TypeVarVariance::Covariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.left(),
            Type::TypeVar(right.right()),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left.into(), right.into(), derived.into());
    }

    fn add_contravariant_lower_weakened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteLowerBound<'db>,
        right: impl ProvidesTypeVarRangeBound<'db>,
    ) {
        let left_constraint = left.into();
        let left = left.into_lower_bound();

        // Given `α ≤ T` and `U ≤ S`, if α contains U contravariantly, we can substitute S for U
        // and flip the constraint. For instance,
        //
        //   (Contra[U] ≤ T) ∧ (U ≤ S) ⇒ (Contra[S] ≤ T)
        if left
            .bound()
            .variance_of(db, env, right.left().identity(db))
            .evaluate(db)
            != TypeVarVariance::Contravariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.left(),
            Type::TypeVar(right.right()),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left_constraint, right.into(), derived.into());
    }

    fn add_contravariant_upper_weakened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteUpperBound<'db>,
        right: impl ProvidesTypeVarRangeBound<'db>,
    ) {
        let left_constraint = left.into();
        let left = left.into_upper_bound();

        // Given `T ≤ α` and `S ≤ U`, if α contains U contravariantly, we can substitute S for U
        // and flip the constraint. For instance,
        //
        //   (T ≤ Contra[U]) ∧ (S ≤ U) ⇒ (T ≤ Contra[S])
        if left
            .bound()
            .variance_of(db, env, right.right().identity(db))
            .evaluate(db)
            != TypeVarVariance::Contravariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.right(),
            Type::TypeVar(right.left()),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left_constraint, right.into(), derived.into());
    }

    fn add_contravariant_equivalence_weakened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: ConcreteEquivalenceBound<'db>,
        right: impl ProvidesTypeVarEquivalenceBound<'db>,
    ) {
        // Given `T = α` and `S = U`, if α contains U contravariantly, we can substitute S for U
        // and flip the constraint. For instance,
        //
        //   (T = Contra[U]) ∧ (S = U) ⇒ (T = Contra[S])
        if left
            .bound()
            .variance_of(db, env, right.left().identity(db))
            .evaluate(db)
            != TypeVarVariance::Contravariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.left(),
            Type::TypeVar(right.right()),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left.into(), right.into(), derived.into());
    }

    fn add_invariant_weakened_sequent(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        left: impl ProvidesConcreteBound<'db>,
        right: impl ProvidesTypeVarEquivalenceBound<'db>,
    ) {
        // Given `T ~ α` and `S = U`, if α contains U invariantly, we can substitute S for U. For
        // instance,
        //
        //   (T ~ In[U]) ∧ (S = U) ⇒ (T ~ In[S])
        if left
            .bound()
            .variance_of(db, env, right.left().identity(db))
            .evaluate(db)
            != TypeVarVariance::Invariant
        {
            return;
        }
        let Some(replacement) = Constraint::substitute_if_not_recursive(
            db,
            env,
            left.typevar(),
            left.bound(),
            right.left(),
            Type::TypeVar(right.right()),
        ) else {
            return;
        };
        let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
        let derived = left.map(provenance, replacement);
        map.add_pair_implication(left.into(), right.into(), derived.into());
    }
}

fn possibly_reversed_intersection<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    reversed: bool,
    left: Type<'db>,
    right: Type<'db>,
) -> Type<'db> {
    if reversed {
        IntersectionType::from_two_elements(db, env, right, left)
    } else {
        IntersectionType::from_two_elements(db, env, left, right)
    }
}

fn possibly_reversed_union<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    reversed: bool,
    left: Type<'db>,
    right: Type<'db>,
) -> Type<'db> {
    if reversed {
        UnionType::from_two_elements(db, env, right, left)
    } else {
        UnionType::from_two_elements(db, env, left, right)
    }
}

impl<'db> ConcreteLowerBound<'db> {
    fn add_sequents(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
    ) {
        // `⊥ ≤ T` is always true
        if self.bound == self.typevar.domain(db).bottom(db) {
            map.add_single_tautology(self.into());
        }

        // `⊤ ≤ T` implies `T = ⊤`
        if self.bound == self.typevar.domain(db).top(db) {
            let derived = ConcreteEquivalenceBound::new(self.provenance, self.typevar, self.bound);
            map.add_single_implication(self.into(), derived.into());
        }
    }

    fn add_sequents_with_concrete_lower(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: ConcreteLowerBound<'db>,
        reversed: bool,
    ) {
        // We can infer sequents from `α ≤ T` and `β ≤ U` if α _contains_ U and/or β contains T.
        if !self.typevar.is_same_typevar_as(db, other.typevar) {
            Constraint::add_covariant_lower_tightened_sequent(db, env, map, self, other);
            Constraint::add_covariant_lower_tightened_sequent(db, env, map, other, self);
            return;
        }

        // These might seem redundant with the union calculation check below, since `a → b` means
        // that `a ∧ b = a`. But we are not normalizing constraint bounds, and these clauses help
        // us identify constraints that are identical besides e.g. ordering of union/intersection
        // elements. (For instance, when processing `τ₁ & τ₂ ≤ T` and `τ₂ & τ₁ ≤ T`, these clauses
        // would add sequents for `(τ₁ & τ₂ ≤ T) → (τ₂ & τ₁ ≤ T)` and vice versa.)

        // (β ≤ α) ⇒ ((α ≤ T) ⇒ (β ≤ T))
        if other
            .bound
            .is_constraint_set_assignable_to(db, env, self.bound)
        {
            map.add_single_implication(self.into(), other.into());
        }

        // (α ≤ β) ⇒ ((β ≤ T) ⇒ (α ≤ T))
        if self
            .bound
            .is_constraint_set_assignable_to(db, env, other.bound)
        {
            map.add_single_implication(other.into(), self.into());
        }

        // `(α ≤ T) ∧ (β ≤ T)` is equivalent to `(α | β) ≤ T`. We do not create lower bounds that
        // are unions, so only add sequents when the union simplifies away.
        let combined = possibly_reversed_union(db, env, reversed, self.bound, other.bound);
        if !combined.is_union() {
            let provenance = ConstraintProvenance::simplified(
                self.provenance,
                self.bound,
                other.provenance,
                other.bound,
                combined,
            );
            let combined = ConcreteLowerBound::new(provenance, self.typevar, combined);

            // The result is an equivalence, so add implications in both directions.
            map.add_pair_implication(self.into(), other.into(), combined.into());
            map.add_single_implication(combined.into(), self.into());
            map.add_single_implication(combined.into(), other.into());
        }
    }

    fn add_sequents_with_concrete_upper(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: ConcreteUpperBound<'db>,
        _reversed: bool,
    ) {
        // We can infer sequents from `α ≤ T` and `U ≤ β` if α _contains_ U and/or β contains T.
        if !self.typevar.is_same_typevar_as(db, other.typevar) {
            Constraint::add_contravariant_tightened_sequent(db, env, map, self, other);

            // `(T ≤ pivot) ∧ (pivot ≤ U) → (T ≤ U)` when both constraints use the same
            // fully static pivot type.
            if other.bound != self.typevar.domain(db).bottom(db)
                && other.bound != self.typevar.domain(db).top(db)
                && !self.bound.has_typevar(db, env)
                && !other.bound.has_typevar(db, env)
                && self.bound.is_static_sequent_eligible(db, env)
                && other.bound.is_static_sequent_eligible(db, env)
                && other
                    .bound
                    .is_constraint_set_equivalent_to(db, env, self.bound)
            {
                let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
                let derived = TypeVarRangeBound::new(db, provenance, other.typevar, self.typevar);
                map.add_pair_implication(self.into(), other.into(), derived.into());
            }
            return;
        }

        // `(α ≤ T) ∧ (T ≤ β)` simplifies to `T = α` when `α = β`. (We don't need to add the
        // projection implication `(T = α) ⇒ (α ≤ T)`, since anything we can derive from `α ≤ T` we
        // can also derive from `T = α`.)
        let lower = self.bound.bottom_materialization(db, env);
        let upper = other.bound.top_materialization(db, env);
        if lower.is_constraint_set_equivalent_to(db, env, upper) {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let simplified = ConcreteEquivalenceBound::new(provenance, self.typevar, lower);
            map.add_pair_implication(self.into(), other.into(), simplified.into());
            return;
        }

        // Gradual assignability is not transitive, so only fully static bounds can contribute
        // additional range sequents.
        if self.bound.is_static_sequent_eligible(db, env)
            && other.bound.is_static_sequent_eligible(db, env)
        {
            Constraint::add_sequents_for_range(db, env, map, self, other);
        }
    }

    fn add_sequents_with_concrete_equivalence(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: ConcreteEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // We can infer sequents from `α ≤ T` and `U = β` if α _contains_ U and/or β contains T.
        if !self.typevar.is_same_typevar_as(db, other.typevar) {
            Constraint::add_covariant_lower_tightened_sequent(db, env, map, self, other);
            Constraint::add_covariant_lower_tightened_sequent(db, env, map, other, self);
            Constraint::add_contravariant_tightened_sequent(db, env, map, self, other);
            Constraint::add_invariant_tightened_sequent(db, env, map, self, other);

            // `(pivot ≤ T) ∧ (U = pivot) → (U ≤ T)`.
            if !self.bound.has_typevar(db, env)
                && !other.bound.has_typevar(db, env)
                && self.bound.is_static_sequent_eligible(db, env)
                && other.bound.is_static_sequent_eligible(db, env)
                && self
                    .bound
                    .is_constraint_set_equivalent_to(db, env, other.bound)
            {
                let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
                let derived = TypeVarRangeBound::new(db, provenance, other.typevar, self.typevar);
                map.add_pair_implication(self.into(), other.into(), derived.into());
            }
            return;
        }

        // (α ≤ β) ⇒ ((T = β) ⇒ (α ≤ T))
        if self
            .bound
            .is_constraint_set_assignable_to(db, env, other.bound)
        {
            map.add_single_implication(other.into(), self.into());
        }

        // Given constraints `α ≤ T` and `T = β`, `α ≤ β` must also hold. If those bounds contain
        // other typevars, we can infer additional constraints.
        Constraint::add_sequents_for_range(db, env, map, self, other);
    }

    fn add_sequents_with_typevar_range(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarRangeBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `α ≤ T` and `T ≤ U`, `α ≤ U` must also hold.
        if self.typevar.is_same_typevar_as(db, other.left) {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteLowerBound::new(provenance, other.right, self.bound);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // We can infer sequents from `α ≤ T` and `S ≤ U` if α _contains_ U.
        Constraint::add_covariant_lower_weakened_sequent(db, env, map, self, other);
        Constraint::add_contravariant_lower_weakened_sequent(db, env, map, self, other);
    }

    fn add_sequents_with_typevar_equivalence(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `α ≤ T` and `T = U`, `α ≤ U` must also hold.
        let other_typevar = if self.typevar.is_same_typevar_as(db, other.left) {
            Some(other.right)
        } else if self.typevar.is_same_typevar_as(db, other.right) {
            Some(other.left)
        } else {
            None
        };
        if let Some(other_typevar) = other_typevar {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteLowerBound::new(provenance, other_typevar, self.bound);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // We can infer sequents from `α ≤ T` and `S ≤ U` if α _contains_ U.
        Constraint::add_covariant_lower_weakened_sequent(db, env, map, self, other.forwards());
        Constraint::add_covariant_lower_weakened_sequent(db, env, map, self, other.backwards());
        Constraint::add_contravariant_lower_weakened_sequent(db, env, map, self, other.forwards());
        Constraint::add_contravariant_lower_weakened_sequent(db, env, map, self, other.backwards());
        Constraint::add_invariant_weakened_sequent(db, env, map, self, other.forwards());
        Constraint::add_invariant_weakened_sequent(db, env, map, self, other.backwards());
    }
}

impl<'db> ConcreteUpperBound<'db> {
    fn add_sequents(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
    ) {
        // `T ≤ ⊤` is always true
        if self.bound == self.typevar.domain(db).top(db) {
            map.add_single_tautology(self.into());
        }

        // `T ≤ ⊥` implies `T = ⊥`
        if self.bound == self.typevar.domain(db).bottom(db) {
            let derived = ConcreteEquivalenceBound::new(self.provenance, self.typevar, self.bound);
            map.add_single_implication(self.into(), derived.into());
        }
    }

    fn add_sequents_with_concrete_upper(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: ConcreteUpperBound<'db>,
        reversed: bool,
    ) {
        // We can infer sequents from `T ≤ α` and `U ≤ β` if α _contains_ U and/or β contains T.
        if !self.typevar.is_same_typevar_as(db, other.typevar) {
            Constraint::add_covariant_upper_tightened_sequent(db, env, map, self, other);
            Constraint::add_covariant_upper_tightened_sequent(db, env, map, other, self);
            return;
        }

        // These might seem redundant with the intersection calculation check below, since `a → b`
        // means that `a ∧ b = a`. But we are not normalizing constraint bounds, and these clauses
        // help us identify constraints that are identical besides e.g. ordering of
        // union/intersection elements. (For instance, when processing `T ≤ τ₁ | τ₂` and
        // `T ≤ τ₂ | τ₁`, these clauses would add sequents for `(T ≤ τ₁ | τ₂) → (T ≤ τ₂ | τ₁)` and
        // vice versa.)

        // (α ≤ β) ⇒ ((T ≤ α) ⇒ (T ≤ β))
        if self
            .bound
            .is_constraint_set_assignable_to(db, env, other.bound)
        {
            map.add_single_implication(self.into(), other.into());
        }

        // (β ≤ α) ⇒ ((T ≤ β) ⇒ (T ≤ α))
        if other
            .bound
            .is_constraint_set_assignable_to(db, env, self.bound)
        {
            map.add_single_implication(other.into(), self.into());
        }

        // Keep unions as separate, factored upper bounds. Intersecting a union with another bound
        // can distribute the result into a union of intersections. That expanded type no longer
        // looks like an intersection, and repeatedly combining it with other upper bounds can
        // produce a combinatorial number of equivalent constraints.
        if self.bound.is_union() || other.bound.is_union() {
            return;
        }

        // `(T ≤ α) ∧ (T ≤ β)` is equivalent to `T ≤ (α & β)`. We do not create upper bounds that
        // are intersections, so only add sequents when the intersection simplifies away.
        let combined = possibly_reversed_intersection(db, env, reversed, self.bound, other.bound);
        if !combined.is_nontrivial_intersection(db) {
            let provenance = ConstraintProvenance::simplified(
                self.provenance,
                self.bound,
                other.provenance,
                other.bound,
                combined,
            );
            let combined = ConcreteUpperBound::new(provenance, self.typevar, combined);

            // The result is an equivalence, so add implications in both directions.
            map.add_pair_implication(self.into(), other.into(), combined.into());
            map.add_single_implication(combined.into(), self.into());
            map.add_single_implication(combined.into(), other.into());
        }
    }

    fn add_sequents_with_concrete_equivalence(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: ConcreteEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // We can infer sequents from `T ≤ α` and `U = β` if α _contains_ U and/or β contains T.
        if !self.typevar.is_same_typevar_as(db, other.typevar) {
            Constraint::add_covariant_upper_tightened_sequent(db, env, map, self, other);
            Constraint::add_covariant_upper_tightened_sequent(db, env, map, other, self);
            Constraint::add_contravariant_tightened_sequent(db, env, map, other, self);
            Constraint::add_invariant_tightened_sequent(db, env, map, self, other);

            // `(T ≤ pivot) ∧ (U = pivot) → (T ≤ U)`.
            if !self.bound.has_typevar(db, env)
                && !other.bound.has_typevar(db, env)
                && self.bound.is_static_sequent_eligible(db, env)
                && other.bound.is_static_sequent_eligible(db, env)
                && self
                    .bound
                    .is_constraint_set_equivalent_to(db, env, other.bound)
            {
                let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
                let derived = TypeVarRangeBound::new(db, provenance, self.typevar, other.typevar);
                map.add_pair_implication(self.into(), other.into(), derived.into());
            }
            return;
        }

        // (β ≤ α) ⇒ ((T = β) ⇒ (T ≤ α))
        if other
            .bound
            .is_constraint_set_assignable_to(db, env, self.bound)
        {
            map.add_single_implication(other.into(), self.into());
        }

        // Given constraints `T ≤ α` and `T = β`, `α ≤ β` must also hold. If those bounds contain
        // other typevars, we can infer additional constraints.
        Constraint::add_sequents_for_range(db, env, map, other, self);
    }

    fn add_sequents_with_typevar_range(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarRangeBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `T ≤ α` and `U ≤ T`, `U ≤ α` must also hold.
        if self.typevar.is_same_typevar_as(db, other.right) {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteUpperBound::new(provenance, other.left, self.bound);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // We can infer sequents from `T ≤ α` and `S ≤ U` if α _contains_ S.
        Constraint::add_covariant_upper_weakened_sequent(db, env, map, self, other);
        Constraint::add_contravariant_upper_weakened_sequent(db, env, map, self, other);
    }

    fn add_sequents_with_typevar_equivalence(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `T ≤ α` and `U = T`, `U ≤ α` must also hold.
        let other_typevar = if self.typevar.is_same_typevar_as(db, other.left) {
            Some(other.right)
        } else if self.typevar.is_same_typevar_as(db, other.right) {
            Some(other.left)
        } else {
            None
        };
        if let Some(other_typevar) = other_typevar {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteUpperBound::new(provenance, other_typevar, self.bound);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // We can infer sequents from `T ≤ α` and `S = U` if α _contains_ S.
        Constraint::add_covariant_upper_weakened_sequent(db, env, map, self, other.forwards());
        Constraint::add_covariant_upper_weakened_sequent(db, env, map, self, other.backwards());
        Constraint::add_contravariant_upper_weakened_sequent(db, env, map, self, other.forwards());
        Constraint::add_contravariant_upper_weakened_sequent(db, env, map, self, other.backwards());
        Constraint::add_invariant_weakened_sequent(db, env, map, self, other.forwards());
        Constraint::add_invariant_weakened_sequent(db, env, map, self, other.backwards());
    }
}

impl<'db> ConcreteEquivalenceBound<'db> {
    #[expect(clippy::unused_self)]
    fn add_sequents(
        self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _map: &mut SequentMap<Constraint<'db>>,
    ) {
        // We cannot infer any sequents from `T = α` on its own.
    }

    fn add_sequents_with_concrete_equivalence(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: ConcreteEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // We can infer sequents from `T = α` and `U = β` if α _contains_ U and/or β contains T.
        if !self.typevar.is_same_typevar_as(db, other.typevar) {
            Constraint::add_covariant_equivalence_tightened_sequent(db, env, map, self, other);
            Constraint::add_covariant_equivalence_tightened_sequent(db, env, map, other, self);
            Constraint::add_contravariant_tightened_sequent(db, env, map, self, other);
            Constraint::add_contravariant_tightened_sequent(db, env, map, other, self);
            Constraint::add_invariant_tightened_sequent(db, env, map, self, other);
            Constraint::add_invariant_tightened_sequent(db, env, map, other, self);
            return;
        }

        // Given `T = α` and `T = β`, if α and β are equivalent (but not _identical_), we can infer
        // either from the other.
        if self.bound == other.bound {
            return;
        }
        if self
            .bound
            .is_constraint_set_equivalent_to(db, env, other.bound)
        {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteEquivalenceBound::new(provenance, other.typevar, other.bound);
            map.add_single_implication(self.into(), derived.into());
            let derived = ConcreteEquivalenceBound::new(provenance, self.typevar, self.bound);
            map.add_single_implication(other.into(), derived.into());
        }

        // Given constraints `T = α` and `T = β`, `α = β` must also hold. If those bounds contain
        // other typevars, we can infer additional constraints.
        Constraint::add_sequents_for_equivalence(db, env, map, self, other);
    }

    fn add_sequents_with_typevar_range(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarRangeBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `T = α` and `T ≤ U`, `α ≤ U` must also hold.
        if self.typevar.is_same_typevar_as(db, other.left) {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteLowerBound::new(provenance, other.right, self.bound);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // Given constraints `T = α` and `U ≤ T`, `U ≤ α` must also hold.
        if self.typevar.is_same_typevar_as(db, other.right) {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteUpperBound::new(provenance, other.left, self.bound);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // We can infer sequents from `T = α` and `S ≤ U` if α _contains_ S or U.
        Constraint::add_covariant_lower_weakened_sequent(db, env, map, self, other);
        Constraint::add_covariant_upper_weakened_sequent(db, env, map, self, other);
        Constraint::add_contravariant_lower_weakened_sequent(db, env, map, self, other);
        Constraint::add_contravariant_upper_weakened_sequent(db, env, map, self, other);
    }

    fn add_sequents_with_typevar_equivalence(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `T = α` and `T = U`, `U = α` must also hold.
        let other_typevar = if self.typevar.is_same_typevar_as(db, other.left) {
            Some(other.right)
        } else if self.typevar.is_same_typevar_as(db, other.right) {
            Some(other.left)
        } else {
            None
        };
        if let Some(other_typevar) = other_typevar {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = ConcreteEquivalenceBound::new(provenance, other_typevar, self.bound);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // We can infer sequents from `T = α` and `S = U` if α _contains_ U.
        Constraint::add_covariant_equivalence_weakened_sequent(
            db,
            env,
            map,
            self,
            other.forwards(),
        );
        Constraint::add_covariant_equivalence_weakened_sequent(
            db,
            env,
            map,
            self,
            other.backwards(),
        );
        Constraint::add_contravariant_equivalence_weakened_sequent(
            db,
            env,
            map,
            self,
            other.forwards(),
        );
        Constraint::add_contravariant_equivalence_weakened_sequent(
            db,
            env,
            map,
            self,
            other.backwards(),
        );
        Constraint::add_invariant_weakened_sequent(db, env, map, self, other.forwards());
        Constraint::add_invariant_weakened_sequent(db, env, map, self, other.backwards());
    }
}

impl<'db> TypeVarRangeBound<'db> {
    fn add_sequents(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
    ) {
        // `T ≤ T` is always true
        if self.left.is_same_typevar_as(db, self.right) {
            map.add_single_tautology(self.into());
        }
    }

    fn add_sequents_with_typevar_range(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarRangeBound<'db>,
        _reversed: bool,
    ) {
        // `S ≤ T` and `T ≤ S` implies `S = T`
        if self.left.is_same_typevar_as(db, other.right)
            && self.right.is_same_typevar_as(db, other.left)
        {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = TypeVarEquivalenceBound::new(db, provenance, self.left, self.right);
            map.add_pair_implication(self.into(), other.into(), derived.into());
            return;
        }

        // Given constraints `S ≤ T` and `T ≤ U`, `S ≤ U` must also hold.
        let (left, right) = if self.right.is_same_typevar_as(db, other.left) {
            (self.left, other.right)
        } else if self.left.is_same_typevar_as(db, other.right) {
            (other.left, self.right)
        } else {
            return;
        };

        let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
        let derived = TypeVarRangeBound::new(db, provenance, left, right);
        map.add_pair_implication(self.into(), other.into(), derived.into());
    }

    fn add_sequents_with_typevar_equivalence(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `S ≤ T` and `T = U`, `S ≤ U` must also hold.
        let replacement = if self.right.is_same_typevar_as(db, other.left) {
            Some(other.right)
        } else if self.right.is_same_typevar_as(db, other.right) {
            Some(other.left)
        } else {
            None
        };
        if let Some(replacement) = replacement {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = TypeVarRangeBound::new(db, provenance, self.left, replacement);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // Given constraints `S ≤ T` and `R = S`, `R ≤ T` must also hold.
        let replacement = if self.left.is_same_typevar_as(db, other.left) {
            Some(other.right)
        } else if self.left.is_same_typevar_as(db, other.right) {
            Some(other.left)
        } else {
            None
        };
        if let Some(replacement) = replacement {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = TypeVarRangeBound::new(db, provenance, replacement, self.right);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }
    }
}

impl<'db> TypeVarEquivalenceBound<'db> {
    fn add_sequents(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
    ) {
        // `T = T` is always true
        if self.left.is_same_typevar_as(db, self.right) {
            map.add_single_tautology(self.into());
        }
    }

    fn add_sequents_with_typevar_equivalence(
        self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        map: &mut SequentMap<Constraint<'db>>,
        other: TypeVarEquivalenceBound<'db>,
        _reversed: bool,
    ) {
        // Given constraints `S = T` and `T = U`, `S = U` must also hold.
        let replacement = if self.right.is_same_typevar_as(db, other.left) {
            Some(other.right)
        } else if self.right.is_same_typevar_as(db, other.right) {
            Some(other.left)
        } else {
            None
        };
        if let Some(replacement) = replacement {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = TypeVarEquivalenceBound::new(db, provenance, self.left, replacement);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }

        // Given constraints `S = T` and `R = S`, `R = T` must also hold.
        let replacement = if self.left.is_same_typevar_as(db, other.left) {
            Some(other.right)
        } else if self.left.is_same_typevar_as(db, other.right) {
            Some(other.left)
        } else {
            None
        };
        if let Some(replacement) = replacement {
            let provenance = ConstraintProvenance::derived(self.provenance, other.provenance);
            let derived = TypeVarEquivalenceBound::new(db, provenance, replacement, self.right);
            map.add_pair_implication(self.into(), other.into(), derived.into());
        }
    }
}

impl<'db> Type<'db> {
    /// Returns whether this type can participate in a transitive sequent proof.
    ///
    /// Gradual assignability is not transitive, so constraints with dynamic bounds are ineligible.
    /// Note that we can't use [`is_fully_static`][Type::is_fully_static] here, since that
    /// considers the declared bounds/constraints of typevars. In the context of a sequent map,
    /// typevars are opaque symbolic atoms: considering their bounds or defaults could incorrectly
    /// make their eligibility depend on a specialization that the sequent is meant to constrain.
    fn is_static_sequent_eligible(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> bool {
        struct EligibilityVisitor<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            seen: TypeCollector<'db>,
            eligible: Cell<bool>,
        }

        impl<'db> TypeVisitor<'db> for EligibilityVisitor<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }

            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                if !self.eligible.get() || ty.is_type_var() {
                    return;
                }
                if ty.is_dynamic() {
                    self.eligible.set(false);
                    return;
                }
                walk_type_with_recursion_guard(db, ty, self, &self.seen);
            }
        }

        let visitor = EligibilityVisitor {
            env,
            seen: TypeCollector::default(),
            eligible: Cell::new(true),
        };
        visitor.visit_type(db, self);
        visitor.eligible.get()
    }
}

impl<'db> ConstraintSetStorage<'db> {
    /// Returns how much sequent fuel is needed to derive this constraint.
    ///
    /// This cost is driven by two factors.
    ///
    /// First, nested types containing typevars can produce increasingly complex families of
    /// derived constraints. Charge more fuel for those constraints so that each additional level
    /// of typevar depth shortens the remaining derivation chain.
    ///
    /// Second, even without considering typevars, the lower and upper bounds can become more
    /// structurally complex. We consider a type to be more complex if it has deeper nesting of
    /// type constructors. Each sequent is charged the _increase_ in that complexity between its
    /// antecedents and its consequent. (Measuring growth rather than absolute depth avoids
    /// penalizing a complex concrete bound that is merely propagated unchanged.)
    pub(super) fn sequent_fuel_cost(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraint: ConstraintId,
        antecedent_constructor_depth: u16,
    ) -> u16 {
        let (constructor_depth, typevar_depth) =
            self.cached_constraint_bound_depth(db, env, constraint);
        let constructor_growth = constructor_depth.saturating_sub(antecedent_constructor_depth);
        typevar_depth.max(constructor_growth).saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::tests::{TestDb, setup_db};
    use crate::types::typevar::TypeVarBoundOrConstraints;
    use crate::types::{BoundTypeVarInstance, KnownClass, SubclassOfType, TypeVarVariance};
    use ruff_python_ast::name::Name;

    fn create_typevar<'db>(db: &'db TestDb, name: &'static str) -> BoundTypeVarInstance<'db> {
        BoundTypeVarInstance::synthetic(
            db,
            &db.program_environment(),
            Name::new_static(name),
            TypeVarVariance::Invariant,
        )
    }

    fn known_instance(db: &TestDb, class: KnownClass) -> Type<'_> {
        class.to_instance(db, &db.program_environment())
    }

    #[test]
    fn overlapping_lower_bounds_do_not_skip_nonempty_sequent_map() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let bool = known_instance(db, KnownClass::Bool);
        let u = create_typevar(db, "U")
            .map_bound_or_constraints(db, |_| Some(TypeVarBoundOrConstraints::UpperBound(bool)));
        let type_of_u = SubclassOfType::from(db, &env, u);
        let bool_class = KnownClass::Bool.to_class_literal(db, &env);
        let left = Constraint::from(ConcreteLowerBound::new(
            ConstraintProvenance::Evidence,
            t,
            type_of_u,
        ));
        let right = Constraint::from(ConcreteLowerBound::new(
            ConstraintProvenance::Evidence,
            t,
            bool_class,
        ));

        for (left, right) in [(left, right), (right, left)] {
            let sequents = SequentMap::<Constraint>::for_constraint_pair(db, &env, left, right);

            assert!(
                sequents
                    .sequents
                    .iter()
                    .any(|sequent| matches!(sequent, Sequent::SingleImplication { .. }))
            );
            assert!(!SequentMap::<Constraint>::pair_cannot_produce_sequents(
                db, &env, left, right
            ));
        }
    }

    #[test]
    fn ground_leaf_can_tighten_nested_lower_bound_without_enabling_deepening() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let s = create_typevar(db, "S");
        let t = create_typevar(db, "T");
        let int = known_instance(db, KnownClass::Int);

        let lower = |typevar, bound| {
            Constraint::from(ConcreteLowerBound::new(
                ConstraintProvenance::Evidence,
                typevar,
                bound,
            ))
        };
        let produces = |map: &SequentMap<Constraint<'_>>, expected| {
            map.sequents.iter().any(|sequent| {
                matches!(
                    sequent,
                    Sequent::PairImplication {
                        post: Constraint::ConcreteLower(post),
                        ..
                    } if post.typevar.is_same_typevar_as(db, t) && post.bound == expected
                )
            })
        };

        let iterator_s =
            KnownClass::Iterator.to_specialized_instance(db, &env, &[Type::TypeVar(s)]);
        let iterator_int = KnownClass::Iterator.to_specialized_instance(db, &env, &[int]);
        let map = SequentMap::<Constraint>::for_constraint_pair(
            db,
            &env,
            lower(s, int),
            lower(t, iterator_s),
        );
        assert!(produces(map, iterator_int));

        let list_s = KnownClass::List.to_specialized_instance(db, &env, &[Type::TypeVar(s)]);
        let list_int = KnownClass::List.to_specialized_instance(db, &env, &[int]);
        let list_list_int = KnownClass::List.to_specialized_instance(db, &env, &[list_int]);
        let map = SequentMap::<Constraint>::for_constraint_pair(
            db,
            &env,
            lower(s, list_int),
            lower(t, list_s),
        );
        assert!(!produces(map, list_list_int));
    }
}
