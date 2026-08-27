//! The [`SequentMap`] and related functionality

use std::cell::Cell;
use std::fmt::{Debug, Display};

use smallvec::SmallVec;

use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintBound, ConstraintBounds, ConstraintId,
    ConstraintSetBuilder, ConstraintSetStorage, IntersectionResult, Node,
};
use crate::types::typevar::TypeVarSet;
use crate::types::variance::VarianceInferable;
use crate::types::visitor::{
    TypeCollector, TypeVisitor, any_over_type, walk_type_with_recursion_guard,
};
use crate::types::{BoundTypeVarInstance, Type, TypeVarVariance};
use crate::{Db, ProgramEnvironment};

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
#[derive(Debug, Default)]
pub(super) struct SequentMap {
    pub(super) sequents: Vec<Sequent>,
}

/// Describes one rule for deriving new implicit constraints from existing constraints in a BDD
/// path.
#[derive(Clone, Copy, Debug)]
pub(super) enum Sequent {
    /// Sequent of the form `¬C → false`
    ///
    /// This indicates that `C` is always true. Any path that assumes it is false is impossible and
    /// can be pruned.
    SingleTautology { ante: ConstraintId },

    /// Sequent of the form `C₁ ∧ C₂ → false`
    ///
    /// This indicates that `C₁` and `C₂` are disjoint: it is not possible for both to hold. Any
    /// path that assumes both is impossible and can be pruned.
    PairImpossibility {
        ante1: ConstraintId,
        ante2: ConstraintId,
    },

    /// Sequent of the form `C → D`
    ///
    /// This indicates that `C` on its own is enough to imply `D`. For any path that assumes `C`
    /// holds, we can add `D` to the path even if it doesn't appear in the BDD.
    SingleImplication {
        ante: ConstraintId,
        post: ConstraintId,
    },

    /// Sequent of the form `C₁ ∧ C₂ → D`
    ///
    /// This indicates that if `C₁` and `C₂` are both true, then `D` is guaranteed to be true as
    /// well. For any path that assumes both `C₁` and `C₂` hold, we can add `D` to the path even if
    /// it doesn't appear in the BDD.
    PairImplication {
        ante1: ConstraintId,
        ante2: ConstraintId,
        post: ConstraintId,
    },
}

