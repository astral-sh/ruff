//! The different conditions that can be checked by an interior node in a constraint set BDD
#![expect(dead_code)]

use std::fmt::Display;
use std::marker::PhantomData;

use itertools::Either;
use salsa::plumbing::AsId;

use crate::types::constraints::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintSetBuilder, ConstraintSetStorage, InterimConstraint, Node,
    NodeId, SourceOrderId, max_constructor_and_typevar_depth, wobble_index,
};
use crate::types::typevar::{BoundTypeVarInstance, TypeVarDomain, TypeVarSet};
use crate::types::{ApplyTypeMappingVisitor, Type, TypeContext, TypeMapping};
use crate::{Db, ProgramEnvironment};

/// The _provenance_ of a BDD constraint.
///
/// Most bounds come from specific relationships found at the call site — for instance, the
/// relationship between the argument type and parameter annotation when invoking a generic
/// function. These bounds express actual user intent, and are called _evidence_ bounds.
///
/// Other bounds are background limitations on which specializations are valid — for instance, a
/// typevar's declared `bound_or_constraints`. These are called _validity_ bounds. Importantly, we
/// don't want to choose a validity bound as a solution unless we have no other choice. There is
/// often an evidence bound that is a better choice.
///
/// A bound derived only from validity remains validity. Any derivation that also depends on
/// evidence is itself evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum ConstraintProvenance {
    Validity,
    Evidence,
}

impl ConstraintProvenance {
    /// Returns the provenance of a constraint derived from two existing constraints.
    ///
    /// Derived constraints must retain any call-site evidence that contributed to them. Otherwise,
    /// a derivation could downgrade evidence to a background validity restriction, causing the
    /// solver to ignore a specialization justified by the call site.
    pub(super) const fn derived(left: Self, right: Self) -> Self {
        match (left, right) {
            (Self::Validity, Self::Validity) => Self::Validity,
            _ => Self::Evidence,
        }
    }

    /// Returns the provenance of a bound produced by simplifying two existing bounds.
    ///
    /// Simplifying bounds can make one input redundant, and a redundant input must not affect
    /// provenance. In particular, allowing redundant evidence to promote a surviving validity
    /// bound to evidence could make the solver choose a specialization that the call site does not
    /// actually support. When neither input alone
    /// determines the combined bound, its provenance must reflect both inputs.
    pub(super) fn simplified<'db>(
        left_provenance: Self,
        left_bound: Type<'db>,
        right_provenance: Self,
        right_bound: Type<'db>,
        combined: Type<'db>,
    ) -> Self {
        match (combined == left_bound, combined == right_bound) {
            (true, false) => left_provenance,
            (false, true) => right_provenance,
            _ => ConstraintProvenance::derived(left_provenance, right_provenance),
        }
    }
}

pub(super) struct UnsatisfiableBound;

