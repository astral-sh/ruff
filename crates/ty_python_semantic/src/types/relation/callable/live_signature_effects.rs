//! Signature dependencies retain the exact live checker view while their caller is suspended.
//!
//! This queue is a representation boundary. Its consumer must admit and drive each dependency;
//! it does not execute recursive relation, solving, or mapping operations itself.
//! Structural combinations and folds retain their ordinary inline execution, so this provider
//! does not yet supervise their work through the runtime.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::{Future, poll_fn, ready};
use std::ops::ControlFlow;
use std::rc::Rc;
use std::task::{Poll, Waker};

use crate::Db;
use crate::types::callable::{CallableType, CallableTypes};
use crate::types::constraints::{
    ConstraintFold, ConstraintFoldKind, ConstraintSet, ConstraintSetBuilder,
};
use crate::types::generics::GenericContext;
use crate::types::relation::TypeRelationChecker;
use crate::types::signatures::effects::{
    ConstraintBound, LegacyInlineEffects, SignatureEffects, SignatureVisit, sealed,
};
use crate::types::typevar::{TypeVarNonce, TypeVarSet};
use crate::types::{BoundTypeVarInstance, Parameter, Parameters, Signature, Type, UnionBuilder};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum UnsupportedSignatureDependency {
    Disjointness,
    Freshening,
    AggregateInspection,
    UnionNormalization,
    AliasResolution,
    ParameterInspection,
    ParameterExpansion,
    VariadicNormalization,
    TupleNormalization,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum SignatureQueueError {
    Unsupported(UnsupportedSignatureDependency),
    Cancelled,
}

type Answer<T> = Result<T, SignatureQueueError>;

struct ReplyState<T> {
    answer: Option<Answer<T>>,
    waker: Option<Waker>,
}

/// A dependency has one response type and one consumer. Dropping it interrupts the caller.
pub(in crate::types) struct SignatureReply<T> {
    state: Rc<RefCell<ReplyState<T>>>,
    active: Rc<Cell<bool>>,
}

impl<T> SignatureReply<T> {
    pub(in crate::types) fn respond(self, answer: Answer<T>) -> Answer<()> {
        if !self.active.get() {
            return Err(SignatureQueueError::Cancelled);
        }
        let waker = {
            let mut state = self.state.borrow_mut();
            state.answer = Some(answer);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(())
    }
}

impl<T> Drop for SignatureReply<T> {
    fn drop(&mut self) {
        if !self.active.get() {
            return;
        }
        let waker = {
            let mut state = self.state.borrow_mut();
            if state.answer.is_some() {
                return;
            }
            state.answer = Some(Err(SignatureQueueError::Cancelled));
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

pub(in crate::types) enum SignatureDependency<'db, 'c> {
    Relate {
        source: Type<'db>,
        target: Type<'db>,
        reply: SignatureReply<ConstraintSet<'db, 'c>>,
    },
    IsNever {
        constraints: ConstraintSet<'db, 'c>,
        reply: SignatureReply<bool>,
    },
    IsAlways {
        constraints: ConstraintSet<'db, 'c>,
        reply: SignatureReply<bool>,
    },
    ConstraintBound {
        kind: ConstraintBound,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
        reply: SignatureReply<ConstraintSet<'db, 'c>>,
    },
    ReceiverConstraints {
        signature: Signature<'db>,
        reply: SignatureReply<ConstraintSet<'db, 'c>>,
    },
    ReduceInferable {
        constraints: ConstraintSet<'db, 'c>,
        inferable: TypeVarSet<'db>,
        reply: SignatureReply<ConstraintSet<'db, 'c>>,
    },
    MaxFreshness {
        signature: Signature<'db>,
        context: GenericContext<'db>,
        reply: SignatureReply<Option<TypeVarNonce>>,
    },
    SignatureTypevars {
        signature: Signature<'db>,
        reply: SignatureReply<TypeVarSet<'db>>,
    },
}

pub(in crate::types) struct LiveSignatureRequest<'state, 'db, 'c> {
    pub(in crate::types) checker: TypeRelationChecker<'state, 'c, 'db>,
    pub(in crate::types) dependency: SignatureDependency<'db, 'c>,
}

struct QueueEntry<'state, 'db, 'c> {
    active: Rc<Cell<bool>>,
    request: LiveSignatureRequest<'state, 'db, 'c>,
}

#[derive(Default)]
pub(in crate::types) struct LiveSignatureEffects<'state, 'db, 'c> {
    requests: RefCell<VecDeque<QueueEntry<'state, 'db, 'c>>>,
}

struct DemandLease<'queue, 'state, 'db, 'c> {
    queue: &'queue LiveSignatureEffects<'state, 'db, 'c>,
    active: Rc<Cell<bool>>,
}

impl Drop for DemandLease<'_, '_, '_, '_> {
    fn drop(&mut self) {
        self.active.set(false);
        self.queue
            .requests
            .borrow_mut()
            .retain(|entry| !Rc::ptr_eq(&entry.active, &self.active));
    }
}

impl<'state, 'db, 'c> LiveSignatureEffects<'state, 'db, 'c> {
    pub(in crate::types) fn take_request(&self) -> Option<LiveSignatureRequest<'state, 'db, 'c>> {
        self.requests
            .borrow_mut()
            .pop_front()
            .map(|entry| entry.request)
    }

    /// The caller keeps shared visitors and diagnostic scopes alive through completion or drop.
    pub(in crate::types) async fn compare_pair(
        &self,
        db: &'db dyn Db,
        checker: TypeRelationChecker<'state, 'c, 'db>,
        source: CallableType<'db>,
        target: CallableType<'db>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        checker
            .check_callable_pair_with(db, self, source, target)
            .await
    }

    pub(in crate::types) async fn compare_callables(
        &self,
        db: &'db dyn Db,
        checker: TypeRelationChecker<'state, 'c, 'db>,
        sources: CallableTypes<'db>,
        target: CallableType<'db>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        checker
            .check_callables_vs_callable_with(db, self, &sources, target)
            .await
    }

    async fn demand<T>(
        &self,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        dependency: impl FnOnce(SignatureReply<T>) -> SignatureDependency<'db, 'c>,
    ) -> Answer<T> {
        let active = Rc::new(Cell::new(true));
        let state = Rc::new(RefCell::new(ReplyState {
            answer: None,
            waker: None,
        }));
        let lease = DemandLease {
            queue: self,
            active: Rc::clone(&active),
        };
        self.requests.borrow_mut().push_back(QueueEntry {
            active: Rc::clone(&active),
            request: LiveSignatureRequest {
                checker: checker.clone(),
                dependency: dependency(SignatureReply {
                    state: Rc::clone(&state),
                    active,
                }),
            },
        });
        let answer = poll_fn(|cx| {
            let mut state = state.borrow_mut();
            if let Some(answer) = state.answer.take() {
                Poll::Ready(answer)
            } else {
                state.waker = Some(cx.waker().clone());
                Poll::Pending
            }
        })
        .await;
        drop(lease);
        answer
    }
}

impl sealed::Sealed for LiveSignatureEffects<'_, '_, '_> {}

macro_rules! unsupported_effects {
    ($(fn $method:ident($($argument:ident: $ty:ty),*) -> $output:ty => $operation:ident;)*) => {
        $(fn $method(
            &self,
            _db: &'db dyn Db,
            _checker: &TypeRelationChecker<'state, 'c, 'db>,
            $($argument: $ty,)*
        ) -> impl Future<Output = Answer<$output>> {
            let _ = ($($argument,)*);
            ready(Err(SignatureQueueError::Unsupported(UnsupportedSignatureDependency::$operation)))
        })*
    };
}

impl<'state, 'db, 'c> SignatureEffects<'state, 'db, 'c> for LiveSignatureEffects<'state, 'db, 'c> {
    type Error = SignatureQueueError;

    async fn combine_constraints(
        &self,
        db: &'db dyn Db,
        builder: &'c ConstraintSetBuilder<'db>,
        kind: ConstraintFoldKind,
        left: ConstraintSet<'db, 'c>,
        right: ConstraintSet<'db, 'c>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        LegacyInlineEffects
            .combine_constraints(db, builder, kind, left, right)
            .await
            .map_err(|never| match never {})
    }

    async fn push_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
        next: ConstraintSet<'db, 'c>,
    ) -> Answer<ControlFlow<ConstraintSet<'db, 'c>>> {
        LegacyInlineEffects
            .push_constraints(db, fold, next)
            .await
            .map_err(|never| match never {})
    }

    async fn finish_constraints(
        &self,
        db: &'db dyn Db,
        fold: &mut ConstraintFold<'db, 'c>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        LegacyInlineEffects
            .finish_constraints(db, fold)
            .await
            .map_err(|never| match never {})
    }

    async fn begin_signature_visit<'visit>(
        &self,
        checker: &'visit TypeRelationChecker<'state, 'c, 'db>,
        source: &Signature<'db>,
        target: &Signature<'db>,
    ) -> Answer<SignatureVisit<'visit, 'db>> {
        LegacyInlineEffects
            .begin_signature_visit(checker, source, target)
            .await
            .map_err(|never| match never {})
    }

    async fn relate(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        self.demand(checker, |reply| SignatureDependency::Relate {
            source,
            target,
            reply,
        })
        .await
    }

    async fn is_never(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Answer<bool> {
        if constraints.is_trivially_never_satisfied() || constraints.is_trivially_always_satisfied()
        {
            return Ok(constraints.is_trivially_never_satisfied());
        }
        self.demand(checker, |reply| SignatureDependency::IsNever {
            constraints,
            reply,
        })
        .await
    }

    async fn is_always(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
    ) -> Answer<bool> {
        if constraints.is_trivially_never_satisfied() || constraints.is_trivially_always_satisfied()
        {
            return Ok(constraints.is_trivially_always_satisfied());
        }
        self.demand(checker, |reply| SignatureDependency::IsAlways {
            constraints,
            reply,
        })
        .await
    }

    async fn constraint_bound(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        kind: ConstraintBound,
        typevar: BoundTypeVarInstance<'db>,
        bound: Type<'db>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        self.demand(checker, |reply| SignatureDependency::ConstraintBound {
            kind,
            typevar,
            bound,
            reply,
        })
        .await
    }

    async fn receiver_constraints(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        if signature.receiver_constraints().is_none() {
            return Ok(checker.always());
        }
        self.demand(checker, |reply| SignatureDependency::ReceiverConstraints {
            signature: signature.clone(),
            reply,
        })
        .await
    }

    async fn reduce_inferable(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        constraints: ConstraintSet<'db, 'c>,
        inferable: TypeVarSet<'db>,
    ) -> Answer<ConstraintSet<'db, 'c>> {
        if inferable == TypeVarSet::None {
            return Ok(constraints);
        }
        self.demand(checker, |reply| SignatureDependency::ReduceInferable {
            constraints,
            inferable,
            reply,
        })
        .await
    }

    async fn max_freshness(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
        context: GenericContext<'db>,
    ) -> Answer<Option<TypeVarNonce>> {
        self.demand(checker, |reply| SignatureDependency::MaxFreshness {
            signature: signature.clone(),
            context,
            reply,
        })
        .await
    }

    async fn signature_typevars(
        &self,
        _db: &'db dyn Db,
        checker: &TypeRelationChecker<'state, 'c, 'db>,
        signature: &Signature<'db>,
    ) -> Answer<TypeVarSet<'db>> {
        if signature.generic_context.is_none() {
            return Ok(TypeVarSet::None);
        }
        self.demand(checker, |reply| SignatureDependency::SignatureTypevars {
            signature: signature.clone(),
            reply,
        })
        .await
    }

    fn resolve_alias(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Answer<Type<'db>>> {
        ready(
            if matches!(
                ty,
                Type::TypeAlias(_) | Type::Recursive(_) | Type::RecursiveVar(_)
            ) {
                Err(SignatureQueueError::Unsupported(
                    UnsupportedSignatureDependency::AliasResolution,
                ))
            } else {
                Ok(ty)
            },
        )
    }

    fn expand_parameters(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        parameters: &Parameters<'db>,
    ) -> impl Future<Output = Answer<Parameters<'db>>> {
        ready(
            if parameters
                .iter()
                .all(|parameter| !parameter.has_starred_annotation())
            {
                Ok(parameters.clone())
            } else {
                Err(SignatureQueueError::Unsupported(
                    UnsupportedSignatureDependency::ParameterExpansion,
                ))
            },
        )
    }

    fn normalize_variadic_parameters(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        source: Parameters<'db>,
        target: Parameters<'db>,
    ) -> impl Future<Output = Answer<(Parameters<'db>, Parameters<'db>)>> {
        ready(
            if source.variadic().is_none() || target.variadic().is_none() {
                Ok((source, target))
            } else {
                Err(SignatureQueueError::Unsupported(
                    UnsupportedSignatureDependency::VariadicNormalization,
                ))
            },
        )
    }

    unsupported_effects! {
        fn disjoint(source: Type<'db>, target: Type<'db>) -> ConstraintSet<'db, 'c> => Disjointness;
        fn freshen_signature(signature: &Signature<'db>, delta: u32) -> Signature<'db> => Freshening;
        fn aggregate_candidate(ty: Type<'db>) -> bool => AggregateInspection;
        fn union_add(builder: UnionBuilder<'db>, ty: Type<'db>) -> UnionBuilder<'db> => UnionNormalization;
        fn union_build(builder: UnionBuilder<'db>) -> Type<'db> => UnionNormalization;
        fn parameter_contains_typevar(parameters: &Parameters<'db>, typevar: BoundTypeVarInstance<'db>) -> bool => ParameterInspection;
        fn empty_tuple() -> Type<'db> => TupleNormalization;
    }

    fn tuple_from_parameters<'p>(
        &self,
        _db: &'db dyn Db,
        _checker: &TypeRelationChecker<'state, 'c, 'db>,
        _parameters: impl Iterator<Item = &'p Parameter<'db>> + Clone,
    ) -> impl Future<Output = Answer<Type<'db>>>
    where
        'db: 'p,
    {
        ready(Err(SignatureQueueError::Unsupported(
            UnsupportedSignatureDependency::TupleNormalization,
        )))
    }
}