impl SequentMap {
    /// Returns a sequent map containing the sequents that we can infer from a single constraint in
    /// isolation. This method is salsa-tracked so that we only perform this work once per
    /// constraint.
    pub(super) fn for_constraint<'db, 'c>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &'c mut ConstraintSetStorage<'db>,
        constraint: ConstraintId,
    ) -> &'c Self {
        let key = constraint;
        if !storage.single_sequent_cache.contains_key(&key) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                constraint = %constraint.display(db, env, storage),
                "add sequents for constraint",
            );
            let mut map = SequentMap::default();
            map.add_sequents_for_single(db, env, storage, constraint);
            storage.single_sequent_cache.insert(key, map);
        }
        &storage.single_sequent_cache[&key]
    }

    /// Returns a sequent map containing the sequents that we can infer from a pair of constraints.
    /// This method is salsa-tracked so that we only perform this work once per constraint pair.
    ///
    /// (Note that this method is _not_ commutative; you should provide `left` and `right` in the
    /// order that they appear in the source code, so that we can construct derived constraints
    /// that retain that ordering.)
    pub(super) fn for_constraint_pair<'db, 'c>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &'c mut ConstraintSetStorage<'db>,
        left: ConstraintId,
        right: ConstraintId,
    ) -> &'c Self {
        let key = (left, right);
        if !storage.pair_sequent_cache.contains_key(&key) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left.display(db, env, storage),
                right = %right.display(db, env, storage),
                "add sequents for constraint pair",
            );
            let mut map = SequentMap::default();
            map.add_sequents_for_pair(db, env, storage, left, right);
            storage.pair_sequent_cache.insert(key, map);
        }
        &storage.pair_sequent_cache[&key]
    }

    /// Quickly determines whether two constraints cannot possibly produce any sequents when passed
    /// to [`for_constraint_pair`][Self::for_constraint_pair]. If this returns `true`, it is safe
    /// to skip calling `for_constraint_pair` for this pair of constraints.
    pub(super) fn pair_cannot_produce_sequents<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        left: ConstraintId,
        right: ConstraintId,
    ) -> bool {
        // Currently, the only pattern we look for is when two constraints that have _only_ lower
        // bounds, where those lower bounds are disjoint. Given `l₁ ≤ T ∧ l₂ ≤ T`, the only
        // sequent we could theoretically produce is `(l₁ | l₂) ≤ T`. But we don't store that as a
        // single constraint; we always break that apart into the two smaller constraints that we
        // started with.

        let left = storage.constraint_data(left);
        let right = storage.constraint_data(right);
        if !left.typevar.is_same_typevar_as(db, right.typevar) {
            return false;
        }

        let (Some(left_lower), Some(right_lower)) = (left.bounds.lower, right.bounds.lower) else {
            return false;
        };
        if left.bounds.upper.is_some() || right.bounds.upper.is_some() {
            return false;
        }
        let left_lower = left_lower.ty();
        let right_lower = right_lower.ty();

        // This call might need its own borrow of the builder's storage, so create a new builder
        // that it can use.
        let builder = ConstraintSetBuilder::new();
        left_lower
            .when_trivially_disjoint_from(db, env, right_lower, &builder, TypeVarSet::None)
            .is_trivially_always_satisfied()
    }

    fn add_single_tautology(&mut self, ante: ConstraintId) {
        self.sequents.push(Sequent::SingleTautology { ante });
    }

    fn add_pair_impossibility(&mut self, ante1: ConstraintId, ante2: ConstraintId) {
        self.sequents
            .push(Sequent::PairImpossibility { ante1, ante2 });
    }

    fn add_pair_implication<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        ante1: ConstraintId,
        ante2: ConstraintId,
        post: ConstraintId,
    ) {
        // If the post constraint is unsatisfiable, then the antecedents contradict each other.
        let post_data = storage.constraint_data(post);
        let post_lower = post_data.bounds.lower_bound().ty();
        let post_upper = post_data.bounds.upper_bound().ty();
        let (when, source_order) = storage.load(
            db,
            env,
            &post_lower.when_constraint_set_assignable_to_owned(db, env, post_upper),
        );
        if when.is_never_satisfied(db, env, storage, source_order) {
            self.add_pair_impossibility(ante1, ante2);
            return;
        }

        // If either antecedent implies the consequent on its own, this new sequent is redundant.
        if ante1.implies(db, env, storage, post) || ante2.implies(db, env, storage, post) {
            return;
        }

        self.sequents
            .push(Sequent::PairImplication { ante1, ante2, post });
    }

    fn add_single_implication(&mut self, ante: ConstraintId, post: ConstraintId) {
        if ante == post {
            return;
        }

        self.sequents
            .push(Sequent::SingleImplication { ante, post });
    }

    fn add_sequents_for_single<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        constraint: ConstraintId,
    ) {
        // If this constraint binds its typevar to `Never ≤ T ≤ object`, then the typevar can take
        // on any type, and the constraint is always satisfied.
        let constraint_data = storage.constraint_data(constraint);
        let lower = constraint_data.bounds.lower_bound().ty();
        let upper = constraint_data.bounds.upper_bound().ty();
        if lower.is_never() && upper.is_object() {
            self.add_single_tautology(constraint);
            return;
        }

        // Given a constraint `L ≤ T ≤ U`, `L ≤ U` must also hold. If those bounds contain other
        // typevars, we can infer additional constraints. This is easiest to see when the bounds
        // _are_ typevars:
        //
        //   1. `(S ≤ T ≤ U) → (S ≤ U)`
        //   2. `(S ≤ T ≤ τ) → (S ≤ τ)`
        //   3. `(τ ≤ T ≤ U) → (τ ≤ U)`
        //
        // but it also holds when the bounds _contain_ typevars:
        //
        //   4. `(Covariant[S] ≤ T ≤ Covariant[U]) → (S ≤ U)`
        //      `(Covariant[S] ≤ T ≤ Covariant[τ]) → (S ≤ τ)`
        //      `(Covariant[τ] ≤ T ≤ Covariant[U]) → (τ ≤ U)`
        //
        //   5. `(Contravariant[S] ≤ T ≤ Contravariant[U]) → (U ≤ S)`
        //      `(Contravariant[S] ≤ T ≤ Contravariant[τ]) → (τ ≤ S)`
        //      `(Contravariant[τ] ≤ T ≤ Contravariant[U]) → (U ≤ τ)`
        //
        //   6. `(Invariant[S] ≤ T ≤ Invariant[U]) → (S = U)`
        //      `(Invariant[S] ≤ T ≤ Invariant[τ]) → (S = τ)`
        //      `(Invariant[τ] ≤ T ≤ Invariant[U]) → (τ = U)`
        //
        // and whenever the bounds are assignable, even if they don't mention exactly the same
        // types:
        //
        //   class Sub(Covariant[int]): ...
        //
        //   7. `(Covariant[S] ≤ T ≤ Sub) → (S ≤ int)`
        //      `(Sub ≤ T ≤ Covariant[U]) → (int ≤ U)`
        //
        // To handle all of these cases, we perform a constraint set assignability check to see
        // when `L ≤ U`. This gives us a constraint set, which should be the rhs of the sequent
        // implication. (That is, this check directly encodes `(L ≤ T ≤ U) → (L ≤ U)` as an
        // implication.)

        // Skip trivial cases where the assignability check won't produce useful results.
        if lower.is_never() || upper.is_object() {
            return;
        }

        let (when, source_order) = storage.load(
            db,
            env,
            &lower.when_constraint_set_assignable_to_owned(db, env, upper),
        );

        // If L is _never_ assignable to U, this constraint would violate transitivity, and should
        // never have been added.
        #[expect(clippy::debug_assert_with_mut_call)]
        {
            debug_assert!(!when.is_never_satisfied(db, env, storage, source_order));
        }

        // Fast path: If L is trivially always assignable to U, there are no derived constraints
        // that we can infer. This would be handled correctly by the logic below, but this is a
        // useful early return. Since we only use this check as an early return happy path, we can
        // accept false negatives. That lets us use the simpler and cheaper check against
        // ALWAYS_TRUE, rather than a more expensive is_always_satisfiable call.
        if when == ALWAYS_TRUE {
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
        let mut node = when;
        if !node.is_single_conjunction(storage) {
            return;
        }

        loop {
            match node.node() {
                Node::AlwaysTrue | Node::AlwaysFalse => break,
                Node::Interior(interior) => {
                    let interior = storage.interior_node_data(interior.node());
                    let derived = storage.constraint_data(interior.constraint);
                    let derived = ConstraintId::new_with_bounds(
                        db,
                        env,
                        storage,
                        derived.typevar,
                        derived
                            .bounds
                            .lower
                            .map(|bound| bound.with_source_provenance(constraint_data.bounds)),
                        derived
                            .bounds
                            .upper
                            .map(|bound| bound.with_source_provenance(constraint_data.bounds)),
                    );
                    if interior.if_true != ALWAYS_FALSE {
                        self.add_single_implication(constraint, derived);
                        node = interior.if_true;
                    } else {
                        self.add_pair_impossibility(constraint, derived);
                        node = interior.if_false;
                    }
                }
            }
        }
    }

    fn add_sequents_for_pair<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        // If either of the constraints has another typevar as a lower/upper bound, the only
        // sequents we can add are for the transitive closure. For instance, if we have
        // `(S ≤ T) ∧ (T ≤ int)`, then `(S ≤ int)` will also hold, and we should add a sequent for
        // this implication. These are the `mutual_sequents` mentioned below — sequents that come
        // about because two typevars are mutually constrained.
        //
        // Complicating things is that `(S ≤ T)` will be encoded differently depending on how `S`
        // and `T` compare in our arbitrary BDD variable ordering.
        //
        // When `S` comes before `T`, `(S ≤ T)` will be encoded as `(Never ≤ S ≤ T)`, and the
        // overall antecedent will be `(Never ≤ S ≤ T) ∧ (T ≤ int)`. Those two individual
        // constraints constrain different typevars (`S` and `T`, respectively), and are handled by
        // `add_mutual_sequents_for_different_typevars`.
        //
        // When `T` comes before `S`, `(S ≤ T)` will be encoded as `(S ≤ T ≤ object)`, and the
        // overall antecedent will be `(S ≤ T ≤ object) ∧ (T ≤ int)`. Those two individual
        // constraints both constrain `T`, and are handled by
        // `add_mutual_sequents_for_same_typevars`.
        //
        // If all of the lower and upper bounds are concrete (i.e., not typevars), then there
        // several _other_ sequents that we can add, as handled by `add_concrete_sequents`.
        let left_constraint_data = storage.constraint_data(left_constraint);
        let left_typevar = left_constraint_data.typevar;
        let right_constraint_data = storage.constraint_data(right_constraint);
        let right_typevar = right_constraint_data.typevar;

        if !left_typevar.is_same_typevar_as(db, right_typevar) {
            self.add_mutual_sequents_for_different_typevars(
                db,
                env,
                storage,
                left_constraint,
                right_constraint,
            );
            self.add_nested_typevar_sequents(db, env, storage, left_constraint, right_constraint);
        } else if left_constraint_data.bounds.lower_bound().ty().is_type_var()
            || left_constraint_data.bounds.upper_bound().ty().is_type_var()
            || right_constraint_data
                .bounds
                .lower_bound()
                .ty()
                .is_type_var()
            || right_constraint_data
                .bounds
                .upper_bound()
                .ty()
                .is_type_var()
        {
            self.add_mutual_sequents_for_same_typevars(
                db,
                env,
                storage,
                left_constraint,
                right_constraint,
            );
        } else {
            self.add_concrete_sequents(db, env, storage, left_constraint, right_constraint);
        }
    }

    fn add_mutual_sequents_for_different_typevars<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        // We've structured our constraints so that a typevar's upper/lower bound can only
        // be another typevar if the bound is "later" in our arbitrary ordering. That means
        // we only have to check this pair of constraints in one direction — though we do
        // have to figure out which of the two typevars is constrained, and which one is
        // the upper/lower bound.
        let left_constraint_data = storage.constraint_data(left_constraint);
        let left_typevar = left_constraint_data.typevar;
        let right_constraint_data = storage.constraint_data(right_constraint);
        let right_typevar = right_constraint_data.typevar;
        let (bound_constraint, constrained_constraint) =
            if left_typevar.can_be_bound_for(db, storage, right_typevar) {
                (left_constraint, right_constraint)
            } else {
                (right_constraint, left_constraint)
            };

        // We then look for cases where the "constrained" typevar's upper and/or lower bound
        // matches the "bound" typevar. If so, we're going to add an implication sequent that
        // replaces the upper/lower bound that matched with the bound constraint's corresponding
        // bound.
        let bound_constraint_data = storage.constraint_data(bound_constraint);
        let bound_typevar = bound_constraint_data.typevar;
        let constrained_constraint_data = storage.constraint_data(constrained_constraint);
        let constrained_typevar = constrained_constraint_data.typevar;
        let constrained_lower_bound = constrained_constraint_data.bounds.lower_bound();
        let constrained_upper_bound = constrained_constraint_data.bounds.upper_bound();
        let bound_lower_bound = bound_constraint_data.bounds.lower_bound();
        let bound_upper_bound = bound_constraint_data.bounds.upper_bound();

        // Transitive pivots require subtyping; classes with dynamic bases can be assignable to
        // unrelated types without being subtypes.
        let (new_lower, new_upper) = match (
            constrained_lower_bound.ty(),
            constrained_upper_bound.ty(),
            bound_lower_bound.ty(),
            bound_upper_bound.ty(),
        ) {
            // (B ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ BU)
            (Type::TypeVar(constrained_lower), Type::TypeVar(constrained_upper), _, _)
                if constrained_lower.is_same_typevar_as(db, bound_typevar)
                    && constrained_upper.is_same_typevar_as(db, bound_typevar) =>
            {
                (
                    ConstraintBound::from_transitive_derivation(
                        bound_lower_bound.ty(),
                        constrained_lower_bound,
                        bound_lower_bound,
                    ),
                    ConstraintBound::from_transitive_derivation(
                        bound_upper_bound.ty(),
                        constrained_upper_bound,
                        bound_upper_bound,
                    ),
                )
            }

            // (CL ≤ C ≤ B) ∧ (BL ≤ B ≤ BU) → (CL ≤ C ≤ BU)
            (_, Type::TypeVar(constrained_upper), _, _)
                if constrained_upper.is_same_typevar_as(db, bound_typevar) =>
            {
                (
                    constrained_lower_bound,
                    ConstraintBound::from_transitive_derivation(
                        bound_upper_bound.ty(),
                        constrained_upper_bound,
                        bound_upper_bound,
                    ),
                )
            }

            // (B ≤ C ≤ CU) ∧ (BL ≤ B ≤ BU) → (BL ≤ C ≤ CU)
            (Type::TypeVar(constrained_lower), _, _, _)
                if constrained_lower.is_same_typevar_as(db, bound_typevar) =>
            {
                (
                    ConstraintBound::from_transitive_derivation(
                        bound_lower_bound.ty(),
                        constrained_lower_bound,
                        bound_lower_bound,
                    ),
                    constrained_upper_bound,
                )
            }

            // (CL ≤ C ≤ pivot) ∧ (pivot ≤ B ≤ BU) → (CL ≤ C ≤ B)
            (_, constrained_upper, bound_lower, _)
                if !constrained_upper.is_never()
                    && !constrained_upper.is_object()
                    && storage.cached_is_constraint_set_subtype_of(
                        db,
                        env,
                        constrained_upper.top_materialization(db, env),
                        bound_lower.bottom_materialization(db, env),
                    ) =>
            {
                (
                    constrained_lower_bound,
                    ConstraintBound::from_transitive_derivation(
                        Type::TypeVar(bound_typevar),
                        constrained_upper_bound,
                        bound_lower_bound,
                    ),
                )
            }

            // (pivot ≤ C ≤ CU) ∧ (BL ≤ B ≤ pivot) → (B ≤ C ≤ CU)
            (constrained_lower, _, _, bound_upper)
                if !constrained_lower.is_never()
                    && !constrained_lower.is_object()
                    && storage.cached_is_constraint_set_subtype_of(
                        db,
                        env,
                        bound_upper.top_materialization(db, env),
                        constrained_lower.bottom_materialization(db, env),
                    ) =>
            {
                (
                    ConstraintBound::from_transitive_derivation(
                        Type::TypeVar(bound_typevar),
                        constrained_lower_bound,
                        bound_upper_bound,
                    ),
                    constrained_upper_bound,
                )
            }

            _ => return,
        };

        let mut post_constraints: SmallVec<[ConstraintId; 3]> = SmallVec::new();
        // These are derived logical constraints, not direct inference evidence. Avoid preserving
        // explicit bounds that are equivalent to missing lower/upper bounds, so a derived
        // `T ≤ U ≤ object` can satisfy a later query for `T ≤ U` without requiring a separate
        // materialized-default implication.
        let mut constrained_lower = (!new_lower.ty().is_never()).then_some(new_lower);
        let mut constrained_upper = (!new_upper.ty().is_object()).then_some(new_upper);

        // The transitive rule above gives us an intended post-condition
        // `new_lower ≤ [constrained] ≤ new_upper`.
        //
        // If a top-level bound typevar is "earlier" than `constrained`, we cannot represent that
        // directly as a bound on `constrained` without violating our canonical ordering.
        // Instead, split it into equivalent canonical constraints by "moving" that bound onto the
        // other typevar:
        //
        //   invalid lower  `L ≤ [C]`  ->  `(Never ≤ [L] ≤ C)` and drop `L` from C's lower bound
        //   invalid upper  `[C] ≤ U`  ->  `(C ≤ [U] ≤ object)` and drop `U` from C's upper bound
        //
        // Example: if we derive `[A] ≤ T ≤ [B]` but `A`/`B` are not valid top-level bounds for
        // `T` in this ordering, we emit two pair implications:
        //   `(Never ≤ [A] ≤ T)` and `(T ≤ [B] ≤ object)`.
        // This preserves the relationship while keeping all derived constraints canonical.
        if let Type::TypeVar(lower_bound_typevar) = new_lower.ty()
            && !lower_bound_typevar.can_be_bound_for(db, storage, constrained_typevar)
        {
            post_constraints.push(ConstraintId::new_with_bounds(
                db,
                env,
                storage,
                lower_bound_typevar,
                None,
                Some(new_lower.with_type(Type::TypeVar(constrained_typevar))),
            ));
            constrained_lower = None;
        }

        if let Type::TypeVar(upper_bound_typevar) = new_upper.ty()
            && !upper_bound_typevar.can_be_bound_for(db, storage, constrained_typevar)
        {
            post_constraints.push(ConstraintId::new_with_bounds(
                db,
                env,
                storage,
                upper_bound_typevar,
                Some(new_upper.with_type(Type::TypeVar(constrained_typevar))),
                None,
            ));
            constrained_upper = None;
        }

        if constrained_lower.is_some() || constrained_upper.is_some() {
            post_constraints.push(ConstraintId::new_with_bounds(
                db,
                env,
                storage,
                constrained_typevar,
                constrained_lower,
                constrained_upper,
            ));
        }

        for post_constraint in post_constraints {
            self.add_pair_implication(
                db,
                env,
                storage,
                left_constraint,
                right_constraint,
                post_constraint,
            );
        }
    }

    /// Adds sequents for the case where one constraint's lower or upper bound contains another
    /// constraint's typevar nested inside a parameterized type (e.g., `U ≤ Covariant[T]`).
    ///
    /// This is distinct from `add_mutual_sequents_for_different_typevars`, which handles the case
    /// where a typevar appears _directly_ as a top-level lower/upper bound (e.g., `U ≤ T`). A
    /// bare `Type::TypeVar` is technically a special case of covariant nesting (since the variance
    /// of `T` in `T` itself is covariant), but the existing direct-typevar logic handles it
    /// separately because it requires careful canonical ordering of typevar-to-typevar constraints
    /// that the generic nested-typevar logic here does not need to worry about.
    fn add_nested_typevar_sequents<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        // Keep this precheck aligned with `variance_of`, which visits lazy types.
        let has_typevar_bound = |bounds: ConstraintBounds<'db>| {
            bounds
                .lower
                .is_some_and(|lower| any_over_type(db, env, lower.ty(), true, Type::is_type_var))
                || bounds.upper.is_some_and(|upper| {
                    any_over_type(db, env, upper.ty(), true, Type::is_type_var)
                })
        };
        if !has_typevar_bound(storage.constraint_data(left_constraint).bounds)
            && !has_typevar_bound(storage.constraint_data(right_constraint).bounds)
        {
            return;
        }

        let mut try_tightening =
            |bound_constraint: ConstraintId, constrained_constraint: ConstraintId| {
                let bound_data = storage.constraint_data(bound_constraint);
                let bound_typevar = bound_data.typevar;
                let bound_identity = bound_typevar.identity(db);
                let bound_lower_bound = bound_data.bounds.lower_bound();
                let bound_upper_bound = bound_data.bounds.upper_bound();
                let constrained_data = storage.constraint_data(constrained_constraint);
                let constrained_typevar = constrained_data.typevar;
                let constrained_identity = constrained_typevar.identity(db);
                let constrained_lower_bound = constrained_data.bounds.lower_bound();
                let constrained_upper_bound = constrained_data.bounds.upper_bound();
                let constrained_lower = constrained_lower_bound.ty();
                let constrained_upper = constrained_upper_bound.ty();

                // If the replacement contains the bound typevar itself (e.g., the bound
                // constraint is `_V ≤ G[_V]`), or the constrained typevar (e.g., the bound
                // constraint is `_T ≤ G[_V]` and we're about to substitute into `_V ≤ G[_T]`),
                // substituting would create a deeper nesting of the same recursive pattern
                // that triggers the same substitution again ad infinitum. Skip in both cases.
                //
                // Fast-path bare typevar replacements (`Type::TypeVar`) using equality checks
                // instead of calling `variance_of` on them. This avoids a large number of tiny
                // tracked `variance_of` queries in hot paths.
                let replacement_mentions_bound_or_constrained = |replacement: Type<'db>| {
                    replacement.variance_of(db, env, bound_identity) != TypeVarVariance::Bivariant
                        || replacement.variance_of(db, env, constrained_identity)
                            != TypeVarVariance::Bivariant
                };

                // Check the upper bound of the constrained constraint for nested occurrences of
                // the bound typevar. We use `variance_of` as our combined presence + variance
                // check: `Bivariant` means the typevar doesn't appear in the type (or is genuinely
                // bivariant, which is semantically equivalent — no implication is needed in either
                // case).
                //
                // Note: if `Bivariant` is ever removed from the `TypeVarVariance` enum, we would
                // need an alternative representation for "typevar not present"
                // (e.g., `Option<TypeVarVariance>`).
                let upper_replacement = match (
                    constrained_upper.variance_of(db, env, bound_identity),
                    bound_lower_bound.ty(),
                    bound_upper_bound.ty(),
                ) {
                    (TypeVarVariance::Bivariant, _, _) => None,
                    // Skip bare typevars — those are handled by
                    // `add_mutual_sequents_for_different_typevars`.
                    _ if constrained_upper.is_type_var() => None,
                    // Covariance preserves direction: upper bound on T substitutes into upper
                    // bound. A ≤ B → G[A] ≤ G[B], so (T ≤ u_B) gives G[T] ≤ G[u_B].
                    (TypeVarVariance::Covariant, _, bound_upper) if !bound_upper.is_object() => {
                        Some(bound_upper_bound)
                    }
                    // Contravariance flips direction: lower bound on T substitutes into upper
                    // bound. A ≤ B → G[B] ≤ G[A], so (l_B ≤ T) gives G[T] ≤ G[l_B].
                    (TypeVarVariance::Contravariant, bound_lower, _) if !bound_lower.is_never() => {
                        Some(bound_lower_bound)
                    }
                    // Invariance requires equality: only substitute if l_B = u_B.
                    (TypeVarVariance::Invariant, bound_lower, bound_upper)
                        if bound_lower == bound_upper && !bound_lower.is_never() =>
                    {
                        Some(ConstraintBound::from_combination(
                            bound_lower,
                            bound_lower_bound,
                            bound_upper_bound,
                        ))
                    }
                    _ => None,
                };
                let upper_replacement = upper_replacement.filter(|replacement| {
                    // Substituting one typevar for another into large unions can generate many
                    // very-weak derived constraints and cause severe performance regressions.
                    // Keep the common/non-union case enabled; skip union upper bounds for this
                    // specific typevar-to-typevar replacement shape.
                    if replacement.ty().is_type_var() && constrained_upper.is_union() {
                        return false;
                    }
                    !replacement_mentions_bound_or_constrained(replacement.ty())
                });
                if let Some(replacement) = upper_replacement {
                    let new_upper = constrained_upper.substitute_one_typevar(
                        db,
                        env,
                        bound_typevar,
                        replacement.ty(),
                    );
                    if new_upper != constrained_upper {
                        let post = ConstraintId::new_with_bounds(
                            db,
                            env,
                            storage,
                            constrained_typevar,
                            constrained_data.bounds.lower,
                            Some(ConstraintBound::from_transitive_derivation(
                                new_upper,
                                constrained_upper_bound,
                                replacement,
                            )),
                        );
                        self.add_pair_implication(
                            db,
                            env,
                            storage,
                            bound_constraint,
                            constrained_constraint,
                            post,
                        );
                    }
                }

                // Check the lower bound of the constrained constraint for nested occurrences.
                let lower_replacement = match (
                    constrained_lower.variance_of(db, env, bound_identity),
                    bound_lower_bound.ty(),
                    bound_upper_bound.ty(),
                ) {
                    (TypeVarVariance::Bivariant, _, _) => None,
                    _ if constrained_lower.is_type_var() => None,
                    // Covariance preserves direction: lower bound on T substitutes into lower
                    // bound. A ≤ B → G[A] ≤ G[B], so (l_B ≤ T) gives G[l_B] ≤ G[T].
                    (TypeVarVariance::Covariant, bound_lower, _) if !bound_lower.is_never() => {
                        Some(bound_lower_bound)
                    }
                    // Contravariance flips direction: upper bound on T substitutes into lower
                    // bound. A ≤ B → G[B] ≤ G[A], so (T ≤ u_B) gives G[u_B] ≤ G[T].
                    (TypeVarVariance::Contravariant, _, bound_upper)
                        if !bound_upper.is_object() =>
                    {
                        Some(bound_upper_bound)
                    }
                    // Invariance requires equality: only substitute if l_B = u_B.
                    (TypeVarVariance::Invariant, bound_lower, bound_upper)
                        if bound_lower == bound_upper && !bound_lower.is_never() =>
                    {
                        Some(ConstraintBound::from_combination(
                            bound_lower,
                            bound_lower_bound,
                            bound_upper_bound,
                        ))
                    }
                    _ => None,
                };
                let lower_replacement = lower_replacement.filter(|replacement| {
                    // Substituting one typevar for another into large intersections can generate
                    // many very-weak derived constraints and cause severe performance regressions.
                    // Keep the common/non-intersection case enabled; skip intersection lower
                    // bounds for this specific typevar-to-typevar replacement shape.
                    if replacement.ty().is_type_var() && constrained_lower.is_intersection() {
                        return false;
                    }
                    !replacement_mentions_bound_or_constrained(replacement.ty())
                });
                if let Some(replacement) = lower_replacement {
                    let new_lower = constrained_lower.substitute_one_typevar(
                        db,
                        env,
                        bound_typevar,
                        replacement.ty(),
                    );
                    if new_lower != constrained_lower {
                        let post = ConstraintId::new_with_bounds(
                            db,
                            env,
                            storage,
                            constrained_typevar,
                            Some(ConstraintBound::from_transitive_derivation(
                                new_lower,
                                constrained_lower_bound,
                                replacement,
                            )),
                            constrained_data.bounds.upper,
                        );
                        self.add_pair_implication(
                            db,
                            env,
                            storage,
                            bound_constraint,
                            constrained_constraint,
                            post,
                        );
                    }
                }
            };

        try_tightening(left_constraint, right_constraint);
        try_tightening(right_constraint, left_constraint);

        // Additionally, check if one constraint's bare typevar *bound* appears nested in the other
        // constraint's bounds. This handles the "dual" direction: instead of substituting a
        // typevar's concrete bounds into another constraint (tightening), we substitute the
        // typevar itself for one of its bare typevar bounds (weakening), creating a cross-typevar
        // link.
        //
        // For example, given `(Covariant[S] ≤ C) ∧ (Never ≤ B ≤ S)`, S is B's upper bound and
        // appears covariantly in C's lower bound. Since `B ≤ S`, covariance tells us that
        // `Covariant[B] ≤ Covariant[S]`. Transitivity then lets us derive `Covariant[B] ≤ C`.
        //
        // The derived constraint is weaker than the original, but it introduces a relationship
        // between B and C that we need to remember and propagate if we ever existentially quantify
        // away S.
        //
        // TODO: This only handles the case where the bound (in this case, S) is a bare typevar. A
        // future extension could handle arbitrary types by pattern-matching on generic alias
        // structure.
        //
        // This is defined as a separate closure because it iterates over the bound constraint's
        // bare typevar bounds, which is a different axis than `try_tightening`'s check on the
        // bound constraint's typevar.
        let mut try_weakening =
            |bound_constraint: ConstraintId, constrained_constraint: ConstraintId| {
                let bound_data = storage.constraint_data(bound_constraint);
                let bound_typevar = bound_data.typevar;
                let bound_lower_bound = bound_data.bounds.lower_bound();
                let bound_upper_bound = bound_data.bounds.upper_bound();
                let bound_lower = bound_lower_bound.ty();
                let constrained_data = storage.constraint_data(constrained_constraint);
                let constrained_typevar = constrained_data.typevar;
                let constrained_lower_bound = constrained_data.bounds.lower_bound();
                let constrained_upper_bound = constrained_data.bounds.upper_bound();
                let constrained_lower = constrained_lower_bound.ty();
                let constrained_upper = constrained_upper_bound.ty();

                let mut try_one_bound = |bound: ConstraintBound<'db>, is_upper_bound: bool| {
                    let Some(nested_typevar) = bound.ty().as_typevar() else {
                        return;
                    };

                    // Skip if the nested typevar is the same as the constrained typevar — that
                    // case is handled by `add_mutual_sequents_for_different_typevars`.
                    if nested_typevar.is_same_typevar_as(db, constrained_typevar)
                        || nested_typevar.is_same_typevar_as(db, bound_typevar)
                    {
                        return;
                    }

                    let replacement = Type::TypeVar(bound_typevar);

                    // Check the constrained constraint's upper bound for nested occurrences of
                    // nested_typevar (S). We want to *weaken* (relax) the upper bound by making it
                    // larger:
                    //   - Covariant + S is B's lower bound (S ≤ B): G[S] ≤ G[B] → weaker. Emit.
                    //   - Contravariant + S is B's upper bound (B ≤ S): G[S] ≤ G[B] → weaker. Emit.
                    //   - Other combinations tighten rather than weaken. Skip.
                    let should_weaken_upper = !constrained_upper.is_type_var()
                        && !constrained_upper.is_never()
                        && !constrained_upper.is_object()
                        && !constrained_upper.is_dynamic()
                        && match constrained_upper.variance_of(db, env, nested_typevar.identity(db))
                        {
                            TypeVarVariance::Bivariant => false,
                            TypeVarVariance::Covariant => !is_upper_bound,
                            TypeVarVariance::Contravariant => is_upper_bound,
                            TypeVarVariance::Invariant => {
                                bound_lower_bound.ty() == bound_upper_bound.ty()
                                    && !bound_lower.is_never()
                            }
                        };
                    if should_weaken_upper {
                        let new_upper = constrained_upper.substitute_one_typevar(
                            db,
                            env,
                            nested_typevar,
                            replacement,
                        );
                        if new_upper != constrained_upper {
                            let post = ConstraintId::new_with_bounds(
                                db,
                                env,
                                storage,
                                constrained_typevar,
                                constrained_data.bounds.lower,
                                Some(ConstraintBound::from_transitive_derivation(
                                    new_upper,
                                    constrained_upper_bound,
                                    bound,
                                )),
                            );
                            self.add_pair_implication(
                                db,
                                env,
                                storage,
                                bound_constraint,
                                constrained_constraint,
                                post,
                            );
                        }
                    }

                    // Ditto for the lower bound.
                    let should_weaken_lower = !constrained_lower.is_type_var()
                        && !constrained_lower.is_never()
                        && !constrained_lower.is_object()
                        && !constrained_lower.is_dynamic()
                        && match constrained_lower.variance_of(db, env, nested_typevar.identity(db))
                        {
                            TypeVarVariance::Bivariant => false,
                            TypeVarVariance::Covariant => is_upper_bound,
                            TypeVarVariance::Contravariant => !is_upper_bound,
                            TypeVarVariance::Invariant => {
                                bound_lower_bound.ty() == bound_upper_bound.ty()
                                    && !bound_lower.is_never()
                            }
                        };
                    if should_weaken_lower {
                        let new_lower = constrained_lower.substitute_one_typevar(
                            db,
                            env,
                            nested_typevar,
                            replacement,
                        );
                        if new_lower != constrained_lower {
                            let post = ConstraintId::new_with_bounds(
                                db,
                                env,
                                storage,
                                constrained_typevar,
                                Some(ConstraintBound::from_transitive_derivation(
                                    new_lower,
                                    constrained_lower_bound,
                                    bound,
                                )),
                                constrained_data.bounds.upper,
                            );
                            self.add_pair_implication(
                                db,
                                env,
                                storage,
                                bound_constraint,
                                constrained_constraint,
                                post,
                            );
                        }
                    }
                };

                // For each bare typevar bound S of the bound constraint, check if S appears
                // nested in the constrained constraint's bounds. If so, we can substitute B
                // (the bound constraint's typevar) for S, producing a weaker but useful
                // constraint.
                if let Some(upper) = bound_data.bounds.upper {
                    try_one_bound(upper, true);
                }
                if let Some(lower) = bound_data.bounds.lower {
                    try_one_bound(lower, false);
                }
            };

        try_weakening(left_constraint, right_constraint);
        try_weakening(right_constraint, left_constraint);
    }

    fn add_mutual_sequents_for_same_typevars<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        let mut try_one_direction =
            |left_constraint: ConstraintId, right_constraint: ConstraintId| {
                let left_constraint_data = storage.constraint_data(left_constraint);
                let left_lower = left_constraint_data.bounds.lower_bound();
                let left_upper = left_constraint_data.bounds.upper_bound();
                let right_constraint_data = storage.constraint_data(right_constraint);
                let right_lower = right_constraint_data.bounds.lower_bound();
                let right_upper = right_constraint_data.bounds.upper_bound();
                let mut new_constraints =
                    |bound_typevar: BoundTypeVarInstance<'db>,
                     mut right_lower: Option<ConstraintBound<'db>>,
                     mut right_upper: Option<ConstraintBound<'db>>| {
                        if let Some(right_lower_bound) = right_lower
                            && let Type::TypeVar(other_bound_typevar) = right_lower_bound.ty()
                            && bound_typevar.is_same_typevar_as(db, other_bound_typevar)
                        {
                            right_lower = None;
                        }
                        if let Some(right_upper_bound) = right_upper
                            && let Type::TypeVar(other_bound_typevar) = right_upper_bound.ty()
                            && bound_typevar.is_same_typevar_as(db, other_bound_typevar)
                        {
                            right_upper = None;
                        }

                        // Same idea as `add_mutual_sequents_for_different_typevars`: if a derived
                        // post-condition for `[bound]` has top-level typevar bounds in the wrong
                        // orientation, split it into equivalent canonical constraints instead of
                        // dropping it.
                        let mut post_constraints: SmallVec<[ConstraintId; 3]> = SmallVec::new();
                        // These are derived logical constraints, not direct inference evidence.
                        // Avoid preserving explicit bounds that are equivalent to missing
                        // lower/upper bounds; direct constraints still retain their explicit
                        // bound presence.
                        let mut constrained_lower =
                            right_lower.filter(|bound| !bound.ty().is_never());
                        let mut constrained_upper =
                            right_upper.filter(|bound| !bound.ty().is_object());

                        if let Some(right_lower_bound) = right_lower
                            && let Type::TypeVar(lower_bound_typevar) = right_lower_bound.ty()
                            && !lower_bound_typevar.can_be_bound_for(db, storage, bound_typevar)
                        {
                            post_constraints.push(ConstraintId::new_with_bounds(
                                db,
                                env,
                                storage,
                                lower_bound_typevar,
                                None,
                                Some(right_lower_bound.with_type(Type::TypeVar(bound_typevar))),
                            ));
                            constrained_lower = None;
                        }

                        if let Some(right_upper_bound) = right_upper
                            && let Type::TypeVar(upper_bound_typevar) = right_upper_bound.ty()
                            && !upper_bound_typevar.can_be_bound_for(db, storage, bound_typevar)
                        {
                            post_constraints.push(ConstraintId::new_with_bounds(
                                db,
                                env,
                                storage,
                                upper_bound_typevar,
                                Some(right_upper_bound.with_type(Type::TypeVar(bound_typevar))),
                                None,
                            ));
                            constrained_upper = None;
                        }

                        if constrained_lower.is_some() || constrained_upper.is_some() {
                            post_constraints.push(ConstraintId::new_with_bounds(
                                db,
                                env,
                                storage,
                                bound_typevar,
                                constrained_lower,
                                constrained_upper,
                            ));
                        }

                        post_constraints
                    };
                let post_constraints = match (left_lower.ty(), left_upper.ty()) {
                    (Type::TypeVar(bound_typevar), Type::TypeVar(other_bound_typevar))
                        if bound_typevar.is_same_typevar_as(db, other_bound_typevar) =>
                    {
                        new_constraints(
                            bound_typevar,
                            Some(ConstraintBound::from_transitive_derivation(
                                right_lower.ty(),
                                left_lower,
                                right_lower,
                            )),
                            Some(ConstraintBound::from_transitive_derivation(
                                right_upper.ty(),
                                left_upper,
                                right_upper,
                            )),
                        )
                    }
                    (Type::TypeVar(bound_typevar), _) => new_constraints(
                        bound_typevar,
                        None,
                        Some(ConstraintBound::from_transitive_derivation(
                            right_upper.ty(),
                            left_lower,
                            right_upper,
                        )),
                    ),
                    (_, Type::TypeVar(bound_typevar)) => new_constraints(
                        bound_typevar,
                        Some(ConstraintBound::from_transitive_derivation(
                            right_lower.ty(),
                            left_upper,
                            right_lower,
                        )),
                        None,
                    ),
                    _ => return,
                };
                for post_constraint in post_constraints {
                    self.add_pair_implication(
                        db,
                        env,
                        storage,
                        left_constraint,
                        right_constraint,
                        post_constraint,
                    );
                }
            };

        try_one_direction(left_constraint, right_constraint);
        try_one_direction(right_constraint, left_constraint);
    }

    fn add_concrete_sequents<'db>(
        &mut self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        left_constraint: ConstraintId,
        right_constraint: ConstraintId,
    ) {
        // These might seem redundant with the intersection check below, since `a → b` means that
        // `a ∧ b = a`. But we are not normalizing constraint bounds, and these clauses help us
        // identify constraints that are identical besides e.g. ordering of union/intersection
        // elements. (For instance, when processing `T ≤ τ₁ & τ₂` and `T ≤ τ₂ & τ₁`, these clauses
        // would add sequents for `(T ≤ τ₁ & τ₂) → (T ≤ τ₂ & τ₁)` and vice versa.)
        if storage.cached_constraint_implies(db, env, left_constraint, right_constraint) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left_constraint.display(db, env, storage),
                right = %right_constraint.display(db, env, storage),
                "left implies right",
            );
            self.add_single_implication(left_constraint, right_constraint);
        }
        if storage.cached_constraint_implies(db, env, right_constraint, left_constraint) {
            tracing::trace!(
                target: "ty_python_semantic::types::constraints::SequentMap",
                left = %left_constraint.display(db, env, storage),
                right = %right_constraint.display(db, env, storage),
                "right implies left",
            );
            self.add_single_implication(right_constraint, left_constraint);
        }

        match left_constraint.intersect(db, env, storage, right_constraint) {
            IntersectionResult::Simplified(intersection_constraint_data) => {
                let intersection_constraint =
                    storage.intern_constraint(db, env, intersection_constraint_data);
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::SequentMap",
                    left = %left_constraint.display(db, env, storage),
                    right = %right_constraint.display(db, env, storage),
                    intersection = %intersection_constraint.display(db, env, storage),
                    "left and right overlap",
                );
                self.add_pair_implication(
                    db,
                    env,
                    storage,
                    left_constraint,
                    right_constraint,
                    intersection_constraint,
                );
                self.add_single_implication(intersection_constraint, left_constraint);
                self.add_single_implication(intersection_constraint, right_constraint);
            }

            // The sequent map only needs to include constraints that might appear in a BDD. If the
            // intersection does not collapse to a single constraint, then there's no new
            // constraint that we need to add to the sequent map.
            IntersectionResult::CannotSimplify => {}

            IntersectionResult::Disjoint => {
                tracing::trace!(
                    target: "ty_python_semantic::types::constraints::SequentMap",
                    left = %left_constraint.display(db, env, storage),
                    right = %right_constraint.display(db, env, storage),
                    "left and right are disjoint",
                );
                self.add_pair_impossibility(left_constraint, right_constraint);
            }
        }
    }

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