/// One condition that can be checked by an interior node in a constraint set BDD
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(crate) enum Constraint<'db> {
    ConcreteLower(ConcreteLowerBound<'db>),
    ConcreteUpper(ConcreteUpperBound<'db>),
    ConcreteEquivalence(ConcreteEquivalenceBound<'db>),
    ParamSpecLower(ParamSpecLowerBound<'db>),
    ParamSpecUpper(ParamSpecUpperBound<'db>),
    ParamSpecEquivalence(ParamSpecEquivalenceBound<'db>),
    TypeVarRange(TypeVarRangeBound<'db>),
    TypeVarEquivalence(TypeVarEquivalenceBound<'db>),
}

impl<'db> Constraint<'db> {
    pub(super) fn new_node(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let constraint_id = storage.intern_constraint(db, env, InterimConstraint::New(self));
        Node::new_constraint(storage, constraint_id)
    }

    pub(super) fn new_nodes(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        storage: &mut ConstraintSetStorage<'db>,
        constraints: impl IntoIterator<Item = Result<Self, UnsatisfiableBound>>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let (mut node, mut source_order) = (ALWAYS_TRUE, None);
        for constraint in constraints {
            let Ok(constraint) = constraint else {
                return (ALWAYS_FALSE, None);
            };
            let (constraint_node, constraint_source_order) = constraint.new_node(db, env, storage);
            node = node.and(storage, constraint_node);
            source_order = storage.ordered_source_order(source_order, constraint_source_order);
        }
        (node, source_order)
    }

    /// Returns the constraints that model the requirement that `bound` must be assignable to
    /// `typevar`. Union lower bounds are broken apart into separate constraints. Returns no
    /// constraints when the relationship always holds (e.g. when comparing a typevar with itself).
    pub(super) fn new_lower_bound(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> impl Iterator<Item = Result<Self, UnsatisfiableBound>> {
        let choose_lower_bound = move |bound: Type<'db>| {
            let bound = Self::normalize_bound(db, typevar, bound);
            match bound {
                // Two identical typevars must always solve to the same type, so it is not useful to
                // have a lower bound that is the typevar being constrained.
                Type::TypeVar(lower) if typevar.is_same_typevar_as(db, lower) => None,

                // The same applies for a lower bound that's an intersection containing the typevar
                // being constrained.
                Type::Intersection(intersection)
                    if intersection.positive(db).iter().any(|element| {
                        element.as_typevar().is_some_and(|element_bound_typevar| {
                            typevar.is_same_typevar_as(db, element_bound_typevar)
                        })
                    }) =>
                {
                    None
                }

                // And if we find the _negation_ of the typevar being constrained, the overall result
                // is unsatisfiable.
                Type::Intersection(intersection)
                    if intersection.negative(db).iter().any(|element| {
                        element.as_typevar().is_some_and(|element_bound_typevar| {
                            typevar.is_same_typevar_as(db, element_bound_typevar)
                        })
                    }) =>
                {
                    Some(Err(UnsatisfiableBound))
                }

                // Otherwise we construct the correct lower-bound constraint.

                // Comparing two typevars
                Type::TypeVar(lower) if typevar.domain(db) == lower.domain(db) => {
                    let constraint = TypeVarRangeBound::new(db, provenance, lower, typevar).into();
                    Some(Ok(constraint))
                }
                Type::TypeVar(_) => Some(Err(UnsatisfiableBound)),

                // Comparing a paramspec with a callable type
                Type::Callable(_) if typevar.domain(db) == TypeVarDomain::ParameterSignature => {
                    let constraint =
                        ParamSpecLowerBound::new(db, provenance, typevar, bound).into();
                    Some(Ok(constraint))
                }

                // Cannot compare a paramspec with a non-callable type
                _ if typevar.domain(db) == TypeVarDomain::ParameterSignature => {
                    Some(Err(UnsatisfiableBound))
                }

                // Comparing a typevar with a type
                _ => {
                    let constraint = ConcreteLowerBound::new(db, provenance, typevar, bound).into();
                    Some(Ok(constraint))
                }
            }
        };

        // It's not useful for a lower bound to be a union type. Because the following equivalence
        // holds, we can break these bounds apart and create an equivalent BDD with more nodes but
        // simpler constraints. (Fewer, simpler constraints mean that our sequent maps won't grow
        // pathologically large.)
        //
        //   (α | β) ≤ T   ⇔ (α ≤ T) ∧ (β ≤ T)
        match bound {
            Type::Union(bound) => Either::Left(
                bound
                    .elements(db)
                    .iter()
                    .copied()
                    .filter_map(choose_lower_bound),
            ),
            _ => Either::Right(choose_lower_bound(bound).into_iter()),
        }
    }

    /// Returns the constraints that model the requirement that `typevar` must be assignable to
    /// `bound`. Intersection upper bounds are broken apart into separate constraints. We also
    /// return whether each constraint should hold (for positive intersection elements) or not hold
    /// (for negative). Returns no constraints when the relationship always holds (e.g. when
    /// comparing a typevar with itself).
    pub(super) fn new_upper_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> impl Iterator<Item = Result<Self, UnsatisfiableBound>> {
        let choose_upper_bound = move |bound: Type<'db>| {
            let bound = Self::normalize_bound(db, typevar, bound);
            match bound {
                // Two identical typevars must always solve to the same type, so it is not useful to
                // have an upper bound that is the typevar being constrained.
                Type::TypeVar(upper) if typevar.is_same_typevar_as(db, upper) => None,

                // The same applies for an upper bound that's a union containing the typevar
                // being constrained.
                Type::Union(union)
                    if union.elements(db).iter().any(|element| {
                        element.as_typevar().is_some_and(|element_bound_typevar| {
                            typevar.is_same_typevar_as(db, element_bound_typevar)
                        })
                    }) =>
                {
                    None
                }

                // Otherwise we construct the correct upper-bound constraint.

                // Comparing two typevars
                Type::TypeVar(upper) if typevar.domain(db) == upper.domain(db) => {
                    let constraint = TypeVarRangeBound::new(db, provenance, typevar, upper).into();
                    Some(Ok(constraint))
                }
                Type::TypeVar(_) => Some(Err(UnsatisfiableBound)),

                // Comparing a paramspec with a callable type
                Type::Callable(_) if typevar.domain(db) == TypeVarDomain::ParameterSignature => {
                    let constraint =
                        ParamSpecUpperBound::new(db, provenance, typevar, bound).into();
                    Some(Ok(constraint))
                }

                // Cannot compare a paramspec with a non-callable type
                _ if typevar.domain(db) == TypeVarDomain::ParameterSignature => {
                    Some(Err(UnsatisfiableBound))
                }

                // Comparing a typevar with a type
                _ => {
                    let constraint = ConcreteUpperBound::new(db, provenance, typevar, bound).into();
                    Some(Ok(constraint))
                }
            }
        };

        // It's not useful for an upper bound to be an intersection type. Because the following
        // equivalences hold, we can break these bounds apart and create an equivalent BDD with
        // more nodes but simpler constraints. (Fewer, simpler constraints mean that our sequent
        // maps won't grow pathologically large.)
        //
        //   T ≤ (α & β)   ⇔ (T ≤ α) ∧ (T ≤ β)
        //   T ≤ (¬α & ¬β) ⇔ (T ≤ ¬α) ∧ (T ≤ ¬β)
        match bound {
            Type::Intersection(bound) => {
                let positive = bound.iter_positive(db);
                let negative = bound.iter_negative(db).map(|ty| ty.negate(db, env));
                Either::Left(std::iter::chain(positive, negative).filter_map(choose_upper_bound))
            }
            _ => Either::Right(choose_upper_bound(bound).into_iter()),
        }
    }

    /// Returns the constraints that model the requirement that `typevar` must be equivalent to
    /// `bound`.
    ///
    /// A fully static equality is represented by one equivalence constraint when possible. Gradual
    /// bounds are represented by separate lower and upper constraints. We also use the latter
    /// representation when a top-level union or intersection refers to `typevar` itself, so that
    /// the tautological half of the equality can be removed without discarding the other half.
    pub(super) fn new_equivalence_bound(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> impl Iterator<Item = Result<Self, UnsatisfiableBound>> {
        let is_same_typevar = |element: &Type<'db>| {
            element
                .as_typevar()
                .is_some_and(|element| typevar.is_same_typevar_as(db, element))
        };
        let bound_refers_to_typevar = match bound {
            Type::Union(union) => union.elements(db).iter().any(&is_same_typevar),
            Type::Intersection(intersection) => {
                intersection.positive(db).iter().any(&is_same_typevar)
                    || intersection.negative(db).iter().any(is_same_typevar)
            }
            _ => false,
        };

        let normalized_bound = Self::normalize_bound(db, typevar, bound);
        if normalized_bound.bottom_materialization(db, env)
            != normalized_bound.top_materialization(db, env)
            || bound_refers_to_typevar
        {
            return Either::Left(std::iter::chain(
                Self::new_lower_bound(db, provenance, typevar, bound),
                Self::new_upper_bound(db, env, provenance, typevar, bound),
            ));
        }

        let constraint = match normalized_bound {
            // Two identical typevars must always solve to the same type, so it is not useful to
            // have an equivalence bound that is the typevar being constrained.
            Type::TypeVar(bound_typevar) if typevar.is_same_typevar_as(db, bound_typevar) => None,

            // Otherwise we construct the correct equivalence constraint.

            // Comparing two typevars
            Type::TypeVar(bound_typevar) if typevar.domain(db) == bound_typevar.domain(db) => {
                let constraint =
                    TypeVarEquivalenceBound::new(db, provenance, typevar, bound_typevar).into();
                Some(Ok(constraint))
            }
            Type::TypeVar(_) => Some(Err(UnsatisfiableBound)),

            // Comparing a paramspec with a callable type
            Type::Callable(_) if typevar.domain(db) == TypeVarDomain::ParameterSignature => {
                // We already normalized the callable into a paramspec_value above
                let constraint =
                    ParamSpecEquivalenceBound::new(db, provenance, typevar, normalized_bound)
                        .into();
                Some(Ok(constraint))
            }

            // Cannot compare a paramspec with a non-callable type
            _ if typevar.domain(db) == TypeVarDomain::ParameterSignature => {
                Some(Err(UnsatisfiableBound))
            }

            // Comparing a typevar with a type
            _ => {
                let constraint =
                    ConcreteEquivalenceBound::new(db, provenance, typevar, normalized_bound).into();
                Some(Ok(constraint))
            }
        };
        Either::Right(constraint.into_iter())
    }

    fn normalize_bound(
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Type<'db> {
        match bound {
            Type::Callable(callable) if typevar.domain(db) == TypeVarDomain::ParameterSignature => {
                if let [signature] = callable.signatures(db).overloads.as_slice()
                    && signature.generic_context.is_none()
                    && let Some(paramspec) = signature.parameters().as_paramspec()
                {
                    // Callable[P, anything] should be treated the same as P
                    Type::TypeVar(paramspec)
                } else {
                    Type::Callable(callable.into_paramspec_value(db))
                }
            }
            _ => bound,
        }
    }

    pub(super) fn provides_lower(self) -> bool {
        matches!(
            self,
            Constraint::ConcreteLower(_)
                | Constraint::ConcreteEquivalence(_)
                | Constraint::ParamSpecLower(_)
                | Constraint::ParamSpecEquivalence(_)
                | Constraint::TypeVarRange(_)
                | Constraint::TypeVarEquivalence(_)
        )
    }

    pub(super) fn provides_upper(self) -> bool {
        matches!(
            self,
            Constraint::ConcreteUpper(_)
                | Constraint::ConcreteEquivalence(_)
                | Constraint::ParamSpecUpper(_)
                | Constraint::ParamSpecEquivalence(_)
                | Constraint::TypeVarRange(_)
                | Constraint::TypeVarEquivalence(_)
        )
    }

    pub(super) fn as_concrete(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Option<BoundTypeVarInstance<'db>> {
        let bound_is_concrete = |bound: Type<'db>| {
            !bound.has_typevar(db, env)
                && !bound.has_unspecialized_type_var(db, env)
                && bound.bottom_materialization(db, env) == bound.top_materialization(db, env)
        };
        match self {
            Constraint::ConcreteLower(this) => {
                bound_is_concrete(this.bound).then_some(this.typevar)
            }
            Constraint::ConcreteUpper(this) => {
                bound_is_concrete(this.bound).then_some(this.typevar)
            }
            Constraint::ConcreteEquivalence(this) => {
                bound_is_concrete(this.bound).then_some(this.typevar)
            }
            Constraint::ParamSpecLower(this) => {
                bound_is_concrete(this.bound).then_some(this.typevar)
            }
            Constraint::ParamSpecUpper(this) => {
                bound_is_concrete(this.bound).then_some(this.typevar)
            }
            Constraint::ParamSpecEquivalence(this) => {
                bound_is_concrete(this.bound).then_some(this.typevar)
            }
            Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => None,
        }
    }

    pub(crate) fn bound_depth(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> (u16, u16) {
        match self {
            Constraint::ConcreteLower(this) => {
                max_constructor_and_typevar_depth(db, env, this.bound)
            }
            Constraint::ConcreteUpper(this) => {
                max_constructor_and_typevar_depth(db, env, this.bound)
            }
            Constraint::ConcreteEquivalence(this) => {
                max_constructor_and_typevar_depth(db, env, this.bound)
            }
            Constraint::ParamSpecLower(this) => {
                max_constructor_and_typevar_depth(db, env, this.bound)
            }
            Constraint::ParamSpecUpper(this) => {
                max_constructor_and_typevar_depth(db, env, this.bound)
            }
            Constraint::ParamSpecEquivalence(this) => {
                max_constructor_and_typevar_depth(db, env, this.bound)
            }
            Constraint::TypeVarRange(_) | Constraint::TypeVarEquivalence(_) => (0, 0),
        }
    }

    pub(super) fn directly_constrains_inferable_typevar(
        self,
        db: &'db dyn Db,
        inferable: TypeVarSet<'db>,
    ) -> bool {
        match self {
            Constraint::ConcreteLower(this) => this.typevar.is_inferable(db, inferable),
            Constraint::ConcreteUpper(this) => this.typevar.is_inferable(db, inferable),
            Constraint::ConcreteEquivalence(this) => this.typevar.is_inferable(db, inferable),
            Constraint::ParamSpecLower(this) => this.typevar.is_inferable(db, inferable),
            Constraint::ParamSpecUpper(this) => this.typevar.is_inferable(db, inferable),
            Constraint::ParamSpecEquivalence(this) => this.typevar.is_inferable(db, inferable),
            Constraint::TypeVarRange(this) => {
                this.left.is_inferable(db, inferable) || this.right.is_inferable(db, inferable)
            }
            Constraint::TypeVarEquivalence(this) => {
                this.left.is_inferable(db, inferable) || this.right.is_inferable(db, inferable)
            }
        }
    }

    pub(super) fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        match self {
            Constraint::ConcreteLower(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
            Constraint::ConcreteUpper(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
            Constraint::ConcreteEquivalence(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
            Constraint::ParamSpecLower(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
            Constraint::ParamSpecUpper(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
            Constraint::ParamSpecEquivalence(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
            Constraint::TypeVarRange(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
            Constraint::TypeVarEquivalence(this) => {
                this.apply_type_mapping_impl(db, builder, type_mapping, tcx, visitor)
            }
        }
    }

    pub(super) fn types(self) -> impl Iterator<Item = Type<'db>> {
        let types = match self {
            Constraint::ConcreteLower(this) => [Type::TypeVar(this.typevar), this.bound],
            Constraint::ConcreteUpper(this) => [Type::TypeVar(this.typevar), this.bound],
            Constraint::ConcreteEquivalence(this) => [Type::TypeVar(this.typevar), this.bound],
            Constraint::ParamSpecLower(this) => [Type::TypeVar(this.typevar), this.bound],
            Constraint::ParamSpecUpper(this) => [Type::TypeVar(this.typevar), this.bound],
            Constraint::ParamSpecEquivalence(this) => [Type::TypeVar(this.typevar), this.bound],
            Constraint::TypeVarRange(this) => [Type::TypeVar(this.left), Type::TypeVar(this.right)],
            Constraint::TypeVarEquivalence(this) => {
                [Type::TypeVar(this.left), Type::TypeVar(this.right)]
            }
        };
        types.into_iter()
    }

    pub(super) fn display<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        holds: Option<bool>,
    ) -> impl Display + 'a {
        std::fmt::from_fn(move |f| match self {
            Constraint::ConcreteLower(this) => this.display(db, env, holds).fmt(f),
            Constraint::ConcreteUpper(this) => this.display(db, env, holds).fmt(f),
            Constraint::ConcreteEquivalence(this) => this.display(db, env, holds).fmt(f),
            Constraint::ParamSpecLower(this) => this.display(db, env, holds).fmt(f),
            Constraint::ParamSpecUpper(this) => this.display(db, env, holds).fmt(f),
            Constraint::ParamSpecEquivalence(this) => this.display(db, env, holds).fmt(f),
            Constraint::TypeVarRange(this) => this.display(db, holds).fmt(f),
            Constraint::TypeVarEquivalence(this) => this.display(db, holds).fmt(f),
        })
    }
}

pub(super) trait ProvidesConcreteBound<'db>: Copy + Into<Constraint<'db>> {
    fn provenance(self) -> ConstraintProvenance;
    fn typevar(self) -> BoundTypeVarInstance<'db>;
    fn bound(self) -> Type<'db>;
    fn map(self, provenance: ConstraintProvenance, bound: Type<'db>) -> Self;
}

pub(super) trait ProvidesConcreteLowerBound<'db>: ProvidesConcreteBound<'db> {
    type LowerBound: ProvidesConcreteBound<'db>;

    fn into_lower_bound(self) -> Self::LowerBound;
}

pub(super) trait ProvidesConcreteUpperBound<'db>: ProvidesConcreteBound<'db> {
    type UpperBound: ProvidesConcreteBound<'db>;

    fn into_upper_bound(self) -> Self::UpperBound;
}

pub(super) trait ProvidesConcreteEquivalenceBound<'db>: ProvidesConcreteBound<'db> {}

pub(super) trait ProvidesTypeVarBound<'db>: Copy + Into<Constraint<'db>> {
    fn provenance(self) -> ConstraintProvenance;
    fn left(self) -> BoundTypeVarInstance<'db>;
    fn right(self) -> BoundTypeVarInstance<'db>;
}

pub(super) trait ProvidesTypeVarRangeBound<'db>: ProvidesTypeVarBound<'db> {}
pub(super) trait ProvidesTypeVarEquivalenceBound<'db>:
    ProvidesTypeVarRangeBound<'db>
{
}

/// Restricts a single typevar so that a concrete lower bound is assignable to it. (A concrete type
/// is not a bare typevar. [`TypeVarRangeBound`] is used to model an assignability relationship
/// between two typevars.)
///
/// The bound will never be a union type, since union lower bounds can be broken apart into
/// separate constraints for each union element.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ConcreteLowerBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) typevar: BoundTypeVarInstance<'db>,
    pub(super) bound: Type<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> ConcreteLowerBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        // TODO: Handle TypeVarTuple separately
        assert!(matches!(
            typevar.domain(db),
            TypeVarDomain::Type | TypeVarDomain::TypeTuple
        ));
        Self {
            provenance,
            typevar,
            bound,
            _phantom: PhantomData,
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let subject =
            Type::TypeVar(self.typevar).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let bound = self
            .bound
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match subject {
            Type::TypeVar(typevar) => {
                let applied = Constraint::new_lower_bound(db, self.provenance, typevar, bound);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &bound.when_constraint_set_assignable_to_owned(db, env, subject),
            ),
        }
    }

    fn display<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        holds: Option<bool>,
    ) -> impl Display + 'a {
        let range_prefix = match holds {
            Some(true) => "",
            Some(false) => "¬",
            None => "?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "{range_prefix}({} ≤ {})",
                self.bound.display(db, env),
                self.typevar.identity(db).display(db),
            )
        })
    }
}

impl<'db> From<ConcreteLowerBound<'db>> for Constraint<'db> {
    fn from(bound: ConcreteLowerBound<'db>) -> Constraint<'db> {
        Constraint::ConcreteLower(bound)
    }
}

impl<'db> ProvidesConcreteBound<'db> for ConcreteLowerBound<'db> {
    fn provenance(self) -> ConstraintProvenance {
        self.provenance
    }

    fn typevar(self) -> BoundTypeVarInstance<'db> {
        self.typevar
    }

    fn bound(self) -> Type<'db> {
        self.bound
    }

    fn map(self, provenance: ConstraintProvenance, bound: Type<'db>) -> Self {
        Self {
            provenance,
            typevar: self.typevar,
            bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteLowerBound<'db> for ConcreteLowerBound<'db> {
    type LowerBound = ConcreteLowerBound<'db>;

    fn into_lower_bound(self) -> ConcreteLowerBound<'db> {
        self
    }
}

/// Restricts a single typevar so that it is assignable to a concrete upper bound. (A concrete type
/// is not a bare typevar. [`TypeVarRangeBound`] is used to model an assignability relationship
/// between two typevars.)
///
/// The bound will never be an intersection type, since intersection upper bounds can be broken
/// apart into separate constraints for each intersection element.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ConcreteUpperBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) typevar: BoundTypeVarInstance<'db>,
    pub(super) bound: Type<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> ConcreteUpperBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        // TODO: Handle TypeVarTuple separately
        assert!(matches!(
            typevar.domain(db),
            TypeVarDomain::Type | TypeVarDomain::TypeTuple
        ));
        Self {
            provenance,
            typevar,
            bound,
            _phantom: PhantomData,
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let subject =
            Type::TypeVar(self.typevar).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let bound = self
            .bound
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match subject {
            Type::TypeVar(typevar) => {
                let applied = Constraint::new_upper_bound(db, env, self.provenance, typevar, bound);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &subject.when_constraint_set_assignable_to_owned(db, env, bound),
            ),
        }
    }

    fn display<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        holds: Option<bool>,
    ) -> impl Display + 'a {
        let range_prefix = match holds {
            Some(true) => "",
            Some(false) => "¬",
            None => "?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "{range_prefix}({} ≤ {})",
                self.typevar.identity(db).display(db),
                self.bound.display(db, env),
            )
        })
    }
}

