//! Admitted construction of a fixed pair of type-variable occurrences on the original builder.

use std::ops::ControlFlow;

use super::control::{
    AllocationKind, TableKind, TddControl, TddError, TddWork, hash_access, reserve_map,
    reserve_smallvec, reserve_vec,
};
use super::source_order::PendingSourceOrder;
use super::storage::PendingNode;
use super::support::Support;
use super::variables::{Constraint, ConstraintProvenance, TypeVarEquivalenceBound};
use super::{
    ALWAYS_FALSE, ALWAYS_TRUE, ConstraintId, ConstraintSet, ConstraintSetBuilder,
    ConstraintSetStorage, InteriorNodeData, NodeId, SourceOrder, SourceOrderId, TypeVarId,
};
use crate::Db;
use crate::types::BoundTypeVarInstance;
use crate::types::typevar::BoundTypeVarIdentity;

#[cfg(test)]
pub(super) mod tests;

const WORD_BATCH: usize = 64;

/// This constructor does not initialize the identity caches of a compacted builder.
#[derive(Debug, Eq, PartialEq)]
pub(in crate::types) struct UnsupportedCompactedBuilder;

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
}

enum Pending<'db> {
    Requested,
    Canonical,
    Occurrence {
        pair: TypeVarEquivalenceBound<'db>,
        side: Side,
    },
    Fill {
        pair: TypeVarEquivalenceBound<'db>,
        side: Side,
        id: TypeVarId,
        extent: usize,
        reserved: bool,
    },
    Constraint(TypeVarEquivalenceBound<'db>),
    Node {
        constraint: ConstraintId,
        cursor: PendingNode,
    },
    Source {
        node: NodeId,
        cursor: PendingSourceOrder,
    },
    Done(NodeId, Option<SourceOrderId>),
}

pub(in crate::types) struct PendingTypevarEquivalence<'db, 'c> {
    db: &'db dyn Db,
    builder: &'c ConstraintSetBuilder<'db>,
    requested: BoundTypeVarInstance<'db>,
    bound: BoundTypeVarInstance<'db>,
    support: Support,
    pending: Pending<'db>,
}

impl<'db, 'c> PendingTypevarEquivalence<'db, 'c> {
    pub(in crate::types) fn new(
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        requested: BoundTypeVarInstance<'db>,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<Self, UnsupportedCompactedBuilder> {
        if builder.storage.borrow().compacted.is_some() {
            return Err(UnsupportedCompactedBuilder);
        }
        Ok(Self {
            db,
            builder,
            requested,
            bound,
            support: Support::default(),
            pending: Pending::Requested,
        })
    }

    /// Abandon this owner after refusal. Complete identities and admitted capacity can be reused
    /// by a fresh owner; every return releases the builder's mutable storage borrow.
    pub(in crate::types) fn advance_with<C: TddControl>(
        &mut self,
        control: &mut C,
    ) -> Result<ControlFlow<ConstraintSet<'db, 'c>>, TddError<C::Error>> {
        control.admit(TddWork::Advance)?;
        match &mut self.pending {
            Pending::Requested => {
                self.builder
                    .storage
                    .borrow_mut()
                    .intern_typevar_controlled(self.db, self.requested, control)?;
                self.pending = Pending::Canonical;
            }
            Pending::Canonical => {
                self.pending = match TypeVarEquivalenceBound::new_if_nontrivial(
                    self.db,
                    ConstraintProvenance::Evidence,
                    self.requested,
                    self.bound,
                ) {
                    None => Pending::Done(ALWAYS_TRUE, None),
                    Some(Err(_)) => Pending::Done(ALWAYS_FALSE, None),
                    Some(Ok(pair)) => Pending::Occurrence {
                        pair,
                        side: Side::Left,
                    },
                };
            }
            Pending::Occurrence { pair, side } => {
                let typevar = match side {
                    Side::Left => pair.left,
                    Side::Right => pair.right,
                };
                let id = self
                    .builder
                    .storage
                    .borrow_mut()
                    .intern_typevar_controlled(self.db, typevar, control)?;
                self.pending = Pending::Fill {
                    pair: *pair,
                    side: *side,
                    id,
                    extent: Support::words_needed(id),
                    reserved: false,
                };
            }
            Pending::Fill {
                pair,
                side,
                id,
                extent,
                reserved,
            } => {
                if !*reserved {
                    let additional = extent.saturating_sub(self.support.words().len());
                    reserve_smallvec(
                        self.support.words_mut(),
                        additional,
                        AllocationKind::SupportWords,
                        control,
                    )?;
                    *reserved = true;
                }
                if self.support.words().len() < *extent {
                    let words = (*extent - self.support.words().len()).min(WORD_BATCH);
                    control.admit(TddWork::SupportWords { words })?;
                    let len = self.support.words().len() + words;
                    self.support.words_mut().resize(len, 0);
                    return Ok(ControlFlow::Continue(()));
                }
                control.admit(TddWork::SupportWords { words: 1 })?;
                // The extent is already initialized, so insert only sets this occurrence's bit.
                self.support.insert(*id);
                self.pending = match side {
                    Side::Left => Pending::Occurrence {
                        pair: *pair,
                        side: Side::Right,
                    },
                    Side::Right => Pending::Constraint(*pair),
                };
            }
            Pending::Constraint(pair) => {
                let constraint = self
                    .builder
                    .storage
                    .borrow_mut()
                    .intern_constraint_controlled((*pair).into(), &mut self.support, control)?;
                // A cache hit still constructed the temporary support, as ordinary interning does.
                self.support = Support::default();
                self.pending = Pending::Node {
                    constraint,
                    cursor: PendingNode::new(
                        InteriorNodeData {
                            constraint,
                            if_true: ALWAYS_TRUE,
                            if_uncertain: ALWAYS_FALSE,
                            if_false: ALWAYS_FALSE,
                        },
                        None,
                        true,
                    ),
                };
            }
            Pending::Node { constraint, cursor } => {
                let result = cursor.advance(&mut self.builder.storage.borrow_mut(), control)?;
                if let ControlFlow::Break(node) = result {
                    self.pending = Pending::Source {
                        node,
                        cursor: PendingSourceOrder::new(SourceOrder::Constraint(*constraint)),
                    };
                }
                // The graph cursor and its borrow end before source-order processing begins.
            }
            Pending::Source { node, cursor } => {
                if let ControlFlow::Break(source) =
                    cursor.advance_with(&mut self.builder.storage.borrow_mut(), control)?
                {
                    let node = *node;
                    self.pending = Pending::Done(node, Some(source));
                    return Ok(ControlFlow::Break(ConstraintSet::from_node(
                        self.builder,
                        node,
                        Some(source),
                    )));
                }
            }
            Pending::Done(node, source) => {
                return Ok(ControlFlow::Break(ConstraintSet::from_node(
                    self.builder,
                    *node,
                    *source,
                )));
            }
        }
        Ok(ControlFlow::Continue(()))
    }
}