impl<'db> Type<'db> {
    /// Returns whether this type can participate in a transitive sequent proof.
    ///
    /// Gradual assignability is not transitive, so constraints with dynamic bounds are ineligible.
    /// Note that we can't use [`is_fully_static`][Type::is_fully_static] here, since that
    /// considers the declared bounds/constraints of typevars. In the context of a sequent map,
    /// typevars are opaque symbolic atoms: considering their bounds or defaults could incorrectly
    /// make their eligibility depend on a specialization that the sequent is meant to constrain.
    pub(super) fn is_static_sequent_eligible(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> bool {
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
        let builder = ConstraintSetBuilder::new();
        let t = create_typevar(db, "T");
        let bool = known_instance(db, KnownClass::Bool);
        let u = create_typevar(db, "U")
            .map_bound_or_constraints(db, |_| Some(TypeVarBoundOrConstraints::UpperBound(bool)));
        let type_of_u = SubclassOfType::from(db, &env, u);
        let bool_class = KnownClass::Bool.to_class_literal(db, &env);
        let mut storage = builder.storage.borrow_mut();
        let left = ConstraintId::new_with_bounds(
            db,
            &env,
            &mut storage,
            t,
            Some(ConstraintBound::Evidence(type_of_u)),
            None,
        );
        let right = ConstraintId::new_with_bounds(
            db,
            &env,
            &mut storage,
            t,
            Some(ConstraintBound::Evidence(bool_class)),
            None,
        );

        for (left, right) in [(left, right), (right, left)] {
            let sequents = SequentMap::for_constraint_pair(db, &env, &mut storage, left, right);

            assert!(
                sequents
                    .sequents
                    .iter()
                    .any(|sequent| matches!(sequent, Sequent::SingleImplication { .. }))
            );
            assert!(!SequentMap::pair_cannot_produce_sequents(
                db,
                &env,
                &mut storage,
                left,
                right
            ));
        }
    }

    #[test]
    fn constraint_implications_are_cached() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let t = create_typevar(db, "T");
        let builder = ConstraintSetBuilder::new();
        let mut storage = builder.storage.borrow_mut();
        let t_int = ConstraintId::new(
            db,
            &env,
            &mut storage,
            t,
            Type::Never,
            KnownClass::Int.to_instance(db, &env),
        );
        let t_bool = ConstraintId::new(
            db,
            &env,
            &mut storage,
            t,
            Type::Never,
            KnownClass::Bool.to_instance(db, &env),
        );

        assert!(storage.cached_constraint_implies(db, &env, t_bool, t_int));
        assert!(storage.cached_constraint_implies(db, &env, t_bool, t_int));
        drop(storage);

        {
            let storage = builder.storage.borrow();
            assert_eq!(
                storage.constraint_implication_cache.get(&(t_bool, t_int)),
                Some(&true)
            );
            assert_eq!(storage.constraint_implication_cache.len(), 1);
        }

        let mut storage = builder.storage.borrow_mut();
        assert!(!storage.cached_constraint_implies(db, &env, t_int, t_bool));
        assert!(!storage.cached_constraint_implies(db, &env, t_int, t_bool));
        drop(storage);

        let storage = builder.storage.borrow();
        assert_eq!(
            storage.constraint_implication_cache.get(&(t_int, t_bool)),
            Some(&false)
        );
        assert_eq!(storage.constraint_implication_cache.len(), 2);
    }
}