impl<'db> From<ConcreteUpperBound<'db>> for Constraint<'db> {
    fn from(bound: ConcreteUpperBound<'db>) -> Constraint<'db> {
        Constraint::ConcreteUpper(bound)
    }
}

impl<'db> ProvidesConcreteBound<'db> for ConcreteUpperBound<'db> {
    fn provenance(self) -> ConstraintProvenance {
        self.provenance
    }

    fn typevar(self) -> BoundTypeVarInstance<'db> {
        self.typevar
    }

    fn bound(self) -> Type<'db> {
        self.bound
    }

    fn map(self, provenance: ConstraintProvenance, bound: Type<'db>) -> Self {
        Self {
            provenance,
            typevar: self.typevar,
            bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteUpperBound<'db> for ConcreteUpperBound<'db> {
    type UpperBound = ConcreteUpperBound<'db>;

    fn into_upper_bound(self) -> ConcreteUpperBound<'db> {
        self
    }
}

/// Restricts a single typevar so that it is equivalent to some concrete type. (A concrete type is
/// not a bare typevar. [`TypeVarEquivalenceBound`] is used to model an equivalence relationship
/// between two typevars.)
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ConcreteEquivalenceBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) typevar: BoundTypeVarInstance<'db>,
    pub(super) bound: Type<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> ConcreteEquivalenceBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        // TODO: Handle TypeVarTuple separately
        assert!(matches!(
            typevar.domain(db),
            TypeVarDomain::Type | TypeVarDomain::TypeTuple
        ));
        Self {
            provenance,
            typevar,
            bound,
            _phantom: PhantomData,
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let subject =
            Type::TypeVar(self.typevar).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let bound = self
            .bound
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match subject {
            Type::TypeVar(typevar) => {
                let applied =
                    Constraint::new_equivalence_bound(db, env, self.provenance, typevar, bound);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &subject.when_constraint_set_equivalent_to_owned(db, env, bound),
            ),
        }
    }

    fn display<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        holds: Option<bool>,
    ) -> impl Display + 'a {
        let equality_sign = match holds {
            Some(true) => "=",
            Some(false) => "≠",
            None => "=?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "({} {equality_sign} {})",
                self.typevar.identity(db).display(db),
                self.bound.display(db, env),
            )
        })
    }
}

