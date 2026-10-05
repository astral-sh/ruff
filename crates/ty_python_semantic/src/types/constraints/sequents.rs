//! The [`SequentMap`] and related functionality

use std::fmt::{Debug, Display};

use itertools::Either;

use crate::types::MaterializationKind;
use crate::types::constraints::variables::{
    ConcreteEquivalenceBound, ConcreteLowerBound, ConcreteUpperBound, Constraint,
    ConstraintProvenance, ProvidesConcreteBound, ProvidesConcreteLowerBound,
    ProvidesConcreteUpperBound, ProvidesTypeVarBound, ProvidesTypeVarEquivalenceBound,
    ProvidesTypeVarRangeBound, TypeVarEquivalenceBound, TypeVarEquivalenceDirectedView,
    TypeVarRangeBound,
};
use crate::types::constraints::{ALWAYS_FALSE, ALWAYS_TRUE, OwnedConstraintSet};
use crate::types::visitor::{
    OrdinaryTypeWalk, TypeWalkFacts, Unrestricted as UnrestrictedWalk, static_eligible_sync,
};
use crate::types::{BoundTypeVarInstance, Type, TypeVarVariance};
use crate::{Db, Program, ProgramEnvironment};

use self::effects::{
    ConjunctionStep, DomainEndpoint, GroupedSequentSource, OrdinarySequentEffects,
    OwnedConjunctionCursor, SequentBuffer, SequentConsequence, SequentEffects, SequentWork,
};

mod effects;
#[cfg(test)]
pub(super) mod profile;
#[cfg(test)]
pub(super) mod runtime;

#[salsa::tracked(attempt = ReturnOnly,
    returns(ref),
    cycle_initial=|_, _, _, _| SequentMap::default(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn for_constraint_inner<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    constraint: Constraint<'db>,
) -> SequentMap<'db> {
    let env = &ProgramEnvironment::from_program(program);
    tracing::trace!(
        target: "ty_python_semantic::types::constraints::SequentMap",
        constraint = %constraint.display(db, env, Some(true)),
        "add sequents for constraint",
    );
    match single_sequents_sync(constraint, &mut OrdinarySequentEffects { db, env }) {
        Ok(map) => map,
        Err(never) => match never {},
    }
}

#[salsa::tracked(attempt = ReturnOnly,
    returns(ref),
    cycle_initial=|_, _, _, _, _| SequentMap::default(),
    heap_size=ruff_memory_usage::heap_size,
)]
fn for_constraint_pair_inner<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    left: Constraint<'db>,
    right: Constraint<'db>,
) -> SequentMap<'db> {
    let env = &ProgramEnvironment::from_program(program);
    tracing::trace!(
        target: "ty_python_semantic::types::constraints::SequentMap",
        left = %left.display(db, env, Some(true)),
        right = %right.display(db, env, Some(true)),
        "add sequents for constraint pair",
    );
    match pair_sequents_sync(left, right, &mut OrdinarySequentEffects { db, env }) {
        Ok(map) => map,
        Err(never) => match never {},
    }
}

#[cfg(test)]
pub(super) fn single_sequent_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<
    impl salsa::plumbing::function::InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (Program<'a>, Constraint<'a>)>
    + for<'a> salsa::plumbing::function::Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
> {
    for_constraint_inner::fn_ingredient_(db, db.zalsa())
}

#[cfg(test)]
pub(super) fn pair_sequent_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<
    impl salsa::plumbing::function::InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<
        Fields<'a> = (Program<'a>, Constraint<'a>, Constraint<'a>),
    > + for<'a> salsa::plumbing::function::Configuration<DbView = dyn Db, Output<'a> = SequentMap<'a>>,
> {
    for_constraint_pair_inner::fn_ingredient_(db, db.zalsa())
}

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
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct SequentMap<'db> {
    /// The sequents that were discovered while creating this sequent map. Some of those sequents
    /// will be "grouped", so that [`PathAssignments`][super::paths::PathAssignments] can add them
    /// to a [`ConstraintSetBuilder`] in a way that respects the builder's typevar ordering.
    pub(super) sequents: Vec<SequentGroup<'db>>,

    /// Pending sequents that have not yet been added to [`sequents`][Self::sequents]. This is only
    /// used during construction, and will be empty in a finalized sequent map.
    pending: Vec<Sequent<Constraint<'db>>>,
}

/// A batch of sequents, along with information about the order they need to be imported into a
/// [`ConstraintSetBuilder`].
///
/// A `SequentMap` is Salsa-cached independently of any particular [`ConstraintSetBuilder`]. Most
/// sequents can be imported into a builder in the order that they are discovered.
///
/// However, if a sequent is derived from a [`TypeVarEquivalenceBound`], we have to be more
/// careful. When comparing a concrete bound with a `TypeVarEquivalenceBound`, we will create
/// sequents that substitute the `left` typevar for the `right`, and vice versa. A
/// `TypeVarEquivalenceBound` is stored internally with its `left` and `right` typevars in an
/// arbitrary order. That means that whether we do the `left → right` substitution or `right →
/// left` substitution first can affect the types that we provide while solving, since
/// [`PathAssignments`][super::paths::PathAssignments] uses "derivation discovery order" as a tiebreaker for derived sequents. The
/// [`Grouped`][Self::Grouped] variant handles this by storing the sequents that we get for each
/// substitution separately. This allows `PathAssignments` to choose which substitution direction
/// to import first, based on its builder's local typevar ordering.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum SequentGroup<'db> {
    Ungrouped(Box<[Sequent<Constraint<'db>>]>),
    Grouped {
        equivalence: TypeVarEquivalenceBound<'db>,
        leftwards: Box<[Sequent<Constraint<'db>>]>,
        rightwards: Box<[Sequent<Constraint<'db>>]>,
    },
}

/// Describes one rule for deriving new implicit constraints from existing constraints in a BDD
/// path. Fuel costs are filled in when cached sequents are imported into a builder.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) enum Sequent<C, FuelCost = ()> {
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
    TripleImpossibility { ante1: C, ante2: C, ante3: C },

    /// Sequent of the form `C → D`
    ///
    /// This indicates that `C` on its own is enough to imply `D`. For any path that assumes `C`
    /// holds, we can add `D` to the path even if it doesn't appear in the BDD.
    SingleImplication {
        ante: C,
        post: C,
        fuel_cost: FuelCost,
    },

    /// Sequent of the form `C₁ ∧ C₂ → D`
    ///
    /// This indicates that if `C₁` and `C₂` are both true, then `D` is guaranteed to be true as
    /// well. For any path that assumes both `C₁` and `C₂` hold, we can add `D` to the path even if
    /// it doesn't appear in the BDD.
    PairImplication {
        ante1: C,
        ante2: C,
        post: C,
        fuel_cost: FuelCost,
    },
}

impl<'db> SequentMap<'db> {
    #[expect(dead_code)] // Keep this around for debugging purposes
    fn display<'a>(
        &'a self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
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

            for sequent in self.all_sequents() {
                match sequent {
                    Sequent::SingleTautology { .. } => {}

                    Sequent::PairImpossibility { ante1, ante2 } => {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} ∧ {} → false",
                            ante1.display(db, env, Some(true)),
                            ante2.display(db, env, Some(true)),
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
                            ante1.display(db, env, Some(true)),
                            ante2.display(db, env, Some(true)),
                            ante3.display(db, env, Some(true)),
                        )?;
                    }

                    Sequent::PairImplication {
                        ante1, ante2, post, ..
                    } => {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} ∧ {} → {}",
                            ante1.display(db, env, Some(true)),
                            ante2.display(db, env, Some(true)),
                            post.display(db, env, Some(true)),
                        )?;
                    }

                    Sequent::SingleImplication { ante, post, .. } => {
                        maybe_write_prefix(f)?;
                        write!(
                            f,
                            "{} → {}",
                            ante.display(db, env, Some(true)),
                            post.display(db, env, Some(true))
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

    fn all_sequents(&self) -> impl Iterator<Item = Sequent<Constraint<'db>>> {
        self.sequents.iter().flat_map(|group| match group {
            SequentGroup::Ungrouped(ungrouped) => Either::Left(ungrouped.iter().copied()),
            SequentGroup::Grouped {
                leftwards,
                rightwards,
                ..
            } => Either::Right(std::iter::chain(
                leftwards.iter().copied(),
                rightwards.iter().copied(),
            )),
        })
    }

    /// Returns a sequent map containing the sequents that we can infer from a single constraint in
    /// isolation. This method is cached so that we only perform this work once per
    /// constraint.
    pub(super) fn for_constraint(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraint: Constraint<'db>,
    ) -> &'db Self {
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
        match pair_cannot_produce_sync(left, right, &mut OrdinarySequentEffects { db, env }) {
            Ok(result) => result,
            Err(never) => match never {},
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplacementIs {
    FromEquivalence,
    FromRange,
}

impl ReplacementIs {
    fn from_concrete_bound<'db>(bound: impl ProvidesConcreteBound<'db>) -> ReplacementIs {
        if bound.is_equivalence() {
            ReplacementIs::FromEquivalence
        } else {
            ReplacementIs::FromRange
        }
    }

    fn from_typevar_bound<'db>(bound: impl ProvidesTypeVarBound<'db>) -> ReplacementIs {
        if bound.is_equivalence() {
            ReplacementIs::FromEquivalence
        } else {
            ReplacementIs::FromRange
        }
    }
}

#[ty_mapping_probe_macros::dual_sequent]
async fn single_sequents_with<'db, E: SequentEffects<'db>>(
    constraint: Constraint<'db>,
    effects: &mut E,
) -> Result<SequentMap<'db>, E::Error> {
    effects.checkpoint(SequentWork::Entry).await?;
    let mut map = SequentMap::default();
    constraint_sequents_with(constraint, &mut map, effects).await?;
    finish_sequents_with(&mut map, effects).await?;
    effects.checkpoint(SequentWork::Complete).await?;
    Ok(map)
}

