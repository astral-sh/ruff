//! Semantic requests and finite storage steps for shared sequent derivation.

use std::borrow::Cow;
use std::convert::Infallible;
use std::ops::ControlFlow;

use super::{Sequent, SequentMap};
use crate::types::constraints::control::{
    TddControl, TddError, TddWork, Unrestricted, unrestricted,
};
use crate::types::constraints::variables::{
    ConcreteEquivalenceBound, ConcreteLowerBound, ConcreteUpperBound, Constraint,
};
use crate::types::constraints::{
    ALWAYS_FALSE, ConstraintSetBuilder, ConstraintSetStorage, Node, NodeId, OwnedConstraintSet,
    SingleConjunctionScan,
};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarDomain, TypeVarSet};
use crate::types::variance::VarianceInferable;
use crate::types::{
    BoundTypeVarInstance, IntersectionType, MaterializationKind, Type, TypeVarVariance, UnionType,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(super) enum DomainEndpoint {
    Bottom,
    Top,
}
#[derive(Clone, Copy)]
pub(super) enum SequentWork {
    Entry,
    Rule,
    Direction,
    Complete,
}
#[derive(Clone, Copy)]
pub(super) enum SequentBuffer {
    Groups,
    Pending,
}
#[derive(Clone, Copy)]
pub(super) enum GroupedSequentSource<'db> {
    Lower(ConcreteLowerBound<'db>),
    Upper(ConcreteUpperBound<'db>),
    Equivalent(ConcreteEquivalenceBound<'db>),
}
pub(super) enum SequentConsequence<'db> {
    Positive(Constraint<'db>),
    Negative(Constraint<'db>),
}
pub(super) enum ConjunctionStep<'db> {
    Pending,
    Consequence(SequentConsequence<'db>),
    Complete,
}

#[derive(Clone, Copy)]
pub(super) struct SequentFields<'db>(salsa::FieldReads<'db>);
impl<'db> SequentFields<'db> {
    pub(super) fn new(db: &'db dyn Db) -> Self {
        Self(salsa::FieldReads::new(db))
    }
    pub(super) fn identity(self, value: BoundTypeVarInstance<'db>) -> BoundTypeVarIdentity<'db> {
        value.identity_with_fields(self.0)
    }
    pub(super) fn domain(self, value: BoundTypeVarInstance<'db>) -> TypeVarDomain {
        value.domain_with_fields(self.0)
    }
    pub(super) fn is_paramspec(self, value: BoundTypeVarInstance<'db>) -> bool {
        value.is_paramspec_with_fields(self.0)
    }
    pub(super) fn is_nontrivial_intersection(self, value: Type<'db>) -> bool {
        matches!(value, Type::Intersection(value) if !value.is_simple_negation_with_fields(self.0))
    }
    pub(super) fn interned(self) -> salsa::FieldReads<'db> {
        self.0
    }
}

pub(super) trait SequentEffects<'db> {
    type Error;
    fn fields(&self) -> SequentFields<'db>;
    async fn checkpoint(&mut self, work: SequentWork) -> Result<(), Self::Error>;
    async fn domain_endpoint(
        &mut self,
        domain: TypeVarDomain,
        end: DomainEndpoint,
    ) -> Result<Type<'db>, Self::Error>;
    async fn materialize(
        &mut self,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Self::Error>;
    async fn static_eligible(&mut self, ty: Type<'db>) -> Result<bool, Self::Error>;
    async fn variance(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarIdentity<'db>,
    ) -> Result<TypeVarVariance, Self::Error>;
    async fn substitute(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn assignable(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
    async fn equivalent(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
    async fn owned_assignable(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error>;
    async fn owned_equivalent(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error>;
    async fn trivially_disjoint(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Self::Error>;
    async fn union(&mut self, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
    async fn intersection(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    async fn emit(
        &mut self,
        map: &mut SequentMap<'db>,
        sequent: Sequent<Constraint<'db>>,
    ) -> Result<(), Self::Error>;
    async fn prepare_extract(&mut self, map: &SequentMap<'db>) -> Result<(), Self::Error>;
    async fn reserve_group(&mut self, map: &mut SequentMap<'db>) -> Result<(), Self::Error>;
    async fn prepare_shrink(
        &mut self,
        map: &SequentMap<'db>,
        buffer: SequentBuffer,
    ) -> Result<(), Self::Error>;
    async fn conjunction_step(
        &mut self,
        cursor: &mut OwnedConjunctionCursor<'db>,
    ) -> Result<ConjunctionStep<'db>, Self::Error>;
}

pub(super) trait SyncSequentEffects<'db> {
    type Error;
    fn fields(&self) -> SequentFields<'db>;
    fn checkpoint(&mut self, work: SequentWork) -> Result<(), Self::Error>;
    fn domain_endpoint(
        &mut self,
        domain: TypeVarDomain,
        end: DomainEndpoint,
    ) -> Result<Type<'db>, Self::Error>;
    fn materialize(
        &mut self,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Self::Error>;
    fn static_eligible(&mut self, ty: Type<'db>) -> Result<bool, Self::Error>;
    fn variance(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarIdentity<'db>,
    ) -> Result<TypeVarVariance, Self::Error>;
    fn substitute(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;
    fn assignable(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
    fn equivalent(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error>;
    fn owned_assignable(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error>;
    fn owned_equivalent(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error>;
    fn trivially_disjoint(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Self::Error>;
    fn union(&mut self, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error>;
    fn intersection(&mut self, left: Type<'db>, right: Type<'db>)
    -> Result<Type<'db>, Self::Error>;
    fn emit(
        &mut self,
        map: &mut SequentMap<'db>,
        sequent: Sequent<Constraint<'db>>,
    ) -> Result<(), Self::Error>;
    fn prepare_extract(&mut self, map: &SequentMap<'db>) -> Result<(), Self::Error>;
    fn reserve_group(&mut self, map: &mut SequentMap<'db>) -> Result<(), Self::Error>;
    fn prepare_shrink(
        &mut self,
        map: &SequentMap<'db>,
        buffer: SequentBuffer,
    ) -> Result<(), Self::Error>;
    fn conjunction_step(
        &mut self,
        cursor: &mut OwnedConjunctionCursor<'db>,
    ) -> Result<ConjunctionStep<'db>, Self::Error>;
}

pub(super) struct OrdinarySequentEffects<'env, 'db> {
    pub(super) db: &'db dyn Db,
    pub(super) env: &'env ProgramEnvironment<'db>,
}
impl<'db> SyncSequentEffects<'db> for OrdinarySequentEffects<'_, 'db> {
    type Error = Infallible;
    fn fields(&self) -> SequentFields<'db> {
        SequentFields::new(self.db)
    }
    fn checkpoint(&mut self, work: SequentWork) -> Result<(), Self::Error> {
        let _ = work;
        Ok(())
    }
    fn domain_endpoint(
        &mut self,
        domain: TypeVarDomain,
        end: DomainEndpoint,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(match end {
            DomainEndpoint::Bottom => domain.bottom(self.db),
            DomainEndpoint::Top => domain.top(self.db),
        })
    }
    fn materialize(
        &mut self,
        ty: Type<'db>,
        kind: MaterializationKind,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(match kind {
            MaterializationKind::Bottom => ty.bottom_materialization(self.db, self.env),
            MaterializationKind::Top => ty.top_materialization(self.db, self.env),
        })
    }
    fn static_eligible(&mut self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty.is_static_sequent_eligible(self.db, self.env))
    }
    fn variance(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarIdentity<'db>,
    ) -> Result<TypeVarVariance, Self::Error> {
        Ok(ty
            .variance_of(self.db, self.env, variable)
            .evaluate(self.db))
    }
    fn substitute(
        &mut self,
        ty: Type<'db>,
        variable: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(ty.substitute_one_typevar(self.db, self.env, variable, replacement))
    }
    fn assignable(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        Ok(left.is_constraint_set_assignable_to(self.db, self.env, right))
    }
    fn equivalent(&mut self, left: Type<'db>, right: Type<'db>) -> Result<bool, Self::Error> {
        Ok(left.is_constraint_set_equivalent_to(self.db, self.env, right))
    }
    fn owned_assignable(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error> {
        Ok(left.when_constraint_set_assignable_to_owned(self.db, self.env, right))
    }
    fn owned_equivalent(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Cow<'db, OwnedConstraintSet<'db>>, Self::Error> {
        Ok(left.when_constraint_set_equivalent_to_owned(self.db, self.env, right))
    }
    fn trivially_disjoint(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<bool, Self::Error> {
        let builder = ConstraintSetBuilder::new();
        Ok(left
            .when_trivially_disjoint_from(self.db, self.env, right, &builder, TypeVarSet::None)
            .is_trivially_always_satisfied())
    }
    fn union(&mut self, left: Type<'db>, right: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(UnionType::from_two_elements(self.db, self.env, left, right))
    }
    fn intersection(
        &mut self,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(IntersectionType::from_two_elements(
            self.db, self.env, left, right,
        ))
    }
    fn emit(
        &mut self,
        map: &mut SequentMap<'db>,
        sequent: Sequent<Constraint<'db>>,
    ) -> Result<(), Self::Error> {
        map.pending.push(sequent);
        Ok(())
    }
    fn prepare_extract(&mut self, map: &SequentMap<'db>) -> Result<(), Self::Error> {
        let _ = map;
        Ok(())
    }
    fn reserve_group(&mut self, map: &mut SequentMap<'db>) -> Result<(), Self::Error> {
        let _ = map;
        Ok(())
    }
    fn prepare_shrink(
        &mut self,
        map: &SequentMap<'db>,
        buffer: SequentBuffer,
    ) -> Result<(), Self::Error> {
        let _ = (map, buffer);
        Ok(())
    }
    fn conjunction_step(
        &mut self,
        cursor: &mut OwnedConjunctionCursor<'db>,
    ) -> Result<ConjunctionStep<'db>, Self::Error> {
        Ok(unrestricted(cursor.advance_with(&mut Unrestricted)))
    }
}

// This overlay owns an Arc reference, not a borrow from a builder or from itself. The caller
// retains the original owned constraint set while either phase can suspend.
pub(super) struct OwnedConjunctionCursor<'db> {
    storage: ConstraintSetStorage<'db>,
    root: NodeId,
    shape: SingleConjunctionScan,
    phase: ConjunctionPhase,
}
#[derive(Clone, Copy)]
enum ConjunctionPhase {
    Shape,
    Walk(NodeId),
    Done,
}
impl<'db> OwnedConjunctionCursor<'db> {
    pub(super) fn new(set: &OwnedConstraintSet<'db>) -> Self {
        Self {
            storage: set.query_storage(),
            root: set.node,
            shape: SingleConjunctionScan::new(set.node),
            phase: ConjunctionPhase::Shape,
        }
    }
    pub(super) fn root(&self) -> NodeId {
        self.root
    }
    pub(super) fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ConjunctionStep<'db>, TddError<C::Error>> {
        match self.phase {
            ConjunctionPhase::Shape => {
                match self.shape.advance_with(&self.storage, control)? {
                    ControlFlow::Continue(()) => {}
                    ControlFlow::Break(true) => self.phase = ConjunctionPhase::Walk(self.root),
                    ControlFlow::Break(false) => {
                        self.phase = ConjunctionPhase::Done;
                        return Ok(ConjunctionStep::Complete);
                    }
                }
                Ok(ConjunctionStep::Pending)
            }
            ConjunctionPhase::Walk(node) => {
                control.admit(TddWork::Advance)?;
                match node.node() {
                    Node::AlwaysTrue | Node::AlwaysFalse => {
                        self.phase = ConjunctionPhase::Done;
                        Ok(ConjunctionStep::Complete)
                    }
                    Node::Interior(interior) => {
                        let data = self.storage.interior_node_data(interior.node());
                        let derived = self.storage.constraint_data(data.constraint);
                        let consequence = if data.if_true != ALWAYS_FALSE {
                            self.phase = ConjunctionPhase::Walk(data.if_true);
                            SequentConsequence::Positive(derived)
                        } else {
                            self.phase = ConjunctionPhase::Walk(data.if_false);
                            SequentConsequence::Negative(derived)
                        };
                        Ok(ConjunctionStep::Consequence(consequence))
                    }
                }
            }
            ConjunctionPhase::Done => Ok(ConjunctionStep::Complete),
        }
    }
}