impl<'db> From<ConcreteEquivalenceBound<'db>> for Constraint<'db> {
    fn from(bound: ConcreteEquivalenceBound<'db>) -> Constraint<'db> {
        Constraint::ConcreteEquivalence(bound)
    }
}

impl<'db> ProvidesConcreteBound<'db> for ConcreteEquivalenceBound<'db> {
    fn provenance(self) -> ConstraintProvenance {
        self.provenance
    }

    fn typevar(self) -> BoundTypeVarInstance<'db> {
        self.typevar
    }

    fn bound(self) -> Type<'db> {
        self.bound
    }

    fn map(self, provenance: ConstraintProvenance, bound: Type<'db>) -> Self {
        Self {
            provenance,
            typevar: self.typevar,
            bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteLowerBound<'db> for ConcreteEquivalenceBound<'db> {
    type LowerBound = ConcreteLowerBound<'db>;

    fn into_lower_bound(self) -> ConcreteLowerBound<'db> {
        ConcreteLowerBound {
            provenance: self.provenance,
            typevar: self.typevar,
            bound: self.bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteUpperBound<'db> for ConcreteEquivalenceBound<'db> {
    type UpperBound = ConcreteUpperBound<'db>;

    fn into_upper_bound(self) -> ConcreteUpperBound<'db> {
        ConcreteUpperBound {
            provenance: self.provenance,
            typevar: self.typevar,
            bound: self.bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteEquivalenceBound<'db> for ConcreteEquivalenceBound<'db> {}

/// Restricts a single paramspec so that a concrete lower bound signature is assignable to it. (A
/// concrete type is not a bare typevar. [`TypeVarRangeBound`] is used to model an assignability
/// relationship between two paramspecs.)
///
/// The bound will never be a union type, since union lower bounds can be broken apart into
/// separate constraints for each union element.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ParamSpecLowerBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) typevar: BoundTypeVarInstance<'db>,
    pub(super) bound: Type<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> ParamSpecLowerBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        assert_eq!(typevar.domain(db), TypeVarDomain::ParameterSignature);
        Self {
            provenance,
            typevar,
            bound,
            _phantom: PhantomData,
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let subject =
            Type::TypeVar(self.typevar).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let bound = self
            .bound
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match subject {
            Type::TypeVar(typevar) => {
                let applied = Constraint::new_lower_bound(db, self.provenance, typevar, bound);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &bound.when_constraint_set_assignable_to_owned(db, env, subject),
            ),
        }
    }

    fn display<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        holds: Option<bool>,
    ) -> impl Display + 'a {
        let range_prefix = match holds {
            Some(true) => "",
            Some(false) => "¬",
            None => "?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "{range_prefix}({} ≤ {})",
                self.bound.display(db, env),
                self.typevar.identity(db).display(db),
            )
        })
    }
}

impl<'db> From<ParamSpecLowerBound<'db>> for Constraint<'db> {
    fn from(bound: ParamSpecLowerBound<'db>) -> Constraint<'db> {
        Constraint::ParamSpecLower(bound)
    }
}

impl<'db> ProvidesConcreteBound<'db> for ParamSpecLowerBound<'db> {
    fn provenance(self) -> ConstraintProvenance {
        self.provenance
    }

    fn typevar(self) -> BoundTypeVarInstance<'db> {
        self.typevar
    }

    fn bound(self) -> Type<'db> {
        self.bound
    }

    fn map(self, provenance: ConstraintProvenance, bound: Type<'db>) -> Self {
        Self {
            provenance,
            typevar: self.typevar,
            bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteLowerBound<'db> for ParamSpecLowerBound<'db> {
    type LowerBound = ParamSpecLowerBound<'db>;

    fn into_lower_bound(self) -> ParamSpecLowerBound<'db> {
        self
    }
}

/// Restricts a single paramspec so that it is assignable to a concrete upper bound signature. (A
/// concrete type is not a bare typevar. [`TypeVarRangeBound`] is used to model an assignability
/// relationship between two paramspecs.)
///
/// The bound will never be an intersection type, since intersection upper bounds can be broken
/// apart into separate constraints for each intersection element.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ParamSpecUpperBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) typevar: BoundTypeVarInstance<'db>,
    pub(super) bound: Type<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> ParamSpecUpperBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        assert_eq!(typevar.domain(db), TypeVarDomain::ParameterSignature);
        Self {
            provenance,
            typevar,
            bound,
            _phantom: PhantomData,
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let subject =
            Type::TypeVar(self.typevar).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let bound = self
            .bound
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match subject {
            Type::TypeVar(typevar) => {
                let applied = Constraint::new_upper_bound(db, env, self.provenance, typevar, bound);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &subject.when_constraint_set_assignable_to_owned(db, env, bound),
            ),
        }
    }

    fn display<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        holds: Option<bool>,
    ) -> impl Display + 'a {
        let range_prefix = match holds {
            Some(true) => "",
            Some(false) => "¬",
            None => "?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "{range_prefix}({} ≤ {})",
                self.typevar.identity(db).display(db),
                self.bound.display(db, env),
            )
        })
    }
}