#[ty_mapping_probe_macros::dual_sequent]
async fn pair_sequents_with<'db, E: SequentEffects<'db>>(
    left: Constraint<'db>,
    right: Constraint<'db>,
    effects: &mut E,
) -> Result<SequentMap<'db>, E::Error> {
    effects.checkpoint(SequentWork::Entry).await?;
    let mut map = SequentMap::default();
    constraint_pair_sequents_with(left, &mut map, right, effects).await?;
    finish_sequents_with(&mut map, effects).await?;
    effects.checkpoint(SequentWork::Complete).await?;
    Ok(map)
}

#[ty_mapping_probe_macros::dual_sequent]
async fn pair_cannot_produce_with<'db, E: SequentEffects<'db>>(
    left: Constraint<'db>,
    right: Constraint<'db>,
    effects: &mut E,
) -> Result<bool, E::Error> {
    effects.checkpoint(SequentWork::Entry).await?;
    // Currently, the only pattern we look for is when two concrete lower-bound constraints
    // have disjoint bounds. Given `l₁ ≤ T ∧ l₂ ≤ T`, the only sequent we could theoretically
    // produce is `(l₁ | l₂) ≤ T`. But we don't store that as a single constraint; we always
    // break that apart into the two smaller constraints that we started with.

    let Constraint::ConcreteLower(left) = left else {
        return Ok(false);
    };
    let Constraint::ConcreteLower(right) = right else {
        return Ok(false);
    };
    if effects.fields().identity(left.typevar) != effects.fields().identity(right.typevar) {
        return Ok(false);
    }

    Ok(effects.trivially_disjoint(left.bound, right.bound).await?)
}

