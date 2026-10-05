//! Tuple values retain their contexts while their elements run in the current inference invocation.

use std::borrow::Cow;
use std::vec;

use super::*;
use crate::types::tuple::TupleSpec;

/// Element contexts prepared before evaluating a tuple's source expressions.
/// The resized specification remains alive until the last element has returned.
#[derive(Debug)]
pub(in crate::types::infer::builder) struct Prepared<'db> {
    pub(in crate::types::infer::builder) _specification: Option<TupleSpec<'db>>,
    pub(in crate::types::infer::builder) annotations: vec::IntoIter<Type<'db>>,
    pub(in crate::types::infer::builder) can_use_type_context: bool,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::types::infer::builder) enum Phase<'db> {
    Start,
    Cache,
    Select,
    Prepare,
    Elements,
    Waiting,
    Build,
    Complete(Type<'db>),
}

/// One tuple's preparation and source cursor, retained in its invocation's owner slot.
/// A speculative builder belongs to `BuilderStore`; this state retains only its identity.
pub(in crate::types::infer::builder) struct State<'db, 'expr> {
    pub(in crate::types::infer::builder) tuple: &'expr ast::ExprTuple,
    pub(in crate::types::infer::builder) root: BuilderId,
    pub(in crate::types::infer::builder) active: BuilderId,
    pub(in crate::types::infer::builder) original_context: TypeContext<'db>,
    pub(in crate::types::infer::builder) context: TypeContext<'db>,
    pub(in crate::types::infer::builder) phase: Phase<'db>,
    pub(in crate::types::infer::builder) targets: Option<Cow<'db, [Type<'db>]>>,
    pub(in crate::types::infer::builder) target_index: usize,
    pub(in crate::types::infer::builder) teardown_cache: bool,
    pub(in crate::types::infer::builder) prepared: Prepared<'db>,
    pub(in crate::types::infer::builder) remaining: &'expr [ast::Expr],
    #[cfg(test)]
    lifetime: Option<OwnerLifetime>,
    #[cfg(all(test, feature = "experimental-analysis"))]
    pub(super) preparation_lifetime:
        Option<crate::types::infer::source_runtime::tests::contextual_tuple::OwnerLifetime>,
}

impl<'db, 'expr> State<'db, 'expr> {
    /// Creates a cursor without allocating annotation or target buffers.
    fn new(builder: BuilderId, tuple: &'expr ast::ExprTuple, context: TypeContext<'db>) -> Self {
        Self {
            tuple,
            root: builder,
            active: builder,
            original_context: context,
            context,
            phase: Phase::Start,
            targets: None,
            target_index: 0,
            teardown_cache: false,
            prepared: Prepared {
                _specification: None,
                annotations: Vec::new().into_iter(),
                can_use_type_context: true,
            },
            remaining: &tuple.elts,
            #[cfg(test)]
            lifetime: None,
            #[cfg(all(test, feature = "experimental-analysis"))]
            preparation_lifetime: None,
        }
    }
}

#[derive(Debug)]
pub(super) struct Active(pub(super) usize);
#[derive(Debug)]
pub(super) struct Waiting(pub(super) usize);
#[derive(Debug)]
pub(super) struct Finished(pub(super) usize);

/// The next operation after a shared tuple transition has updated its retained state.
#[derive(Debug)]
pub(in crate::types::infer::builder) enum Action<'db, 'expr> {
    Continue,
    Infer {
        builder: BuilderId,
        expression: &'expr ast::Expr,
        context: TypeContext<'db>,
    },
    Complete,
}

#[derive(Debug)]
pub(super) enum Step<'db, 'expr> {
    Continue(Active),
    Infer {
        owner: Waiting,
        builder: BuilderId,
        expression: &'expr ast::Expr,
        context: TypeContext<'db>,
    },
    Complete(Finished),
}

/// Borrows a tuple payload through a transition or completion without moving its buffers.
/// On interruption the invocation still owns the payload and drains children before retiring it.
pub(super) struct Lease<'owner, 'db, 'expr> {
    pub(super) state: &'owner mut State<'db, 'expr>,
}

impl<'db, 'expr, B, T> LocalOwners<'db, 'expr, B, T> {
    pub(super) fn push_tuple_value(
        &mut self,
        builder: BuilderId,
        tuple: &'expr ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> Active {
        let index = self.slots.len();
        let state = State::new(builder, tuple, context);
        #[cfg(test)]
        let mut state = state;
        #[cfg(test)]
        let identity = self.next_identity;
        #[cfg(test)]
        {
            self.next_identity += 1;
            state.lifetime = self.observer.as_ref().map(|observer| OwnerLifetime {
                identity,
                observer: observer.clone(),
            });
        }
        self.slots.push(OwnerSlot::TupleExpression(state));
        #[cfg(test)]
        if let Some(observer) = &self.observer {
            observer(OwnershipEvent::Created {
                kind: OwnerKind::TupleExpression,
                call: tuple.range(),
                index,
                identity,
                len: self.slots.len(),
                capacity: self.slots.capacity(),
            });
        }
        Active(index)
    }

    /// Borrows the tuple owner identified by a phase token.
    /// Phase tokens are created only for the last live slot in this invocation.
    pub(super) fn tuple_value_lease(&mut self, index: usize) -> Lease<'_, 'db, 'expr> {
        assert_eq!(index + 1, self.slots.len());
        let OwnerSlot::TupleExpression(state) = &mut self.slots[index] else {
            panic!("tuple expression token does not identify a tuple owner");
        };
        Lease { state }
    }

    pub(super) fn tuple_value_step(index: usize, action: Action<'db, 'expr>) -> Step<'db, 'expr> {
        match action {
            Action::Continue => Step::Continue(Active(index)),
            Action::Infer {
                builder,
                expression,
                context,
            } => Step::Infer {
                owner: Waiting(index),
                builder,
                expression,
                context,
            },
            Action::Complete => Step::Complete(Finished(index)),
        }
    }

    pub(super) fn retire_tuple_value(&mut self, owner: Finished) -> Type<'db> {
        let lease = self.tuple_value_lease(owner.0);
        let Phase::Complete(ty) = lease.state.phase else {
            panic!("completed tuple expression token does not identify a completed owner");
        };
        self.slots[owner.0] = OwnerSlot::Taken;
        self.retire(FinishedOwner { index: owner.0, ty })
    }
}