impl<'db> From<ParamSpecUpperBound<'db>> for Constraint<'db> {
    fn from(bound: ParamSpecUpperBound<'db>) -> Constraint<'db> {
        Constraint::ParamSpecUpper(bound)
    }
}

impl<'db> ProvidesConcreteBound<'db> for ParamSpecUpperBound<'db> {
    fn provenance(self) -> ConstraintProvenance {
        self.provenance
    }

    fn typevar(self) -> BoundTypeVarInstance<'db> {
        self.typevar
    }

    fn bound(self) -> Type<'db> {
        self.bound
    }

    fn map(self, provenance: ConstraintProvenance, bound: Type<'db>) -> Self {
        Self {
            provenance,
            typevar: self.typevar,
            bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteUpperBound<'db> for ParamSpecUpperBound<'db> {
    type UpperBound = ParamSpecUpperBound<'db>;

    fn into_upper_bound(self) -> ParamSpecUpperBound<'db> {
        self
    }
}

/// Restricts a single paramspec so that it is equivalent to some concrete signature. (A concrete
/// type is not a bare typevar. [`TypeVarEquivalenceBound`] is used to model an equivalence
/// relationship between two paramspecs.)
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct ParamSpecEquivalenceBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) typevar: BoundTypeVarInstance<'db>,
    pub(super) bound: Type<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> ParamSpecEquivalenceBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Self {
        assert_eq!(typevar.domain(db), TypeVarDomain::ParameterSignature);
        Self {
            provenance,
            typevar,
            bound,
            _phantom: PhantomData,
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let subject =
            Type::TypeVar(self.typevar).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let bound = self
            .bound
            .apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match subject {
            Type::TypeVar(typevar) => {
                let applied =
                    Constraint::new_equivalence_bound(db, env, self.provenance, typevar, bound);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &subject.when_constraint_set_equivalent_to_owned(db, env, bound),
            ),
        }
    }

    fn display<'a>(
        self,
        db: &'db dyn Db,
        env: &'a ProgramEnvironment<'db>,
        holds: Option<bool>,
    ) -> impl Display + 'a {
        let equality_sign = match holds {
            Some(true) => "=",
            Some(false) => "≠",
            None => "=?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "({} {equality_sign} {})",
                self.typevar.identity(db).display(db),
                self.bound.display(db, env),
            )
        })
    }
}