fn check_id_length<E>(len: usize, maximum: usize) -> Result<(), TddError<E>> {
    if len > maximum {
        return Err(TddError::CapacityExhausted);
    }
    Ok(())
}

impl<'db> ConstraintSetStorage<'db> {
    fn intern_typevar_controlled<C: TddControl>(
        &mut self,
        db: &'db dyn Db,
        typevar: BoundTypeVarInstance<'db>,
        control: &mut C,
    ) -> Result<TypeVarId, TddError<C::Error>> {
        let identity = typevar.identity(db);
        self.intern_typevar_identity_controlled(identity, typevar, control)
    }

    pub(super) fn intern_typevar_identity_controlled<C: TddControl>(
        &mut self,
        identity: BoundTypeVarIdentity<'db>,
        typevar: BoundTypeVarInstance<'db>,
        control: &mut C,
    ) -> Result<TypeVarId, TddError<C::Error>> {
        hash_access(control, TableKind::Typevars, self.typevar_cache.capacity())?;
        if let Some(id) = self.typevar_cache.get(&identity) {
            return Ok(*id);
        }
        check_id_length::<C::Error>(self.typevars.len(), TypeVarId::MAX_VALUE as usize)?;
        reserve_vec(&mut self.typevars.raw, 1, AllocationKind::Typevars, control)?;
        reserve_map(&mut self.typevar_cache, TableKind::Typevars, control)?;
        control.admit(TddWork::Commit)?;
        Ok(self.publish_typevar_miss(identity, typevar))
    }

    pub(super) fn intern_constraint_controlled<C: TddControl>(
        &mut self,
        data: Constraint<'db>,
        support: &mut Support,
        control: &mut C,
    ) -> Result<ConstraintId, TddError<C::Error>> {
        hash_access(
            control,
            TableKind::Constraints,
            self.constraint_cache.capacity(),
        )?;
        if let Some(id) = self.constraint_cache.get(&data) {
            return Ok(*id);
        }
        check_id_length::<C::Error>(self.constraints.len(), ConstraintId::MAX_VALUE as usize)?;
        check_id_length::<C::Error>(
            self.constraint_supports.len(),
            ConstraintId::MAX_VALUE as usize,
        )?;
        check_id_length::<C::Error>(self.supports.len(), (u32::MAX - 1) as usize)?;
        reserve_vec(&mut self.supports.raw, 1, AllocationKind::Supports, control)?;
        reserve_vec(
            &mut self.constraints.raw,
            1,
            AllocationKind::Constraints,
            control,
        )?;
        reserve_vec(
            &mut self.constraint_supports.raw,
            1,
            AllocationKind::ConstraintSupports,
            control,
        )?;
        reserve_map(&mut self.constraint_cache, TableKind::Constraints, control)?;
        control.admit(TddWork::Commit)?;
        let support = std::mem::take(support);
        Ok(self.publish_constraint_miss(data, support))
    }
}