#[ty_mapping_probe_macros::dual_sequent]
async fn constraint_sequents_with<'db, E: SequentEffects<'db>>(
    constraint: Constraint<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    match constraint {
        Constraint::ConcreteLower(this) => lower_sequents_with(this, map, effects).await?,
        Constraint::ConcreteUpper(this) => upper_sequents_with(this, map, effects).await?,
        Constraint::ConcreteEquivalence(this) => {
            equivalence_sequents_with(this, map, effects).await?
        }
        Constraint::TypeVarRange(this) => range_sequents_with(this, map, effects).await?,
        Constraint::TypeVarEquivalence(this) => {
            typevar_equivalence_sequents_with(this, map, effects).await?
        }
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn constraint_pair_sequents_with<'db, E: SequentEffects<'db>>(
    left: Constraint<'db>,
    map: &mut SequentMap<'db>,
    right: Constraint<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    match (left, right) {
        (Constraint::ConcreteLower(this), Constraint::ConcreteLower(other)) => {
            lower_pair_lower_with(this, map, other, false, effects).await?;
        }
        (Constraint::ConcreteLower(this), Constraint::ConcreteUpper(other)) => {
            lower_pair_upper_with(this, map, other, false, effects).await?;
        }
        (Constraint::ConcreteUpper(other), Constraint::ConcreteLower(this)) => {
            lower_pair_upper_with(this, map, other, true, effects).await?;
        }
        (Constraint::ConcreteLower(this), Constraint::ConcreteEquivalence(other)) => {
            lower_pair_equivalence_with(this, map, other, false, effects).await?;
        }
        (Constraint::ConcreteEquivalence(other), Constraint::ConcreteLower(this)) => {
            lower_pair_equivalence_with(this, map, other, true, effects).await?;
        }
        (Constraint::ConcreteLower(this), Constraint::TypeVarRange(other)) => {
            lower_pair_range_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarRange(other), Constraint::ConcreteLower(this)) => {
            lower_pair_range_with(this, map, other, true, effects).await?;
        }
        (Constraint::ConcreteLower(this), Constraint::TypeVarEquivalence(other)) => {
            lower_pair_typevar_equivalence_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarEquivalence(other), Constraint::ConcreteLower(this)) => {
            lower_pair_typevar_equivalence_with(this, map, other, true, effects).await?;
        }

        (Constraint::ConcreteUpper(this), Constraint::ConcreteUpper(other)) => {
            upper_pair_upper_with(this, map, other, false, effects).await?;
        }
        (Constraint::ConcreteUpper(this), Constraint::ConcreteEquivalence(other)) => {
            upper_pair_equivalence_with(this, map, other, false, effects).await?;
        }
        (Constraint::ConcreteEquivalence(other), Constraint::ConcreteUpper(this)) => {
            upper_pair_equivalence_with(this, map, other, true, effects).await?;
        }
        (Constraint::ConcreteUpper(this), Constraint::TypeVarRange(other)) => {
            upper_pair_range_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarRange(other), Constraint::ConcreteUpper(this)) => {
            upper_pair_range_with(this, map, other, true, effects).await?;
        }
        (Constraint::ConcreteUpper(this), Constraint::TypeVarEquivalence(other)) => {
            upper_pair_typevar_equivalence_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarEquivalence(other), Constraint::ConcreteUpper(this)) => {
            upper_pair_typevar_equivalence_with(this, map, other, true, effects).await?;
        }

        (Constraint::ConcreteEquivalence(this), Constraint::ConcreteEquivalence(other)) => {
            equivalence_pair_equivalence_with(this, map, other, false, effects).await?;
        }
        (Constraint::ConcreteEquivalence(this), Constraint::TypeVarRange(other)) => {
            equivalence_pair_range_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarRange(other), Constraint::ConcreteEquivalence(this)) => {
            equivalence_pair_range_with(this, map, other, true, effects).await?;
        }
        (Constraint::ConcreteEquivalence(this), Constraint::TypeVarEquivalence(other)) => {
            equivalence_pair_typevar_equivalence_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarEquivalence(other), Constraint::ConcreteEquivalence(this)) => {
            equivalence_pair_typevar_equivalence_with(this, map, other, true, effects).await?;
        }

        (Constraint::TypeVarRange(this), Constraint::TypeVarRange(other)) => {
            range_pair_range_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarRange(this), Constraint::TypeVarEquivalence(other)) => {
            range_pair_typevar_equivalence_with(this, map, other, false, effects).await?;
        }
        (Constraint::TypeVarEquivalence(other), Constraint::TypeVarRange(this)) => {
            range_pair_typevar_equivalence_with(this, map, other, true, effects).await?;
        }

        (Constraint::TypeVarEquivalence(this), Constraint::TypeVarEquivalence(other)) => {
            typevar_equivalence_pair_typevar_equivalence_with(this, map, other, false, effects)
                .await?;
        }
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn lower_sequents_with<'db, E: SequentEffects<'db>>(
    this: ConcreteLowerBound<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // `⊥ ≤ T` is always true
    if this.bound
        == effects
            .domain_endpoint(
                effects.fields().domain(this.typevar),
                DomainEndpoint::Bottom,
            )
            .await?
    {
        effects
            .emit(map, Sequent::SingleTautology { ante: this.into() })
            .await?;
    }

    // `⊤ ≤ T` implies `T = ⊤`
    if this.bound
        == effects
            .domain_endpoint(effects.fields().domain(this.typevar), DomainEndpoint::Top)
            .await?
    {
        let derived = ConcreteEquivalenceBound::new(this.provenance, this.typevar, this.bound);
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: this.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn upper_sequents_with<'db, E: SequentEffects<'db>>(
    this: ConcreteUpperBound<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // `T ≤ ⊤` is always true
    if this.bound
        == effects
            .domain_endpoint(effects.fields().domain(this.typevar), DomainEndpoint::Top)
            .await?
    {
        effects
            .emit(map, Sequent::SingleTautology { ante: this.into() })
            .await?;
    }

    // `T ≤ ⊥` implies `T = ⊥`
    if this.bound
        == effects
            .domain_endpoint(
                effects.fields().domain(this.typevar),
                DomainEndpoint::Bottom,
            )
            .await?
    {
        let derived = ConcreteEquivalenceBound::new(this.provenance, this.typevar, this.bound);
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: this.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn equivalence_sequents_with<'db, E: SequentEffects<'db>>(
    this: ConcreteEquivalenceBound<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // We cannot infer any sequents from `T = α` on its own.
    let _ = (this, map);
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn range_sequents_with<'db, E: SequentEffects<'db>>(
    this: TypeVarRangeBound<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // `T ≤ T` is always true
    if effects.fields().identity(this.left) == effects.fields().identity(this.right) {
        effects
            .emit(map, Sequent::SingleTautology { ante: this.into() })
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn typevar_equivalence_sequents_with<'db, E: SequentEffects<'db>>(
    this: TypeVarEquivalenceBound<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // `T = T` is always true
    if effects.fields().identity(this.left) == effects.fields().identity(this.right) {
        effects
            .emit(map, Sequent::SingleTautology { ante: this.into() })
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn lower_pair_lower_with<'db, E: SequentEffects<'db>>(
    this: ConcreteLowerBound<'db>,
    map: &mut SequentMap<'db>,
    other: ConcreteLowerBound<'db>,
    reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // We can infer sequents from `α ≤ T` and `β ≤ U` if α _contains_ U and/or β contains T.
    if effects.fields().identity(this.typevar) != effects.fields().identity(other.typevar) {
        add_covariant_lower_tightened_sequent_with(map, this, other, effects).await?;
        add_covariant_lower_tightened_sequent_with(map, other, this, effects).await?;
        return Ok(());
    }

    // These might seem redundant with the union calculation check below, since `a → b` means
    // that `a ∧ b = a`. But we are not normalizing constraint bounds, and these clauses help
    // us identify constraints that are identical besides e.g. ordering of union/intersection
    // elements. (For instance, when processing `τ₁ & τ₂ ≤ T` and `τ₂ & τ₁ ≤ T`, these clauses
    // would add sequents for `(τ₁ & τ₂ ≤ T) → (τ₂ & τ₁ ≤ T)` and vice versa.)

    // (β ≤ α) ⇒ ((α ≤ T) ⇒ (β ≤ T))
    if effects.assignable(other.bound, this.bound).await? {
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: this.into(),
                    post: other.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // (α ≤ β) ⇒ ((β ≤ T) ⇒ (α ≤ T))
    if effects.assignable(this.bound, other.bound).await? {
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: other.into(),
                    post: this.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // `(α ≤ T) ∧ (β ≤ T)` is equivalent to `(α | β) ≤ T`. We do not create lower bounds that
    // are unions, so only add sequents when the union simplifies away.
    let combined = possibly_reversed_union_with(reversed, this.bound, other.bound, effects).await?;
    if !combined.is_union() {
        let provenance = ConstraintProvenance::simplified(
            this.provenance,
            this.bound,
            other.provenance,
            other.bound,
            combined,
        );
        let combined = ConcreteLowerBound::new(provenance, this.typevar, combined);

        // The result is an equivalence, so add implications in both directions.
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: combined.into(),
                    fuel_cost: (),
                },
            )
            .await?;
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: combined.into(),
                    post: this.into(),
                    fuel_cost: (),
                },
            )
            .await?;
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: combined.into(),
                    post: other.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn lower_pair_upper_with<'db, E: SequentEffects<'db>>(
    this: ConcreteLowerBound<'db>,
    map: &mut SequentMap<'db>,
    other: ConcreteUpperBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // We can infer sequents from `α ≤ T` and `U ≤ β` if α _contains_ U and/or β contains T.
    if effects.fields().identity(this.typevar) != effects.fields().identity(other.typevar) {
        add_contravariant_tightened_sequent_with(map, this, other, effects).await?;

        // `(T ≤ pivot) ∧ (pivot ≤ U) → (T ≤ U)` when both constraints use the same
        // statically eligible pivot type.
        if effects.fields().domain(this.typevar) == effects.fields().domain(other.typevar)
            && other.bound
                != effects
                    .domain_endpoint(
                        effects.fields().domain(this.typevar),
                        DomainEndpoint::Bottom,
                    )
                    .await?
            && other.bound
                != effects
                    .domain_endpoint(effects.fields().domain(this.typevar), DomainEndpoint::Top)
                    .await?
            && effects.static_eligible(this.bound).await?
            && effects.static_eligible(other.bound).await?
            && effects.equivalent(other.bound, this.bound).await?
        {
            let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
            let derived = TypeVarRangeBound::new_with_fields(
                effects.fields().interned(),
                provenance,
                other.typevar,
                this.typevar,
            );
            effects
                .emit(
                    map,
                    Sequent::PairImplication {
                        ante1: this.into(),
                        ante2: other.into(),
                        post: derived.into(),
                        fuel_cost: (),
                    },
                )
                .await?;
        }
        return Ok(());
    }

    // `(α ≤ T) ∧ (T ≤ β)` simplifies to `T = α` when `α = β`. For ordinary typevars, only
    // simplify when the materialized bounds are the same `Type`; checking semantic equivalence
    // can recursively expand protocol members. ParamSpec bounds still need the semantic check
    // because callable types with different return types can represent the same parameter list.
    // (We don't need to add the projection implication `(T = α) ⇒ (α ≤ T)`, since anything we
    // can derive from `α ≤ T` we can also derive from `T = α`.)
    let lower = effects
        .materialize(this.bound, MaterializationKind::Bottom)
        .await?;
    let upper = effects
        .materialize(other.bound, MaterializationKind::Top)
        .await?;
    if lower == upper
        || (effects.fields().is_paramspec(this.typevar) && effects.equivalent(lower, upper).await?)
    {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let simplified = ConcreteEquivalenceBound::new(provenance, this.typevar, lower);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: simplified.into(),
                    fuel_cost: (),
                },
            )
            .await?;
        return Ok(());
    }

    // Gradual assignability is not transitive, so only fully static bounds can contribute
    // additional range sequents.
    if effects.static_eligible(this.bound).await? && effects.static_eligible(other.bound).await? {
        add_sequents_for_range_with(map, this, other, effects).await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn lower_pair_equivalence_with<'db, E: SequentEffects<'db>>(
    this: ConcreteLowerBound<'db>,
    map: &mut SequentMap<'db>,
    other: ConcreteEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // We can infer sequents from `α ≤ T` and `U = β` if α _contains_ U and/or β contains T.
    if effects.fields().identity(this.typevar) != effects.fields().identity(other.typevar) {
        add_covariant_lower_tightened_sequent_with(map, this, other, effects).await?;
        add_covariant_lower_tightened_sequent_with(map, other, this, effects).await?;
        add_contravariant_tightened_sequent_with(map, this, other, effects).await?;
        add_invariant_tightened_sequent_with(map, this, other, effects).await?;

        // `(pivot ≤ T) ∧ (U = pivot) → (U ≤ T)`.
        if effects.fields().domain(this.typevar) == effects.fields().domain(other.typevar)
            && effects.static_eligible(this.bound).await?
            && effects.static_eligible(other.bound).await?
            && effects.equivalent(this.bound, other.bound).await?
        {
            let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
            let derived = TypeVarRangeBound::new_with_fields(
                effects.fields().interned(),
                provenance,
                other.typevar,
                this.typevar,
            );
            effects
                .emit(
                    map,
                    Sequent::PairImplication {
                        ante1: this.into(),
                        ante2: other.into(),
                        post: derived.into(),
                        fuel_cost: (),
                    },
                )
                .await?;
        }
        return Ok(());
    }

    // (α ≤ β) ⇒ ((T = β) ⇒ (α ≤ T))
    if effects.assignable(this.bound, other.bound).await? {
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: other.into(),
                    post: this.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // Given constraints `α ≤ T` and `T = β`, `α ≤ β` must also hold. If those bounds contain
    // other typevars, we can infer additional constraints.
    add_sequents_for_range_with(map, this, other, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn lower_pair_range_with<'db, E: SequentEffects<'db>>(
    this: ConcreteLowerBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarRangeBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `α ≤ T` and `T ≤ U`, `α ≤ U` must also hold.
    if effects.fields().identity(this.typevar) == effects.fields().identity(other.left) {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteLowerBound::new(provenance, other.right, this.bound);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // We can infer sequents from `α ≤ T` and `S ≤ U` if α _contains_ U.
    add_covariant_lower_weakened_sequent_with(map, this, other, effects).await?;
    add_contravariant_lower_weakened_sequent_with(map, this, other, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn lower_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(
    this: ConcreteLowerBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `α ≤ T` and `T = U`, `α ≤ U` must also hold.
    let other_typevar = if effects.fields().identity(this.typevar)
        == effects.fields().identity(other.left)
    {
        Some(other.right)
    } else if effects.fields().identity(this.typevar) == effects.fields().identity(other.right) {
        Some(other.left)
    } else {
        None
    };
    if let Some(other_typevar) = other_typevar {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteLowerBound::new(provenance, other_typevar, this.bound);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // We can infer sequents from `α ≤ T` and `S ≤ U` if α _contains_ U.
    derive_group_with(GroupedSequentSource::Lower(this), other, map, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn upper_pair_upper_with<'db, E: SequentEffects<'db>>(
    this: ConcreteUpperBound<'db>,
    map: &mut SequentMap<'db>,
    other: ConcreteUpperBound<'db>,
    reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // We can infer sequents from `T ≤ α` and `U ≤ β` if α _contains_ U and/or β contains T.
    if effects.fields().identity(this.typevar) != effects.fields().identity(other.typevar) {
        add_covariant_upper_tightened_sequent_with(map, this, other, effects).await?;
        add_covariant_upper_tightened_sequent_with(map, other, this, effects).await?;
        return Ok(());
    }

    // These might seem redundant with the intersection calculation check below, since `a → b`
    // means that `a ∧ b = a`. But we are not normalizing constraint bounds, and these clauses
    // help us identify constraints that are identical besides e.g. ordering of
    // union/intersection elements. (For instance, when processing `T ≤ τ₁ | τ₂` and
    // `T ≤ τ₂ | τ₁`, these clauses would add sequents for `(T ≤ τ₁ | τ₂) → (T ≤ τ₂ | τ₁)` and
    // vice versa.)

    // (α ≤ β) ⇒ ((T ≤ α) ⇒ (T ≤ β))
    if effects.assignable(this.bound, other.bound).await? {
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: this.into(),
                    post: other.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // (β ≤ α) ⇒ ((T ≤ β) ⇒ (T ≤ α))
    if effects.assignable(other.bound, this.bound).await? {
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: other.into(),
                    post: this.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // Keep unions as separate, factored upper bounds. Intersecting a union with another bound
    // can distribute the result into a union of intersections. That expanded type no longer
    // looks like an intersection, and repeatedly combining it with other upper bounds can
    // produce a combinatorial number of equivalent constraints.
    if this.bound.is_union() || other.bound.is_union() {
        return Ok(());
    }

    // `(T ≤ α) ∧ (T ≤ β)` is equivalent to `T ≤ (α & β)`. We do not create upper bounds that
    // are intersections, so only add sequents when the intersection simplifies away.
    let combined =
        possibly_reversed_intersection_with(reversed, this.bound, other.bound, effects).await?;
    if !effects.fields().is_nontrivial_intersection(combined) {
        let provenance = ConstraintProvenance::simplified(
            this.provenance,
            this.bound,
            other.provenance,
            other.bound,
            combined,
        );
        let combined = ConcreteUpperBound::new(provenance, this.typevar, combined);

        // The result is an equivalence, so add implications in both directions.
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: combined.into(),
                    fuel_cost: (),
                },
            )
            .await?;
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: combined.into(),
                    post: this.into(),
                    fuel_cost: (),
                },
            )
            .await?;
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: combined.into(),
                    post: other.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn upper_pair_equivalence_with<'db, E: SequentEffects<'db>>(
    this: ConcreteUpperBound<'db>,
    map: &mut SequentMap<'db>,
    other: ConcreteEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // We can infer sequents from `T ≤ α` and `U = β` if α _contains_ U and/or β contains T.
    if effects.fields().identity(this.typevar) != effects.fields().identity(other.typevar) {
        add_covariant_upper_tightened_sequent_with(map, this, other, effects).await?;
        add_covariant_upper_tightened_sequent_with(map, other, this, effects).await?;
        add_contravariant_tightened_sequent_with(map, other, this, effects).await?;
        add_invariant_tightened_sequent_with(map, this, other, effects).await?;

        // `(T ≤ pivot) ∧ (U = pivot) → (T ≤ U)`.
        if effects.fields().domain(this.typevar) == effects.fields().domain(other.typevar)
            && effects.static_eligible(this.bound).await?
            && effects.static_eligible(other.bound).await?
            && effects.equivalent(this.bound, other.bound).await?
        {
            let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
            let derived = TypeVarRangeBound::new_with_fields(
                effects.fields().interned(),
                provenance,
                this.typevar,
                other.typevar,
            );
            effects
                .emit(
                    map,
                    Sequent::PairImplication {
                        ante1: this.into(),
                        ante2: other.into(),
                        post: derived.into(),
                        fuel_cost: (),
                    },
                )
                .await?;
        }
        return Ok(());
    }

    // (β ≤ α) ⇒ ((T = β) ⇒ (T ≤ α))
    if effects.assignable(other.bound, this.bound).await? {
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: other.into(),
                    post: this.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // Given constraints `T ≤ α` and `T = β`, `α ≤ β` must also hold. If those bounds contain
    // other typevars, we can infer additional constraints.
    add_sequents_for_range_with(map, other, this, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn upper_pair_range_with<'db, E: SequentEffects<'db>>(
    this: ConcreteUpperBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarRangeBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `T ≤ α` and `U ≤ T`, `U ≤ α` must also hold.
    if effects.fields().identity(this.typevar) == effects.fields().identity(other.right) {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteUpperBound::new(provenance, other.left, this.bound);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // We can infer sequents from `T ≤ α` and `S ≤ U` if α _contains_ S.
    add_covariant_upper_weakened_sequent_with(map, this, other, effects).await?;
    add_contravariant_upper_weakened_sequent_with(map, this, other, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn upper_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(
    this: ConcreteUpperBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `T ≤ α` and `U = T`, `U ≤ α` must also hold.
    let other_typevar = if effects.fields().identity(this.typevar)
        == effects.fields().identity(other.left)
    {
        Some(other.right)
    } else if effects.fields().identity(this.typevar) == effects.fields().identity(other.right) {
        Some(other.left)
    } else {
        None
    };
    if let Some(other_typevar) = other_typevar {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteUpperBound::new(provenance, other_typevar, this.bound);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // We can infer sequents from `T ≤ α` and `S = U` if α _contains_ S.
    derive_group_with(GroupedSequentSource::Upper(this), other, map, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn equivalence_pair_equivalence_with<'db, E: SequentEffects<'db>>(
    this: ConcreteEquivalenceBound<'db>,
    map: &mut SequentMap<'db>,
    other: ConcreteEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // We can infer sequents from `T = α` and `U = β` if α _contains_ U and/or β contains T.
    if effects.fields().identity(this.typevar) != effects.fields().identity(other.typevar) {
        add_covariant_equivalence_tightened_sequent_with(map, this, other, effects).await?;
        add_covariant_equivalence_tightened_sequent_with(map, other, this, effects).await?;
        add_contravariant_tightened_sequent_with(map, this, other, effects).await?;
        add_contravariant_tightened_sequent_with(map, other, this, effects).await?;
        add_invariant_tightened_sequent_with(map, this, other, effects).await?;
        add_invariant_tightened_sequent_with(map, other, this, effects).await?;
        return Ok(());
    }

    // Given `T = α` and `T = β`, if α and β are equivalent (but not _identical_), we can infer
    // either from the other.
    if this.bound == other.bound {
        return Ok(());
    }
    if effects.equivalent(this.bound, other.bound).await? {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteEquivalenceBound::new(provenance, other.typevar, other.bound);
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: this.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
        let derived = ConcreteEquivalenceBound::new(provenance, this.typevar, this.bound);
        effects
            .emit(
                map,
                Sequent::SingleImplication {
                    ante: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // Given constraints `T = α` and `T = β`, `α = β` must also hold. If those bounds contain
    // other typevars, we can infer additional constraints.
    add_sequents_for_equivalence_with(map, this, other, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn equivalence_pair_range_with<'db, E: SequentEffects<'db>>(
    this: ConcreteEquivalenceBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarRangeBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `T = α` and `T ≤ U`, `α ≤ U` must also hold.
    if effects.fields().identity(this.typevar) == effects.fields().identity(other.left) {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteLowerBound::new(provenance, other.right, this.bound);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // Given constraints `T = α` and `U ≤ T`, `U ≤ α` must also hold.
    if effects.fields().identity(this.typevar) == effects.fields().identity(other.right) {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteUpperBound::new(provenance, other.left, this.bound);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // We can infer sequents from `T = α` and `S ≤ U` if α _contains_ S or U.
    add_covariant_lower_weakened_sequent_with(map, this, other, effects).await?;
    add_covariant_upper_weakened_sequent_with(map, this, other, effects).await?;
    add_contravariant_lower_weakened_sequent_with(map, this, other, effects).await?;
    add_contravariant_upper_weakened_sequent_with(map, this, other, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn equivalence_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(
    this: ConcreteEquivalenceBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `T = α` and `T = U`, `U = α` must also hold.
    let other_typevar = if effects.fields().identity(this.typevar)
        == effects.fields().identity(other.left)
    {
        Some(other.right)
    } else if effects.fields().identity(this.typevar) == effects.fields().identity(other.right) {
        Some(other.left)
    } else {
        None
    };
    if let Some(other_typevar) = other_typevar {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = ConcreteEquivalenceBound::new(provenance, other_typevar, this.bound);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // We can infer sequents from `T = α` and `S = U` if α _contains_ U.
    derive_group_with(GroupedSequentSource::Equivalent(this), other, map, effects).await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn range_pair_range_with<'db, E: SequentEffects<'db>>(
    this: TypeVarRangeBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarRangeBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // `S ≤ T` and `T ≤ S` implies `S = T`
    if effects.fields().identity(this.left) == effects.fields().identity(other.right)
        && (effects.fields().identity(this.right) == effects.fields().identity(other.left))
    {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = TypeVarEquivalenceBound::new_with_fields(
            effects.fields().interned(),
            provenance,
            this.left,
            this.right,
        );
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
        return Ok(());
    }

    // Given constraints `S ≤ T` and `T ≤ U`, `S ≤ U` must also hold.
    let (left, right) =
        if effects.fields().identity(this.right) == effects.fields().identity(other.left) {
            (this.left, other.right)
        } else if effects.fields().identity(this.left) == effects.fields().identity(other.right) {
            (other.left, this.right)
        } else {
            return Ok(());
        };

    let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
    let derived =
        TypeVarRangeBound::new_with_fields(effects.fields().interned(), provenance, left, right);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: this.into(),
                ante2: other.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn range_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(
    this: TypeVarRangeBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `S ≤ T` and `T = U`, `S ≤ U` must also hold.
    let replacement =
        if effects.fields().identity(this.right) == effects.fields().identity(other.left) {
            Some(other.right)
        } else if effects.fields().identity(this.right) == effects.fields().identity(other.right) {
            Some(other.left)
        } else {
            None
        };
    if let Some(replacement) = replacement {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = TypeVarRangeBound::new_with_fields(
            effects.fields().interned(),
            provenance,
            this.left,
            replacement,
        );
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // Given constraints `S ≤ T` and `R = S`, `R ≤ T` must also hold.
    let replacement =
        if effects.fields().identity(this.left) == effects.fields().identity(other.left) {
            Some(other.right)
        } else if effects.fields().identity(this.left) == effects.fields().identity(other.right) {
            Some(other.left)
        } else {
            None
        };
    if let Some(replacement) = replacement {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = TypeVarRangeBound::new_with_fields(
            effects.fields().interned(),
            provenance,
            replacement,
            this.right,
        );
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn typevar_equivalence_pair_typevar_equivalence_with<'db, E: SequentEffects<'db>>(
    this: TypeVarEquivalenceBound<'db>,
    map: &mut SequentMap<'db>,
    other: TypeVarEquivalenceBound<'db>,
    _reversed: bool,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `S = T` and `T = U`, `S = U` must also hold.
    let replacement =
        if effects.fields().identity(this.right) == effects.fields().identity(other.left) {
            Some(other.right)
        } else if effects.fields().identity(this.right) == effects.fields().identity(other.right) {
            Some(other.left)
        } else {
            None
        };
    if let Some(replacement) = replacement {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = TypeVarEquivalenceBound::new_with_fields(
            effects.fields().interned(),
            provenance,
            this.left,
            replacement,
        );
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // Given constraints `S = T` and `R = S`, `R = T` must also hold.
    let replacement =
        if effects.fields().identity(this.left) == effects.fields().identity(other.left) {
            Some(other.right)
        } else if effects.fields().identity(this.left) == effects.fields().identity(other.right) {
            Some(other.left)
        } else {
            None
        };
    if let Some(replacement) = replacement {
        let provenance = ConstraintProvenance::derived(this.provenance, other.provenance);
        let derived = TypeVarEquivalenceBound::new_with_fields(
            effects.fields().interned(),
            provenance,
            replacement,
            this.right,
        );
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: this.into(),
                    ante2: other.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_sequents_for_range_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    lower: impl ProvidesConcreteLowerBound<'db>,
    upper: impl ProvidesConcreteUpperBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
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

    // Skip trivial cases where the assignability check won't produce useful results.
    if lower.bound()
        == effects
            .domain_endpoint(
                effects.fields().domain(lower.typevar()),
                DomainEndpoint::Bottom,
            )
            .await?
        || upper.bound()
            == effects
                .domain_endpoint(
                    effects.fields().domain(upper.typevar()),
                    DomainEndpoint::Top,
                )
                .await?
    {
        return Ok(());
    }

    let when = effects
        .owned_assignable(lower.bound(), upper.bound())
        .await?;
    add_constraint_set_implication_with(map, lower.into(), upper.into(), when.as_ref(), effects)
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_sequents_for_equivalence_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    lower: impl ProvidesConcreteLowerBound<'db>,
    upper: impl ProvidesConcreteUpperBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given constraints `T = α` and `T = β`, `α = β` must also hold. If those bounds contain
    // other typevars, we can infer additional constraints.
    if effects.static_eligible(lower.bound()).await?
        && effects.static_eligible(upper.bound()).await?
    {
        let when = effects
            .owned_equivalent(lower.bound(), upper.bound())
            .await?;
        add_constraint_set_implication_with(
            map,
            lower.into(),
            upper.into(),
            when.as_ref(),
            effects,
        )
        .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_constraint_set_implication_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    lower_constraint: Constraint<'db>,
    upper_constraint: Constraint<'db>,
    when: &OwnedConstraintSet<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    let mut cursor = OwnedConjunctionCursor::new(when);
    // If the relation _never_ holds, these constraints are contradictory.
    if cursor.root() == ALWAYS_FALSE {
        effects
            .emit(
                map,
                Sequent::PairImpossibility {
                    ante1: lower_constraint,
                    ante2: upper_constraint,
                },
            )
            .await?;
        return Ok(());
    }
    // Fast path: If the relation _always_, there are no derived constraints
    // that we can infer. This would be handled correctly by the logic below, but this is a
    // useful early return. Since we only use this check as an early return happy path, we can
    // accept false negatives. That lets us use the simpler and cheaper check against
    // ALWAYS_TRUE, rather than a more expensive is_always_satisfiable call.
    if cursor.root() == ALWAYS_TRUE {
        return Ok(());
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
    loop {
        match effects.conjunction_step(&mut cursor).await? {
            ConjunctionStep::Pending => {}
            ConjunctionStep::Complete => break,
            ConjunctionStep::Consequence(SequentConsequence::Positive(derived)) => {
                effects
                    .emit(
                        map,
                        Sequent::PairImplication {
                            ante1: lower_constraint,
                            ante2: upper_constraint,
                            post: derived,
                            fuel_cost: (),
                        },
                    )
                    .await?;
            }
            ConjunctionStep::Consequence(SequentConsequence::Negative(derived)) => {
                effects
                    .emit(
                        map,
                        Sequent::TripleImpossibility {
                            ante1: lower_constraint,
                            ante2: upper_constraint,
                            ante3: derived,
                        },
                    )
                    .await?;
            }
        }
    }
    Ok(())
}

/// Substitutes `replacement_bound` for `replacement_typevar` unless the two bounds contain a
/// direct self-reference.
///
/// A replacement containing `replacement_typevar`, such as substituting `G[U]` for `U`, can be
/// fed back into the same substitution to produce `G[G[U]]`. A replacement containing
/// `needle_typevar` can produce the same cycle across the two constraints. Repeatedly
/// following either pattern does not reach a fixed point, so we skip both.
#[ty_mapping_probe_macros::dual_sequent]
async fn substitute_if_not_recursive_with<'db, E: SequentEffects<'db>>(
    needle_typevar: BoundTypeVarInstance<'db>,
    needle_bound: Type<'db>,
    replacement_typevar: BoundTypeVarInstance<'db>,
    replacement_bound: Type<'db>,
    replacement_is: ReplacementIs,
    effects: &mut E,
) -> Result<Option<Type<'db>>, E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Substituting a gradual type is not always safe.
    //
    // If we are substituting because a lower or upper bound, that means we are applying
    // transitivity. Gradual assignability is not transitive. Substituting a dynamic
    // replacement into another bound would let an uncertain relationship participate in an
    // arbitrarily long sequent chain.
    //
    // If we are substituting because of an equivalence bound, that means we have two terms
    // that are equal, and we are just substituting one for the other. This does not rely on
    // transitivity, and is therefore sound even for gradual types.
    if replacement_is == ReplacementIs::FromRange
        && !effects.static_eligible(replacement_bound).await?
    {
        return Ok(None);
    }

    // A self-referential bound can consume another bound on the same typevar repeatedly. For
    // example, combining `F[U] ≤ U` with `M ≤ U` would first produce `F[M] ≤ U`, then
    // `F[F[M]] ≤ U`, and so on.
    if effects.fields().identity(needle_typevar) == effects.fields().identity(replacement_typevar) {
        return Ok(None);
    }

    if let Type::TypeVar(replacement) = replacement_bound
        && ((effects.fields().identity(replacement) == effects.fields().identity(needle_typevar))
            || (effects.fields().identity(replacement)
                == effects.fields().identity(replacement_typevar)))
    {
        return Ok(None);
    }

    if effects
        .variance(replacement_bound, effects.fields().identity(needle_typevar))
        .await?
        != TypeVarVariance::Bivariant
    {
        return Ok(None);
    }
    if effects
        .variance(
            replacement_bound,
            effects.fields().identity(replacement_typevar),
        )
        .await?
        != TypeVarVariance::Bivariant
    {
        return Ok(None);
    }

    Ok(Some(
        effects
            .substitute(needle_bound, replacement_typevar, replacement_bound)
            .await?,
    ))
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_covariant_lower_tightened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteLowerBound<'db>,
    right: impl ProvidesConcreteLowerBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `α ≤ T` and `β ≤ U`, if α contains U covariantly, we can substitute β for U:
    //
    //   (Co[U] ≤ T) ∧ (β ≤ U) ⇒ (Co[β] ≤ T)
    if effects
        .variance(left.bound(), effects.fields().identity(right.typevar()))
        .await?
        != TypeVarVariance::Covariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.typevar(),
        right.bound(),
        ReplacementIs::from_concrete_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.into_lower_bound().map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_covariant_upper_tightened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteUpperBound<'db>,
    right: impl ProvidesConcreteUpperBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T ≤ α` and `U ≤ β`, if α contains U covariantly, we can substitute β for U:
    //
    //   (T ≤ Co[U]) ∧ (U ≤ β) ⇒ (T ≤ Co[β])
    if effects
        .variance(left.bound(), effects.fields().identity(right.typevar()))
        .await?
        != TypeVarVariance::Covariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.typevar(),
        right.bound(),
        ReplacementIs::from_concrete_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.into_upper_bound().map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_covariant_equivalence_tightened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: ConcreteEquivalenceBound<'db>,
    right: ConcreteEquivalenceBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T = α` and `U = β`, if α contains U covariantly, we can substitute β for U:
    //
    //   (T = Co[U]) ∧ (U = β) ⇒ (T = Co[β])
    if effects
        .variance(left.bound(), effects.fields().identity(right.typevar()))
        .await?
        != TypeVarVariance::Covariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.typevar(),
        right.bound(),
        ReplacementIs::from_concrete_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_contravariant_tightened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    lower: impl ProvidesConcreteLowerBound<'db>,
    upper: impl ProvidesConcreteUpperBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    let provenance = ConstraintProvenance::derived(lower.provenance(), upper.provenance());

    // Given `α ≤ T` and `U ≤ β`, if α contains U contravariantly, substitute β for U:
    //
    //   (Contra[U] ≤ T) ∧ (U ≤ β) ⇒ (Contra[β] ≤ T)
    if effects
        .variance(lower.bound(), effects.fields().identity(upper.typevar()))
        .await?
        == TypeVarVariance::Contravariant
        && let Some(replacement) = substitute_if_not_recursive_with(
            lower.typevar(),
            lower.bound(),
            upper.typevar(),
            upper.bound(),
            ReplacementIs::from_concrete_bound(upper),
            effects,
        )
        .await?
    {
        let derived = lower.into_lower_bound().map(provenance, replacement);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: lower.into(),
                    ante2: upper.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }

    // If β contains T contravariantly, substitute α for T:
    //
    //   (α ≤ T) ∧ (U ≤ Contra[T]) ⇒ (U ≤ Contra[α])
    if effects
        .variance(upper.bound(), effects.fields().identity(lower.typevar()))
        .await?
        == TypeVarVariance::Contravariant
        && let Some(replacement) = substitute_if_not_recursive_with(
            upper.typevar(),
            upper.bound(),
            lower.typevar(),
            lower.bound(),
            ReplacementIs::from_concrete_bound(lower),
            effects,
        )
        .await?
    {
        let derived = upper.into_upper_bound().map(provenance, replacement);
        effects
            .emit(
                map,
                Sequent::PairImplication {
                    ante1: lower.into(),
                    ante2: upper.into(),
                    post: derived.into(),
                    fuel_cost: (),
                },
            )
            .await?;
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_invariant_tightened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteBound<'db>,
    right: ConcreteEquivalenceBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T ~ α` and `U = β`, if α contains U invariantly, we can substitute β for U. For
    // instance,
    //
    //   (T ~ In[U]) ∧ (U = β) ⇒ (T ~ In[β])
    if effects
        .variance(left.bound(), effects.fields().identity(right.typevar()))
        .await?
        != TypeVarVariance::Invariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.typevar(),
        right.bound(),
        ReplacementIs::from_concrete_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_covariant_lower_weakened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteLowerBound<'db>,
    right: impl ProvidesTypeVarRangeBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `α ≤ T` and `S ≤ U`, if α contains U covariantly, we can substitute S for U. For
    // instance,
    //
    //   (Co[U] ≤ T) ∧ (S ≤ U) ⇒ (Co[S] ≤ T)
    if effects
        .variance(left.bound(), effects.fields().identity(right.right()))
        .await?
        != TypeVarVariance::Covariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.right(),
        Type::TypeVar(right.left()),
        ReplacementIs::from_typevar_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.into_lower_bound().map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_covariant_upper_weakened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteUpperBound<'db>,
    right: impl ProvidesTypeVarRangeBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T ≤ α` and `U ≤ S`, if α contains U covariantly, we can substitute S for U. For
    // instance,
    //
    //   (T ≤ Co[U]) ∧ (U ≤ S) ⇒ (T ≤ Co[S])
    if effects
        .variance(left.bound(), effects.fields().identity(right.left()))
        .await?
        != TypeVarVariance::Covariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.left(),
        Type::TypeVar(right.right()),
        ReplacementIs::from_typevar_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.into_upper_bound().map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_covariant_equivalence_weakened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: ConcreteEquivalenceBound<'db>,
    right: impl ProvidesTypeVarEquivalenceBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T = α` and `S = U`, if α contains S covariantly, we can substitute U for S. For
    // instance,
    //
    //   (T = Co[S]) ∧ (S = U) ⇒ (T = Co[U])
    if effects
        .variance(left.bound(), effects.fields().identity(right.left()))
        .await?
        != TypeVarVariance::Covariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.left(),
        Type::TypeVar(right.right()),
        ReplacementIs::from_typevar_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_contravariant_lower_weakened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteLowerBound<'db>,
    right: impl ProvidesTypeVarRangeBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `α ≤ T` and `U ≤ S`, if α contains U contravariantly, we can substitute S for U
    // and flip the constraint. For instance,
    //
    //   (Contra[U] ≤ T) ∧ (U ≤ S) ⇒ (Contra[S] ≤ T)
    if effects
        .variance(left.bound(), effects.fields().identity(right.left()))
        .await?
        != TypeVarVariance::Contravariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.left(),
        Type::TypeVar(right.right()),
        ReplacementIs::from_typevar_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.into_lower_bound().map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_contravariant_upper_weakened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteUpperBound<'db>,
    right: impl ProvidesTypeVarRangeBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T ≤ α` and `S ≤ U`, if α contains U contravariantly, we can substitute S for U
    // and flip the constraint. For instance,
    //
    //   (T ≤ Contra[U]) ∧ (S ≤ U) ⇒ (T ≤ Contra[S])
    if effects
        .variance(left.bound(), effects.fields().identity(right.right()))
        .await?
        != TypeVarVariance::Contravariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.right(),
        Type::TypeVar(right.left()),
        ReplacementIs::from_typevar_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.into_upper_bound().map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_contravariant_equivalence_weakened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: ConcreteEquivalenceBound<'db>,
    right: impl ProvidesTypeVarEquivalenceBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T = α` and `S = U`, if α contains U contravariantly, we can substitute S for U
    // and flip the constraint. For instance,
    //
    //   (T = Contra[U]) ∧ (S = U) ⇒ (T = Contra[S])
    if effects
        .variance(left.bound(), effects.fields().identity(right.left()))
        .await?
        != TypeVarVariance::Contravariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.left(),
        Type::TypeVar(right.right()),
        ReplacementIs::from_typevar_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn add_invariant_weakened_sequent_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    left: impl ProvidesConcreteBound<'db>,
    right: impl ProvidesTypeVarEquivalenceBound<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    // Given `T ~ α` and `S = U`, if α contains U invariantly, we can substitute S for U. For
    // instance,
    //
    //   (T ~ In[U]) ∧ (S = U) ⇒ (T ~ In[S])
    if effects
        .variance(left.bound(), effects.fields().identity(right.left()))
        .await?
        != TypeVarVariance::Invariant
    {
        return Ok(());
    }
    let Some(replacement) = substitute_if_not_recursive_with(
        left.typevar(),
        left.bound(),
        right.left(),
        Type::TypeVar(right.right()),
        ReplacementIs::from_typevar_bound(right),
        effects,
    )
    .await?
    else {
        return Ok(());
    };
    let provenance = ConstraintProvenance::derived(left.provenance(), right.provenance());
    let derived = left.map(provenance, replacement);
    effects
        .emit(
            map,
            Sequent::PairImplication {
                ante1: left.into(),
                ante2: right.into(),
                post: derived.into(),
                fuel_cost: (),
            },
        )
        .await?;
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn possibly_reversed_intersection_with<'db, E: SequentEffects<'db>>(
    reversed: bool,
    left: Type<'db>,
    right: Type<'db>,
    effects: &mut E,
) -> Result<Type<'db>, E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    let result = if reversed {
        effects.intersection(right, left).await?
    } else {
        effects.intersection(left, right).await?
    };
    Ok(result)
}

#[ty_mapping_probe_macros::dual_sequent]
async fn possibly_reversed_union_with<'db, E: SequentEffects<'db>>(
    reversed: bool,
    left: Type<'db>,
    right: Type<'db>,
    effects: &mut E,
) -> Result<Type<'db>, E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    let result = if reversed {
        effects.union(right, left).await?
    } else {
        effects.union(left, right).await?
    };
    Ok(result)
}

#[ty_mapping_probe_macros::dual_sequent]
async fn derive_group_direction_with<'db, E: SequentEffects<'db>>(
    source: GroupedSequentSource<'db>,
    direction: TypeVarEquivalenceDirectedView<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Direction).await?;
    match source {
        GroupedSequentSource::Lower(this) => {
            let reversed = direction.reverse();
            add_covariant_lower_weakened_sequent_with(map, this, direction, effects).await?;
            add_contravariant_lower_weakened_sequent_with(map, this, reversed, effects).await?;
            add_invariant_weakened_sequent_with(map, this, reversed, effects).await?;
        }
        GroupedSequentSource::Upper(this) => {
            let reversed = direction.reverse();
            add_covariant_upper_weakened_sequent_with(map, this, reversed, effects).await?;
            add_contravariant_upper_weakened_sequent_with(map, this, direction, effects).await?;
            add_invariant_weakened_sequent_with(map, this, reversed, effects).await?;
        }
        GroupedSequentSource::Equivalent(this) => {
            let reversed = direction.reverse();
            add_covariant_equivalence_weakened_sequent_with(map, this, reversed, effects).await?;
            add_contravariant_equivalence_weakened_sequent_with(map, this, reversed, effects)
                .await?;
            add_invariant_weakened_sequent_with(map, this, reversed, effects).await?;
        }
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn derive_group_with<'db, E: SequentEffects<'db>>(
    source: GroupedSequentSource<'db>,
    equivalence: TypeVarEquivalenceBound<'db>,
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    flush_pending_with(map, effects).await?;
    derive_group_direction_with(source, equivalence.forwards(), map, effects).await?;
    let leftwards = extract_pending_with(map, effects).await?;
    derive_group_direction_with(source, equivalence.backwards(), map, effects).await?;
    let rightwards = extract_pending_with(map, effects).await?;
    match (leftwards.is_empty(), rightwards.is_empty()) {
        (true, true) => {}
        (true, false) => {
            effects.reserve_group(map).await?;
            map.sequents.push(SequentGroup::Ungrouped(rightwards));
        }
        (false, true) => {
            effects.reserve_group(map).await?;
            map.sequents.push(SequentGroup::Ungrouped(leftwards));
        }
        (false, false) => {
            effects.reserve_group(map).await?;
            map.sequents.push(SequentGroup::Grouped {
                equivalence,
                leftwards,
                rightwards,
            });
        }
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn extract_pending_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<Box<[Sequent<Constraint<'db>>]>, E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    effects.prepare_extract(map).await?;
    Ok(map.pending.drain(..).collect())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn flush_pending_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    if !map.pending.is_empty() {
        let pending = extract_pending_with(map, effects).await?;
        effects.reserve_group(map).await?;
        map.sequents.push(SequentGroup::Ungrouped(pending));
    }
    Ok(())
}

#[ty_mapping_probe_macros::dual_sequent]
async fn finish_sequents_with<'db, E: SequentEffects<'db>>(
    map: &mut SequentMap<'db>,
    effects: &mut E,
) -> Result<(), E::Error> {
    effects.checkpoint(SequentWork::Rule).await?;
    flush_pending_with(map, effects).await?;
    effects.prepare_shrink(map, SequentBuffer::Groups).await?;
    map.sequents.shrink_to_fit();
    effects.prepare_shrink(map, SequentBuffer::Pending).await?;
    map.pending.shrink_to_fit();
    Ok(())
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
        let result = static_eligible_sync(
            self,
            TypeWalkFacts,
            &mut OrdinaryTypeWalk {
                db,
                env,
                control: &mut UnrestrictedWalk,
                query: (),
            },
        );
        match result {
            Ok(eligible) => eligible,
            Err(never) => match never {},
        }
    }
}

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
pub(super) fn sequent_fuel_cost_from_depths(
    post_constructor_depth: u16,
    post_typevar_depth: u16,
    antecedent_constructor_depth: u16,
) -> u16 {
    post_typevar_depth
        .max(post_constructor_depth.saturating_sub(antecedent_constructor_depth))
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::db::tests::{TestDb, TestDbBuilder, setup_db};
    use crate::place::global_symbol;
    use crate::types::constraints::{ConstraintSetStorage, max_constructor_and_typevar_depth};
    use crate::types::tuple::TupleType;
    use crate::types::typevar::TypeVarBoundOrConstraints;
    use crate::types::typevar::{
        TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarInstance,
    };
    use crate::types::{BoundTypeVarInstance, KnownClass, SubclassOfType, TypeVarVariance};
    use crate::types::{ClassLiteral, GenericAlias, GenericContext, TypeFormType};
    use ruff_db::files::system_path_to_file;
    use ruff_python_ast::PythonVersion;
    use ruff_python_ast::name::Name;
    use ty_python_core::ProgramFile;

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
            let sequents = SequentMap::for_constraint_pair(db, &env, left, right);

            assert!(
                sequents
                    .all_sequents()
                    .any(|sequent| matches!(sequent, Sequent::SingleImplication { .. }))
            );
            assert!(!SequentMap::pair_cannot_produce_sequents(
                db, &env, left, right
            ));
        }
    }

    fn record_walk_before_migration<'db>(
        db: &'db TestDb,
        env: &ProgramEnvironment<'db>,
        label: &str,
        subject: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
        expected: (&[BoundTypeVarInstance<'db>], bool, bool),
    ) {
        let eligible = bound.is_static_sequent_eligible(db, env);
        assert_eq!(eligible, expected.2, "{label}: eligibility");
        let depth = max_constructor_and_typevar_depth(db, env, bound);
        let provenance = ConstraintProvenance::Evidence;
        for (kind, constraint) in [
            (
                "lower",
                Constraint::ConcreteLower(ConcreteLowerBound::new(provenance, subject, bound)),
            ),
            (
                "upper",
                Constraint::ConcreteUpper(ConcreteUpperBound::new(provenance, subject, bound)),
            ),
            (
                "equivalence",
                Constraint::ConcreteEquivalence(ConcreteEquivalenceBound::new(
                    provenance, subject, bound,
                )),
            ),
        ] {
            let mut storage = ConstraintSetStorage::default();
            let support = storage.intern_constraint_typevars(db, env, constraint);
            let mentioned = support
                .iter()
                .map(|id| storage.typevar_data(id))
                .collect::<Vec<_>>();
            assert_eq!(
                mentioned.as_slice(),
                expected.0,
                "{label}: subject-first support"
            );
            assert_eq!(
                support.is_complete(),
                expected.1,
                "{label}: support completeness"
            );
            let names = mentioned
                .iter()
                .map(|ty| ty.name(db).to_string())
                .collect::<Vec<_>>();
            eprintln!(
                "WALK_BASELINE policy case={label:?} kind={kind} support={names:?} complete={} eligible={eligible} depth={depth:?}",
                support.is_complete()
            );
        }
    }

    #[test]
    fn walk_before_migration_occurrence_order_and_typeforms() {
        let db = setup_db();
        let env = db.program_environment();
        let t = create_typevar(&db, "Subject");
        let u = create_typevar(&db, "First");
        let v = create_typevar(&db, "Second");
        let shared = Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(u)));
        let root = Type::tuple(TupleType::heterogeneous(
            &db,
            &env,
            [shared, Type::TypeVar(v), shared, Type::TypeVar(t)],
        ));
        record_walk_before_migration(
            &db,
            &env,
            "ordered repeated occurrences",
            t,
            root,
            (&[t, u, v], true, true),
        );
        let gradual_first = Type::tuple(TupleType::heterogeneous(&db, &env, [Type::any(), shared]));
        record_walk_before_migration(
            &db,
            &env,
            "dynamic before later occurrence",
            t,
            gradual_first,
            (&[t, u], true, false),
        );
        for (label, inner, eligible) in [
            ("TypeForm literal", Type::int_literal(1), true),
            ("TypeForm Any", Type::any(), false),
        ] {
            let bound = Type::TypeForm(TypeFormType::new(&db, inner));
            record_walk_before_migration(&db, &env, label, t, bound, (&[t], true, eligible));
            assert_eq!(max_constructor_and_typevar_depth(&db, &env, bound), (1, 0));
        }
        record_walk_before_migration(
            &db,
            &env,
            "TypeForm typevar",
            t,
            shared,
            (&[t, u], true, true),
        );
        assert_eq!(max_constructor_and_typevar_depth(&db, &env, shared), (1, 1));
    }

    #[test]
    fn walk_before_migration_alias_declarations_and_tuple_inner() -> anyhow::Result<()> {
        let db = setup_db();
        let env = db.program_environment();
        let t = create_typevar(&db, "Subject");
        let metadata = create_typevar(&db, "Metadata");
        let argument = create_typevar(&db, "Argument");
        let tuple_only = create_typevar(&db, "TupleOnly");
        let original = create_typevar(&db, "Declaration");
        let declaration = TypeVarInstance::new(
            &db,
            original.typevar(&db).identity(&db),
            Some(TypeVarBoundOrConstraints::UpperBound(Type::any()).into()),
            Some(TypeVarVariance::Invariant),
            Some(TypeVarDefaultEvaluation::Eager(Type::TypeVar(metadata))),
        );
        let declaration = BoundTypeVarInstance::new(
            &db,
            declaration,
            original.binding_context(&db),
            original.paramspec_attr(&db),
            original.freshness(&db),
        );
        let context = GenericContext::from_typevar_instances(&db, &env, [declaration]);
        let Type::ClassLiteral(ClassLiteral::Static(list)) =
            KnownClass::List.to_class_literal(&db, &env)
        else {
            anyhow::bail!("list did not produce a static class literal");
        };
        let alias = Type::GenericAlias(GenericAlias::new(
            &db,
            list,
            context.specialize(&db, vec![Type::TypeVar(argument)]),
        ));
        // An occurrence stays opaque; GenericAlias enters the declaration through a different visitor method.
        record_walk_before_migration(
            &db,
            &env,
            "opaque occurrence with gradual metadata",
            t,
            Type::TypeVar(declaration),
            (&[t, declaration], true, true),
        );
        record_walk_before_migration(
            &db,
            &env,
            "alias eager declaration metadata",
            t,
            alias,
            (&[t, argument], true, false),
        );

        let lazy_declaration = TypeVarInstance::new(
            &db,
            original.typevar(&db).identity(&db),
            Some(TypeVarBoundOrConstraintsEvaluation::LazyUpperBound),
            Some(TypeVarVariance::Invariant),
            Some(TypeVarDefaultEvaluation::Lazy),
        );
        let lazy_declaration = BoundTypeVarInstance::new(
            &db,
            lazy_declaration,
            original.binding_context(&db),
            original.paramspec_attr(&db),
            original.freshness(&db),
        );
        let nested_lazy_occurrence =
            Type::TypeForm(TypeFormType::new(&db, Type::TypeVar(lazy_declaration)));
        record_walk_before_migration(
            &db,
            &env,
            "nested occurrence with lazy metadata",
            t,
            nested_lazy_occurrence,
            (&[t, lazy_declaration], true, true),
        );

        let clean_context = GenericContext::from_typevar_instances(&db, &env, [original]);
        let Type::ClassLiteral(ClassLiteral::Static(tuple_class)) =
            KnownClass::Tuple.to_class_literal(&db, &env)
        else {
            anyhow::bail!("tuple did not produce a static class literal");
        };
        let tuple = TupleType::heterogeneous(&db, &env, [Type::TypeVar(tuple_only), Type::any()]);
        let tuple_alias = Type::GenericAlias(GenericAlias::new(
            &db,
            tuple_class,
            clean_context.specialize_tuple(&db, Type::object(), tuple),
        ));
        // The broad element type is object; detailed tuple elements are stored separately.
        record_walk_before_migration(
            &db,
            &env,
            "alias tuple_inner",
            t,
            tuple_alias,
            (&[t], true, false),
        );
        Ok(())
    }

    #[test]
    fn walk_before_migration_depth_saturates() {
        let db = setup_db();
        let env = db.program_environment();
        let variable = create_typevar(&db, "Deep");
        let mut bound = Type::TypeVar(variable);
        for _ in 0..=u16::MAX {
            bound = Type::TypeForm(TypeFormType::new(&db, bound));
        }
        // The ordinary depth cursor is iterative; do not feed this fixture to a recursive support visitor or renderer.
        let depth = max_constructor_and_typevar_depth(&db, &env, bound);
        assert_eq!(depth, (u16::MAX, u16::MAX));
        eprintln!("WALK_BASELINE depth saturated={depth:?}");
    }

    #[test]
    fn walk_before_migration_skipped_lazy_support() -> anyhow::Result<()> {
        let db = TestDbBuilder::new()
            .with_python_version(PythonVersion::PY313)
            .with_file(
                "/src/walk_before.py",
                r#"
from typing import Any, NewType, Protocol
from typing_extensions import TypedDict

class P[T](Protocol):
    value: Any
protocol: P[int]
class Data(TypedDict):
    value: Any
typed_dict: Data
type Alias = Any
alias: Alias
User = NewType("User", int)
newtype: User
class Scope[T: Any = Any]: ...
scope: Scope[int]
Recursive = tuple[int, "Recursive | None"]
recursive: Recursive
"#,
            )
            .build()?;
        let env = db.program_environment();
        let t = create_typevar(&db, "Subject");
        let file = system_path_to_file(&db, "/src/walk_before.py")?;
        let module = ProgramFile::new(&db, file, env.program(&db));
        for (name, expected_complete) in [
            ("protocol", false),
            ("typed_dict", false),
            ("alias", false),
            ("newtype", false),
            ("scope", true),
            ("recursive", false),
        ] {
            let bound = global_symbol(&db, module, name).place.expect_type();
            match name {
                "protocol" => assert!(matches!(bound, Type::ProtocolInstance(_))),
                "typed_dict" => assert!(matches!(bound, Type::TypedDict(_))),
                "alias" => assert!(matches!(bound, Type::TypeAlias(_))),
                "newtype" => assert!(matches!(bound, Type::NewTypeInstance(_))),
                "scope" => assert!(matches!(bound, Type::NominalInstance(_))),
                "recursive" => assert!(matches!(bound, Type::Recursive(_))),
                _ => {}
            }
            // Recursive is a real closed inferred type; its skipped body makes support incomplete.
            record_walk_before_migration(
                &db,
                &env,
                name,
                t,
                bound,
                (&[t], expected_complete, true),
            );
        }
        let eager = Type::protocol_with_readonly_members(&db, &env, [("value", Type::any())]);
        record_walk_before_migration(
            &db,
            &env,
            "synthesized protocol",
            t,
            eager,
            (&[t], true, false),
        );
        Ok(())
    }
}