impl<'db> From<ParamSpecEquivalenceBound<'db>> for Constraint<'db> {
    fn from(bound: ParamSpecEquivalenceBound<'db>) -> Constraint<'db> {
        Constraint::ParamSpecEquivalence(bound)
    }
}

impl<'db> ProvidesConcreteBound<'db> for ParamSpecEquivalenceBound<'db> {
    fn provenance(self) -> ConstraintProvenance {
        self.provenance
    }

    fn typevar(self) -> BoundTypeVarInstance<'db> {
        self.typevar
    }

    fn bound(self) -> Type<'db> {
        self.bound
    }

    fn map(self, provenance: ConstraintProvenance, bound: Type<'db>) -> Self {
        Self {
            provenance,
            typevar: self.typevar,
            bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteLowerBound<'db> for ParamSpecEquivalenceBound<'db> {
    type LowerBound = ParamSpecLowerBound<'db>;

    fn into_lower_bound(self) -> ParamSpecLowerBound<'db> {
        ParamSpecLowerBound {
            provenance: self.provenance,
            typevar: self.typevar,
            bound: self.bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteUpperBound<'db> for ParamSpecEquivalenceBound<'db> {
    type UpperBound = ParamSpecUpperBound<'db>;

    fn into_upper_bound(self) -> ParamSpecUpperBound<'db> {
        ParamSpecUpperBound {
            provenance: self.provenance,
            typevar: self.typevar,
            bound: self.bound,
            _phantom: PhantomData,
        }
    }
}

impl<'db> ProvidesConcreteEquivalenceBound<'db> for ParamSpecEquivalenceBound<'db> {}

/// Restricts two typevars so that `left` must be assignable to `right`. Both typevars must have
/// the same domain.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct TypeVarRangeBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) left: BoundTypeVarInstance<'db>,
    pub(super) right: BoundTypeVarInstance<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> TypeVarRangeBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        left: BoundTypeVarInstance<'db>,
        right: BoundTypeVarInstance<'db>,
    ) -> Self {
        assert_eq!(left.domain(db), right.domain(db));
        Self {
            provenance,
            left,
            right,
            _phantom: PhantomData,
        }
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let left = Type::TypeVar(self.left).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let right =
            Type::TypeVar(self.right).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match (left, right) {
            (Type::TypeVar(left_typevar), _) => {
                let applied =
                    Constraint::new_upper_bound(db, env, self.provenance, left_typevar, right);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            (_, Type::TypeVar(right_typevar)) => {
                let applied = Constraint::new_lower_bound(db, self.provenance, right_typevar, left);
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &left.when_constraint_set_assignable_to_owned(db, env, right),
            ),
        }
    }

    fn display(self, db: &'db dyn Db, holds: Option<bool>) -> impl Display {
        let range_prefix = match holds {
            Some(true) => "",
            Some(false) => "¬",
            None => "?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "{range_prefix}({} ≤ {})",
                self.left.identity(db).display(db),
                self.right.identity(db).display(db),
            )
        })
    }
}

impl<'db> From<TypeVarRangeBound<'db>> for Constraint<'db> {
    fn from(bound: TypeVarRangeBound<'db>) -> Constraint<'db> {
        Constraint::TypeVarRange(bound)
    }
}

impl<'db> ProvidesTypeVarBound<'db> for TypeVarRangeBound<'db> {
    fn provenance(self) -> ConstraintProvenance {
        self.provenance
    }

    fn left(self) -> BoundTypeVarInstance<'db> {
        self.left
    }

    fn right(self) -> BoundTypeVarInstance<'db> {
        self.right
    }
}

impl<'db> ProvidesTypeVarRangeBound<'db> for TypeVarRangeBound<'db> {}

/// Restricts two typevars so that `left` must be equivalent to `right`. Both typevars must have
/// the same domain.
///
/// (As an invariant, we canonicalize `left` and `right` so that these bounds are always created
/// with a consistent typevar ordering across the process. This does _not_ affect the BDD variable
/// ordering assigned to this constraint in a particular builder.)
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct TypeVarEquivalenceBound<'db> {
    pub(super) provenance: ConstraintProvenance,
    pub(super) left: BoundTypeVarInstance<'db>,
    pub(super) right: BoundTypeVarInstance<'db>,
    // Always construct via the `new` method
    _phantom: PhantomData<()>,
}

impl<'db> TypeVarEquivalenceBound<'db> {
    pub(super) fn new(
        db: &'db dyn Db,
        provenance: ConstraintProvenance,
        left: BoundTypeVarInstance<'db>,
        right: BoundTypeVarInstance<'db>,
    ) -> Self {
        assert_eq!(left.domain(db), right.domain(db));
        let left_id = left.as_id().as_bits();
        let right_id = right.as_id().as_bits();
        let (left, right) = if wobble_index(left_id) > wobble_index(right_id) {
            (right, left)
        } else {
            (left, right)
        };
        Self {
            provenance,
            left,
            right,
            _phantom: PhantomData,
        }
    }

    pub(super) fn forwards(self) -> impl ProvidesTypeVarEquivalenceBound<'db> {
        #[derive(Clone, Copy)]
        struct Forwards<'db>(TypeVarEquivalenceBound<'db>);

        impl<'db> From<Forwards<'db>> for Constraint<'db> {
            fn from(bound: Forwards<'db>) -> Constraint<'db> {
                Constraint::TypeVarEquivalence(bound.0)
            }
        }

        impl<'db> ProvidesTypeVarBound<'db> for Forwards<'db> {
            fn provenance(self) -> ConstraintProvenance {
                self.0.provenance
            }

            fn left(self) -> BoundTypeVarInstance<'db> {
                self.0.left
            }

            fn right(self) -> BoundTypeVarInstance<'db> {
                self.0.right
            }
        }

        impl<'db> ProvidesTypeVarRangeBound<'db> for Forwards<'db> {}
        impl<'db> ProvidesTypeVarEquivalenceBound<'db> for Forwards<'db> {}

        Forwards(self)
    }

    pub(super) fn backwards(self) -> impl ProvidesTypeVarEquivalenceBound<'db> {
        #[derive(Clone, Copy)]
        struct Backwards<'db>(TypeVarEquivalenceBound<'db>);

        impl<'db> From<Backwards<'db>> for Constraint<'db> {
            fn from(bound: Backwards<'db>) -> Constraint<'db> {
                Constraint::TypeVarEquivalence(bound.0)
            }
        }

        impl<'db> ProvidesTypeVarBound<'db> for Backwards<'db> {
            fn provenance(self) -> ConstraintProvenance {
                self.0.provenance
            }

            #[expect(clippy::misnamed_getters)]
            fn left(self) -> BoundTypeVarInstance<'db> {
                // Reversed!
                self.0.right
            }

            #[expect(clippy::misnamed_getters)]
            fn right(self) -> BoundTypeVarInstance<'db> {
                // Reversed!
                self.0.left
            }
        }

        impl<'db> ProvidesTypeVarRangeBound<'db> for Backwards<'db> {}
        impl<'db> ProvidesTypeVarEquivalenceBound<'db> for Backwards<'db> {}

        Backwards(self)
    }

    fn apply_type_mapping_impl(
        self,
        db: &'db dyn Db,
        builder: &ConstraintSetBuilder<'db>,
        type_mapping: &TypeMapping<'_, 'db>,
        tcx: TypeContext<'db>,
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> (NodeId, Option<SourceOrderId>) {
        let env = visitor.env;
        let left = Type::TypeVar(self.left).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let right =
            Type::TypeVar(self.right).apply_type_mapping_impl(db, type_mapping, tcx, visitor);
        let mut storage = builder.storage.borrow_mut();
        match (left, right) {
            (Type::TypeVar(left_typevar), _) => {
                let applied = Constraint::new_equivalence_bound(
                    db,
                    env,
                    self.provenance,
                    left_typevar,
                    right,
                );
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            (_, Type::TypeVar(right_typevar)) => {
                let applied = Constraint::new_equivalence_bound(
                    db,
                    env,
                    self.provenance,
                    right_typevar,
                    left,
                );
                Constraint::new_nodes(db, env, &mut storage, applied)
            }
            _ => storage.load(
                db,
                env,
                &left.when_constraint_set_equivalent_to_owned(db, env, right),
            ),
        }
    }

    fn display(self, db: &'db dyn Db, holds: Option<bool>) -> impl Display {
        let equality_sign = match holds {
            Some(true) => "=",
            Some(false) => "≠",
            None => "=?",
        };
        std::fmt::from_fn(move |f| {
            write!(
                f,
                "({} {equality_sign} {})",
                self.left.identity(db).display(db),
                self.right.identity(db).display(db),
            )
        })
    }
}

impl<'db> From<TypeVarEquivalenceBound<'db>> for Constraint<'db> {
    fn from(bound: TypeVarEquivalenceBound<'db>) -> Constraint<'db> {
        Constraint::TypeVarEquivalence(bound)
    }
}
