//! Local expression, argument, and specialization continuations share the current inference transaction.

mod arguments;
mod assignment;
mod call;
pub(in crate::types::infer) mod callable_annotation;
mod preparation;
mod string_annotation;
pub(in crate::types::infer) mod tuple_annotation;
pub(in crate::types::infer::builder) mod tuple_expression;
#[cfg(feature = "experimental-analysis")]
pub(in crate::types::infer::builder) mod source;
#[cfg(test)]
mod tests;

#[cfg(all(test, feature = "experimental-analysis"))]
pub(super) use tests::annotated_values::{Event as AnnotatedValueEvent, value_boundary as observe_annotated_value};

#[cfg(all(test, feature = "experimental-analysis"))]
pub(super) use tests::deferred_parameters::{
    TransactionKind as DeferredParameterTransactionKind,
    child_completed as deferred_parameter_child_completed,
    child_entered as deferred_parameter_child_entered,
    observe_child_polling as observe_deferred_parameter_child_polling,
};

use std::convert::Infallible;
use std::marker::PhantomData;
use std::rc::Rc;

#[cfg(all(test, feature = "experimental-analysis"))]
use salsa::plumbing::AsId;
use ty_mapping_probe_macros::shared_semantic_family;

use super::annotation_expression::{
    AnnotationExpressionInference, AnnotationPending, AnnotationStep, AnnotationStorage,
    PEP613Policy, QualifierPending,
};
use super::subscript::specialization;
use super::type_expression::{
    TypeExpressionMode, TypeExpressionPending, TypeExpressionRequest, TypeExpressionStep,
};
use super::typevar::legacy;
use super::*;
use crate::types::StaticClassLiteral;
use crate::types::call::Argument;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::cyclic::CallableRecursionGuard;

/// A root or live speculative builder in one local invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BuilderId(usize);

impl BuilderId {
    pub(super) const ROOT: Self = Self(0);
}

// This checkpoint is copied before any fallible operation. It owns no cache or frame storage.
#[derive(Clone, Copy)]
struct RootCheckpoint<'db> {
    binding_context: Option<Definition<'db>>,
    flags: InferenceFlags,
    deferred_state: DeferredExpressionState,
    had_cache: bool,
}

pub(super) struct BuilderStore<'root, 'db, 'ast> {
    root: &'root mut TypeInferenceBuilder<'db, 'ast>,
    speculative: Vec<TypeInferenceBuilder<'db, 'ast>>,
    checkpoint: RootCheckpoint<'db>,
    saved_field_specifiers: Option<SmallVec<[Type<'db>; NUM_FIELD_SPECIFIERS_INLINE]>>,
    completed: bool,
    #[cfg(all(test, feature = "experimental-analysis"))]
    function_annotations: bool,
    #[cfg(all(test, feature = "experimental-analysis"))]
    deferred_parameter: Option<tests::deferred_parameters::TransactionId>,
    #[cfg(test)]
    observer: Option<OwnershipObserver>,
}

impl<'root, 'db, 'ast> BuilderStore<'root, 'db, 'ast> {
    pub(super) fn new(root: &'root mut TypeInferenceBuilder<'db, 'ast>) -> Self {
        let checkpoint = RootCheckpoint {
            binding_context: root.typevar_binding_context,
            flags: root.context.inference_flags,
            deferred_state: root.deferred_state,
            had_cache: root.expression_cache.is_some(),
        };
        Self {
            root,
            speculative: Vec::new(),
            checkpoint,
            saved_field_specifiers: None,
            completed: false,
            #[cfg(all(test, feature = "experimental-analysis"))]
            function_annotations: false,
            #[cfg(all(test, feature = "experimental-analysis"))]
            deferred_parameter: None,
            #[cfg(test)]
            observer: None,
        }
    }

    pub(super) fn builder(&self, id: BuilderId) -> &TypeInferenceBuilder<'db, 'ast> {
        if id == BuilderId::ROOT {
            self.root
        } else {
            &self.speculative[id.0 - 1]
        }
    }

    pub(super) fn get_mut(&mut self, id: BuilderId) -> &mut TypeInferenceBuilder<'db, 'ast> {
        if id == BuilderId::ROOT {
            self.root
        } else {
            &mut self.speculative[id.0 - 1]
        }
    }

    pub(super) fn speculate(&mut self, parent: BuilderId, suppress_diagnostics: bool) -> BuilderId {
        let builder = if suppress_diagnostics {
            self.builder(parent).speculate_without_diagnostics()
        } else {
            self.builder(parent).speculate()
        };
        self.speculative.push(builder);
        BuilderId(self.speculative.len())
    }

    pub(super) fn setup_expression_cache(&mut self, id: BuilderId) -> bool {
        self.get_mut(id).setup_expression_cache()
    }

    pub(super) fn complete(&mut self) {
        debug_assert!(self.speculative.is_empty());
        self.completed = true;
    }

    /// Saves an empty incoming field buffer, restoring it on abort and retiring it on commit.
    /// The store must not already have a saved buffer. The caller checks that the incoming
    /// buffer is empty and admits fixed transfers and retirement of any spilled allocation.
    #[cfg(feature = "experimental-analysis")]
    pub(super) fn save_annotated_value_fields(&mut self) {
        debug_assert!(self.saved_field_specifiers.is_none());
        debug_assert!(self.root.dataclass_field_specifiers.is_empty());
        self.saved_field_specifiers = Some(std::mem::take(&mut self.root.dataclass_field_specifiers));
    }

    #[cfg(all(test, feature = "experimental-analysis"))]
    pub(super) fn observe_function_annotations(&mut self) {
        self.function_annotations = true;
    }

    /// Identifies this checkpoint independently of other checkpoints borrowing the same builder.
    #[cfg(all(test, feature = "experimental-analysis"))]
    pub(super) fn observe_deferred_parameter(&mut self, kind: DeferredParameterTransactionKind) {
        self.deferred_parameter = tests::deferred_parameters::transaction_started(self.root, kind);
    }

    /// Records scalar state immediately before successful checkpoint completion.
    #[cfg(all(test, feature = "experimental-analysis"))]
    pub(super) fn observe_deferred_parameter_completion(&mut self) {
        if let Some(transaction) = self.deferred_parameter.take() {
            tests::deferred_parameters::transaction_completed(transaction, self.root);
        }
    }

    pub(super) fn take_speculative(&mut self, id: BuilderId) -> TypeInferenceBuilder<'db, 'ast> {
        // Local continuations retire their inner speculation before returning to its parent.
        assert_ne!(id, BuilderId::ROOT);
        assert_eq!(id.0, self.speculative.len());
        self.speculative.remove(id.0 - 1)
    }
}

impl Drop for BuilderStore<'_, '_, '_> {
    fn drop(&mut self) {
        while let Some(builder) = self.speculative.pop() {
            #[cfg(test)]
            let id = BuilderId(self.speculative.len() + 1);
            drop(builder);
            #[cfg(test)]
            if let Some(observer) = &self.observer {
                observer(OwnershipEvent::SpeculativeRetired(id));
            }
        }
        if !self.completed {
            #[cfg(all(test, feature = "experimental-analysis"))]
            let annotated_value = self.saved_field_specifiers.is_some();
            self.root.typevar_binding_context = self.checkpoint.binding_context;
            self.root.context.inference_flags = self.checkpoint.flags;
            self.root.deferred_state = self.checkpoint.deferred_state;
            #[cfg(all(test, feature = "experimental-analysis"))]
            crate::types::infer::source_runtime::tests::quoted_annotations::observe_restored(
                self.root.inference_flags(), self.root.deferred_state,
            );
            if let Some(specifiers) = self.saved_field_specifiers.take() {
                self.root.dataclass_field_specifiers = specifiers;
            }
            if !self.checkpoint.had_cache {
                self.root.teardown_expression_cache();
            }
            #[cfg(all(test, feature = "experimental-analysis"))]
            if annotated_value {
                tests::annotated_values::restored(self.root);
            }
            #[cfg(all(test, feature = "experimental-analysis"))]
            tests::annotation_qualifiers::restored(self.root as *const _ as usize, function_annotation_state(self.root));
            #[cfg(all(test, feature = "experimental-analysis"))]
            if self.function_annotations {
                crate::types::infer::source_runtime::tests::signature_annotations::transaction_restored(
                    self.root.db(), function_annotation_state(self.root),
                );
            }
            #[cfg(all(test, feature = "experimental-analysis"))]
            if let Some(transaction) = self.deferred_parameter.take() {
                tests::deferred_parameters::transaction_restored(transaction, self.root);
            }
            #[cfg(test)]
            if let Some(observer) = &self.observer {
                observer(OwnershipEvent::RootRestored);
            }
        }
    }
}

#[cfg(all(test, feature = "experimental-analysis"))]
pub(super) fn function_annotation_state(
    builder: &TypeInferenceBuilder<'_, '_>,
) -> crate::types::infer::source_runtime::tests::signature_annotations::State {
    crate::types::infer::source_runtime::tests::signature_annotations::State {
        binding: builder
            .typevar_binding_context
            .map(|definition| definition.as_id()),
        flags: builder.context.inference_flags,
        deferred: builder.deferred_state,
        had_cache: builder.expression_cache.is_some(),
    }
}

#[derive(Clone, Copy)]
pub(super) enum ExpressionMode {
    Cached,
    Uncached,
    Value,
    MaybeStandalone,
    GetOrInfer,
}

type OwnedState<'db, 'expr, B = ConstraintSetBuilder<'db>> =
    arguments::State<'db, 'expr, 'expr, arguments::OwnedArguments<'expr, 'db>, B>;
type OwnedPending<'db, 'expr, B = ConstraintSetBuilder<'db>> =
    arguments::Pending<'db, 'expr, 'expr, arguments::OwnedArguments<'expr, 'db>, B>;
type OwnedAction<'db, 'expr, B = ConstraintSetBuilder<'db>> =
    arguments::Action<'db, 'expr, 'expr, arguments::OwnedArguments<'expr, 'db>, B>;

/// A phase token belongs to one invocation and is consumed by its next transition.
struct ActiveArgument(usize);
struct PendingArgument(usize);
struct CompletedArgument(usize);

struct CompletedArguments<'db, 'expr> {
    storage: arguments::OwnedArguments<'expr, 'db>,
    result: Result<(), CallErrorKind>,
}

enum ArgumentPhase<'db, 'expr, B = ConstraintSetBuilder<'db>> {
    Active(OwnedState<'db, 'expr, B>),
    Pending(OwnedPending<'db, 'expr, B>),
    Completed(CompletedArguments<'db, 'expr>),
}

/// Retains one call's argument state and any recursion guard across inference phases.
/// The test-only `lifetime` observation follows the actual payload through every phase. Its field is last
/// so an interrupted operation retires the arguments and bindings before recording their drop.
struct ArgumentPayload<'db, 'expr, P> {
    builder: BuilderId,
    data: call::CallData<'db, 'expr>,
    phase: P,
    recursion_guard: Option<CallableRecursionGuard<'db>>,
    #[cfg(test)]
    lifetime: Option<OwnerLifetime>,
}

impl<'db, 'expr, P> ArgumentPayload<'db, 'expr, P> {
    fn map<Q>(self, transition: impl FnOnce(P) -> Q) -> ArgumentPayload<'db, 'expr, Q> {
        let Self {
            builder,
            data,
            recursion_guard,
            phase,
            #[cfg(test)]
            lifetime,
        } = self;
        ArgumentPayload {
            builder,
            data,
            phase: transition(phase),
            recursion_guard,
            #[cfg(test)]
            lifetime,
        }
    }
}

struct TakenArgument<'db, 'expr, P> {
    index: usize,
    payload: ArgumentPayload<'db, 'expr, P>,
}

/// Selects whether class specialization returns a class object or a subclass type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types::infer) enum ClassSpecializationKind {
    ClassObject,
    Subclass,
}

pub(in crate::types::infer) enum SpecializationTarget<'db, T> {
    ClassObject(StaticClassLiteral<'db>),
    ClassSubclass(StaticClassLiteral<'db>),
    Custom(T),
}

type SpecializationState<'db, 'expr, B, T> =
    specialization::State<'db, 'expr, B, SpecializationTarget<'db, T>>;
type SpecializationPending<'db, 'expr, B, T> =
    specialization::Pending<'db, 'expr, B, SpecializationTarget<'db, T>>;
type SpecializationAction<'db, 'expr, B, T> =
    specialization::Action<'db, 'expr, B, SpecializationTarget<'db, T>>;
type SpecializationCompleted<'db, 'expr, B, T> =
    specialization::Completed<'db, 'expr, B, SpecializationTarget<'db, T>>;

struct ActiveSpecialization(usize);
struct PendingSpecialization(usize);
struct CompletedSpecialization(usize);

enum SpecializationPhase<'db, 'expr, B, T> {
    Active(SpecializationState<'db, 'expr, B, T>),
    Pending(SpecializationPending<'db, 'expr, B, T>),
    Completed(SpecializationCompleted<'db, 'expr, B, T>),
}

struct SpecializationPayload<P> {
    builder: BuilderId,
    phase: P,
    #[cfg(test)]
    lifetime: Option<OwnerLifetime>,
}

impl<P> SpecializationPayload<P> {
    fn map<Q>(self, transition: impl FnOnce(P) -> Q) -> SpecializationPayload<Q> {
        SpecializationPayload {
            builder: self.builder,
            phase: transition(self.phase),
            #[cfg(test)]
            lifetime: self.lifetime,
        }
    }
}

struct TakenSpecialization<P> {
    index: usize,
    payload: SpecializationPayload<P>,
}

impl<P> TakenSpecialization<P> {
    fn map<Q>(self, transition: impl FnOnce(P) -> Q) -> TakenSpecialization<Q> {
        TakenSpecialization {
            index: self.index,
            payload: self.payload.map(transition),
        }
    }
}

enum SpecializationStep<'expr> {
    Continue(ActiveSpecialization),
    Infer {
        pending: PendingSpecialization,
        builder: BuilderId,
        request: specialization::ChildRequest<'expr>,
    },
    Complete(CompletedSpecialization),
}

impl<'db, 'expr, P> TakenArgument<'db, 'expr, P> {
    fn map<Q>(self, transition: impl FnOnce(P) -> Q) -> TakenArgument<'db, 'expr, Q> {
        TakenArgument {
            index: self.index,
            payload: self.payload.map(transition),
        }
    }
}

enum OwnerSlot<'db, 'expr, B = ConstraintSetBuilder<'db>, T = Infallible> {
    Argument(ArgumentPayload<'db, 'expr, ArgumentPhase<'db, 'expr, B>>),
    Specialization(SpecializationPayload<SpecializationPhase<'db, 'expr, B, T>>),
    CallableAnnotation(callable_annotation::Payload<callable_annotation::Phase<'db, 'expr>>),
    TupleAnnotation(tuple_annotation::Payload<tuple_annotation::Phase<'db, 'expr>>),
    TupleExpression(tuple_expression::State<'db, 'expr>),
    Taken,
}

/// Keeps completion's arguments in their admitted slot while effects borrow them. Dropping the
/// lease retires the payload before the invocation, including when a finishing effect is cancelled.
struct CompletedArgumentLease<'owner, 'db, 'expr, B = ConstraintSetBuilder<'db>, T = Infallible> {
    index: usize,
    slot: &'owner mut OwnerSlot<'db, 'expr, B, T>,
}

impl<'db, 'expr, B, T> CompletedArgumentLease<'_, 'db, 'expr, B, T> {
    fn parts_mut(
        &mut self,
    ) -> (
        BuilderId,
        &call::CallData<'db, 'expr>,
        arguments::BorrowedArguments<'_, 'expr, 'db>,
        Result<(), CallErrorKind>,
    ) {
        let OwnerSlot::Argument(ArgumentPayload {
            builder,
            data,
            phase: ArgumentPhase::Completed(completed),
            ..
        }) = self.slot
        else {
            panic!("completed argument token does not identify a completed owner");
        };
        (
            *builder,
            data,
            arguments::BorrowedArguments {
                arguments: &mut completed.storage.arguments,
                bindings: &mut completed.storage.bindings,
            },
            completed.result,
        )
    }
}

impl<B, T> Drop for CompletedArgumentLease<'_, '_, '_, B, T> {
    fn drop(&mut self) {
        *self.slot = OwnerSlot::Taken;
    }
}

struct LocalOwners<'db, 'expr, B = ConstraintSetBuilder<'db>, T = Infallible> {
    slots: Vec<OwnerSlot<'db, 'expr, B, T>>,
    #[cfg(test)]
    observer: Option<OwnershipObserver>,
    #[cfg(test)]
    next_identity: usize,
}

impl<'db, 'expr, B, T> Default for LocalOwners<'db, 'expr, B, T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            #[cfg(test)]
            observer: None,
            #[cfg(test)]
            next_identity: 0,
        }
    }
}

impl<'db, 'expr, B, T> LocalOwners<'db, 'expr, B, T> {
    fn push(
        &mut self,
        builder: BuilderId,
        data: call::CallData<'db, 'expr>,
        state: OwnedState<'db, 'expr, B>,
        recursion_guard: Option<CallableRecursionGuard<'db>>,
    ) -> ActiveArgument {
        let index = self.slots.len();
        #[cfg(test)]
        let call = data.call.range();
        #[cfg(test)]
        let identity = self.next_identity;
        #[cfg(test)]
        {
            self.next_identity += 1;
        }
        self.slots.push(OwnerSlot::Argument(ArgumentPayload {
            builder,
            data,
            phase: ArgumentPhase::Active(state),
            recursion_guard,
            #[cfg(test)]
            lifetime: self.observer.as_ref().map(|observer| OwnerLifetime {
                identity,
                observer: observer.clone(),
            }),
        }));
        #[cfg(test)]
        if let Some(observer) = &self.observer {
            observer(OwnershipEvent::Created {
                kind: OwnerKind::Argument,
                call,
                index,
                identity,
                len: self.slots.len(),
                capacity: self.slots.capacity(),
            });
        }
        ActiveArgument(index)
    }

    fn take(&mut self, index: usize) -> ArgumentPayload<'db, 'expr, ArgumentPhase<'db, 'expr, B>> {
        assert_eq!(index + 1, self.slots.len());
        let slot = &mut self.slots[index];
        let OwnerSlot::Argument(payload) = std::mem::replace(slot, OwnerSlot::Taken) else {
            panic!("argument operation attempted to take an already-taken owner");
        };
        #[cfg(test)]
        if let Some(lifetime) = &payload.lifetime {
            (lifetime.observer)(OwnershipEvent::Taken {
                index,
                identity: lifetime.identity,
            });
        }
        payload
    }

    fn take_active(
        &mut self,
        owner: ActiveArgument,
    ) -> TakenArgument<'db, 'expr, OwnedState<'db, 'expr, B>> {
        let payload = self.take(owner.0).map(|phase| {
            let ArgumentPhase::Active(state) = phase else {
                panic!("active argument token does not identify an active owner");
            };
            state
        });
        TakenArgument {
            index: owner.0,
            payload,
        }
    }

    fn take_pending(
        &mut self,
        owner: PendingArgument,
    ) -> TakenArgument<'db, 'expr, OwnedPending<'db, 'expr, B>> {
        let payload = self.take(owner.0).map(|phase| {
            let ArgumentPhase::Pending(pending) = phase else {
                panic!("pending argument token does not identify a pending owner");
            };
            pending
        });
        TakenArgument {
            index: owner.0,
            payload,
        }
    }

    fn take_completed(
        &mut self,
        owner: CompletedArgument,
    ) -> CompletedArgumentLease<'_, 'db, 'expr, B, T> {
        assert_eq!(owner.0 + 1, self.slots.len());
        let slot = &mut self.slots[owner.0];
        #[cfg(test)]
        if let OwnerSlot::Argument(ArgumentPayload {
            lifetime: Some(lifetime),
            ..
        }) = &slot
        {
            (lifetime.observer)(OwnershipEvent::Taken {
                index: owner.0,
                identity: lifetime.identity,
            });
        }
        CompletedArgumentLease {
            index: owner.0,
            slot,
        }
    }

    fn install(&mut self, taken: TakenArgument<'db, 'expr, ArgumentPhase<'db, 'expr, B>>) {
        assert_eq!(taken.index + 1, self.slots.len());
        assert!(matches!(self.slots[taken.index], OwnerSlot::Taken));
        #[cfg(test)]
        let observation = taken.payload.lifetime.as_ref().map(|lifetime| {
            (
                lifetime.identity,
                match &taken.payload.phase {
                    ArgumentPhase::Active(_) => ObservedOwnerPhase::Active,
                    ArgumentPhase::Pending(_) => ObservedOwnerPhase::Pending,
                    ArgumentPhase::Completed(_) => ObservedOwnerPhase::Completed,
                },
            )
        });
        self.slots[taken.index] = OwnerSlot::Argument(taken.payload);
        #[cfg(test)]
        if let Some(observer) = &self.observer
            && let Some((identity, phase)) = observation
        {
            observer(OwnershipEvent::Installed {
                index: taken.index,
                identity,
                phase,
            });
        }
    }

    fn install_active(
        &mut self,
        taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr, B>>,
    ) -> ActiveArgument {
        let owner = ActiveArgument(taken.index);
        self.install(taken.map(ArgumentPhase::Active));
        owner
    }

    fn install_action(
        &mut self,
        taken: TakenArgument<'db, 'expr, OwnedAction<'db, 'expr, B>>,
    ) -> ArgumentStep<'db, 'expr> {
        let TakenArgument { index, payload } = taken;
        let ArgumentPayload {
            builder: call_builder,
            data,
            recursion_guard,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let (phase, step) = match phase {
            arguments::Action::Continue(state) => (
                ArgumentPhase::Active(state),
                ArgumentStep::Continue(ActiveArgument(index)),
            ),
            arguments::Action::Infer {
                pending,
                builder,
                argument: (_, expression, tcx),
                policy,
            } => (
                ArgumentPhase::Pending(pending),
                ArgumentStep::Infer {
                    pending: PendingArgument(index),
                    builder,
                    expression,
                    tcx,
                    policy,
                },
            ),
            arguments::Action::Complete { storage, result } => (
                ArgumentPhase::Completed(CompletedArguments { storage, result }),
                ArgumentStep::Complete(CompletedArgument(index)),
            ),
        };
        self.install(TakenArgument {
            index,
            payload: ArgumentPayload {
                builder: call_builder,
                data,
                phase,
                recursion_guard,
                #[cfg(test)]
                lifetime,
            },
        });
        step
    }

    fn retire(&mut self, finished: FinishedOwner<'db>) -> Type<'db> {
        assert_eq!(finished.index + 1, self.slots.len());
        assert!(matches!(self.slots[finished.index], OwnerSlot::Taken));
        self.slots.pop();
        #[cfg(test)]
        if let Some(observer) = &self.observer {
            observer(OwnershipEvent::SlotRetired {
                index: finished.index,
                len: self.slots.len(),
                capacity: self.slots.capacity(),
            });
        }
        finished.ty
    }

    fn push_specialization(
        &mut self,
        builder: BuilderId,
        request: specialization::Request<'db, 'expr, SpecializationTarget<'db, T>>,
    ) -> ActiveSpecialization {
        let index = self.slots.len();
        #[cfg(test)]
        let call = request.subscript.range();
        #[cfg(test)]
        let identity = self.next_identity;
        #[cfg(test)]
        {
            self.next_identity += 1;
        }
        self.slots
            .push(OwnerSlot::Specialization(SpecializationPayload {
                builder,
                phase: SpecializationPhase::Active(specialization::State::new(request)),
                #[cfg(test)]
                lifetime: self.observer.as_ref().map(|observer| OwnerLifetime {
                    identity,
                    observer: observer.clone(),
                }),
            }));
        #[cfg(test)]
        if let Some(observer) = &self.observer {
            observer(OwnershipEvent::Created {
                kind: OwnerKind::Specialization,
                call,
                index,
                identity,
                len: self.slots.len(),
                capacity: self.slots.capacity(),
            });
        }
        ActiveSpecialization(index)
    }

    fn take_specialization(
        &mut self,
        index: usize,
    ) -> SpecializationPayload<SpecializationPhase<'db, 'expr, B, T>> {
        assert_eq!(index + 1, self.slots.len());
        let OwnerSlot::Specialization(payload) =
            std::mem::replace(&mut self.slots[index], OwnerSlot::Taken)
        else {
            panic!("specialization token does not identify an initialized owner");
        };
        #[cfg(test)]
        if let Some(lifetime) = &payload.lifetime {
            (lifetime.observer)(OwnershipEvent::Taken {
                index,
                identity: lifetime.identity,
            });
        }
        payload
    }

    fn take_specialization_active(
        &mut self,
        owner: ActiveSpecialization,
    ) -> TakenSpecialization<SpecializationState<'db, 'expr, B, T>> {
        TakenSpecialization {
            index: owner.0,
            payload: self.take_specialization(owner.0).map(|phase| {
                let SpecializationPhase::Active(state) = phase else {
                    panic!("active specialization token does not identify an active owner");
                };
                state
            }),
        }
    }

    fn take_specialization_pending(
        &mut self,
        owner: PendingSpecialization,
    ) -> TakenSpecialization<SpecializationPending<'db, 'expr, B, T>> {
        TakenSpecialization {
            index: owner.0,
            payload: self.take_specialization(owner.0).map(|phase| {
                let SpecializationPhase::Pending(pending) = phase else {
                    panic!("pending specialization token does not identify a pending owner");
                };
                pending
            }),
        }
    }

    fn take_specialization_completed(
        &mut self,
        owner: CompletedSpecialization,
    ) -> TakenSpecialization<SpecializationCompleted<'db, 'expr, B, T>> {
        TakenSpecialization {
            index: owner.0,
            payload: self.take_specialization(owner.0).map(|phase| {
                let SpecializationPhase::Completed(completed) = phase else {
                    panic!("completed specialization token does not identify a completed owner");
                };
                completed
            }),
        }
    }

    fn install_specialization(
        &mut self,
        taken: TakenSpecialization<SpecializationPhase<'db, 'expr, B, T>>,
    ) {
        assert_eq!(taken.index + 1, self.slots.len());
        assert!(matches!(self.slots[taken.index], OwnerSlot::Taken));
        #[cfg(test)]
        let observation = taken.payload.lifetime.as_ref().map(|lifetime| {
            (
                lifetime.identity,
                match &taken.payload.phase {
                    SpecializationPhase::Active(_) => ObservedOwnerPhase::Active,
                    SpecializationPhase::Pending(_) => ObservedOwnerPhase::Pending,
                    SpecializationPhase::Completed(_) => ObservedOwnerPhase::Completed,
                },
            )
        });
        self.slots[taken.index] = OwnerSlot::Specialization(taken.payload);
        #[cfg(test)]
        if let Some(observer) = &self.observer
            && let Some((identity, phase)) = observation
        {
            observer(OwnershipEvent::Installed {
                index: taken.index,
                identity,
                phase,
            });
        }
    }

    fn install_specialization_active(
        &mut self,
        taken: TakenSpecialization<SpecializationState<'db, 'expr, B, T>>,
    ) -> ActiveSpecialization {
        let owner = ActiveSpecialization(taken.index);
        self.install_specialization(taken.map(SpecializationPhase::Active));
        owner
    }

    fn install_specialization_action(
        &mut self,
        taken: TakenSpecialization<SpecializationAction<'db, 'expr, B, T>>,
    ) -> SpecializationStep<'expr> {
        let TakenSpecialization { index, payload } = taken;
        let SpecializationPayload {
            builder,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let (phase, step) = match phase {
            specialization::Action::Continue(state) => (
                SpecializationPhase::Active(state),
                SpecializationStep::Continue(ActiveSpecialization(index)),
            ),
            specialization::Action::Infer { pending, request } => (
                SpecializationPhase::Pending(pending),
                SpecializationStep::Infer {
                    pending: PendingSpecialization(index),
                    builder,
                    request,
                },
            ),
            specialization::Action::Complete(completed) => (
                SpecializationPhase::Completed(completed),
                SpecializationStep::Complete(CompletedSpecialization(index)),
            ),
        };
        self.install_specialization(TakenSpecialization {
            index,
            payload: SpecializationPayload {
                builder,
                phase,
                #[cfg(test)]
                lifetime,
            },
        });
        step
    }

    #[cfg(test)]
    fn active(&self, owner: &ActiveArgument) -> Option<&OwnedState<'db, 'expr, B>> {
        match self.slots.get(owner.0) {
            Some(OwnerSlot::Argument(ArgumentPayload {
                phase: ArgumentPhase::Active(state),
                ..
            })) => Some(state),
            _ => None,
        }
    }

    #[cfg(test)]
    fn pending(&self, owner: &PendingArgument) -> Option<&OwnedPending<'db, 'expr, B>> {
        match self.slots.get(owner.0) {
            Some(OwnerSlot::Argument(ArgumentPayload {
                phase: ArgumentPhase::Pending(pending),
                ..
            })) => Some(pending),
            _ => None,
        }
    }

    #[cfg(test)]
    fn completed(&self, owner: &CompletedArgument) -> Option<&CompletedArguments<'db, 'expr>> {
        match self.slots.get(owner.0) {
            Some(OwnerSlot::Argument(ArgumentPayload {
                phase: ArgumentPhase::Completed(completed),
                ..
            })) => Some(completed),
            _ => None,
        }
    }
}

struct FinishedOwner<'db> {
    index: usize,
    ty: Type<'db>,
}

enum PreparedCall<'db> {
    Complete(Type<'db>),
    Arguments(ActiveArgument),
}

enum ArgumentStep<'db, 'expr> {
    Continue(ActiveArgument),
    Infer {
        pending: PendingArgument,
        builder: BuilderId,
        expression: &'expr ast::Expr,
        tcx: TypeContext<'db>,
        policy: arguments::ArgumentPolicy,
    },
    Complete(CompletedArgument),
}

struct LocalInvocation<'root, 'db, 'ast, 'expr, B = ConstraintSetBuilder<'db>, T = Infallible> {
    builders: BuilderStore<'root, 'db, 'ast>,
    owners: LocalOwners<'db, 'expr, B, T>,
    frames: Vec<Frame<'db, 'expr>>,
    annotations: Vec<AnnotationContinuation<'expr>>,
}

impl<'root, 'db, 'ast, 'expr, B, T> LocalInvocation<'root, 'db, 'ast, 'expr, B, T> {
    fn with_target(root: &'root mut TypeInferenceBuilder<'db, 'ast>) -> Self {
        Self {
            builders: BuilderStore::new(root),
            owners: LocalOwners::default(),
            frames: Vec::new(),
            annotations: Vec::new(),
        }
    }

    fn complete(&mut self) {
        assert!(self.frames.is_empty());
        assert!(self.annotations.is_empty());
        assert!(self.owners.slots.is_empty());
        #[cfg(test)]
        if let Some(observer) = &self.owners.observer {
            observer(OwnershipEvent::InvocationCompleted {
                arguments: self.owners.slots.len(),
                frames: self.frames.len(),
                speculative: self.builders.speculative.len(),
                capacity: self.owners.slots.capacity(),
            });
        }
        self.builders.complete();
    }

    #[cfg(test)]
    fn observe_ownership(&mut self, observer: OwnershipObserver) {
        self.builders.observer = Some(observer.clone());
        self.owners.observer = Some(observer);
    }
}

impl<'root, 'db, 'ast, 'expr, B> LocalInvocation<'root, 'db, 'ast, 'expr, B> {
    fn new(root: &'root mut TypeInferenceBuilder<'db, 'ast>) -> Self {
        Self::with_target(root)
    }
}

impl<B, T> Drop for LocalInvocation<'_, '_, '_, '_, B, T> {
    fn drop(&mut self) {
        // Return tokens disappear before their payloads. Builders retain the contexts and caches
        // used by those payloads until every initialized owner has retired, innermost first.
        while self.frames.pop().is_some() {}
        while self.annotations.pop().is_some() {}
        #[cfg(all(test, feature = "experimental-analysis"))]
        tests::annotation_qualifiers::continuations_retired(self.builders.root as *const _ as usize, self.annotations.len());
        while let Some(owner) = self.owners.slots.pop() {
            drop(owner);
        }
        #[cfg(all(test, feature = "experimental-analysis"))]
        tests::annotated_values::rhs_retired(self.builders.root);
    }
}

#[cfg(test)]
type OwnershipObserver = Rc<dyn Fn(OwnershipEvent)>;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObservedOwnerPhase {
    Active,
    Pending,
    Completed,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnerKind {
    Argument,
    Specialization,
    CallableAnnotation,
    TupleAnnotation,
    TupleExpression,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnershipEvent {
    Created {
        kind: OwnerKind,
        call: ruff_text_size::TextRange,
        index: usize,
        identity: usize,
        len: usize,
        capacity: usize,
    },
    InvocationCompleted {
        arguments: usize,
        frames: usize,
        speculative: usize,
        capacity: usize,
    },
    Taken {
        index: usize,
        identity: usize,
    },
    Installed {
        index: usize,
        identity: usize,
        phase: ObservedOwnerPhase,
    },
    PayloadRetired {
        identity: usize,
    },
    SlotRetired {
        index: usize,
        len: usize,
        capacity: usize,
    },
    SpeculativeRetired(BuilderId),
    RootRestored,
}

#[cfg(test)]
struct OwnerLifetime {
    identity: usize,
    observer: OwnershipObserver,
}

#[cfg(test)]
impl Drop for OwnerLifetime {
    fn drop(&mut self) {
        (self.observer)(OwnershipEvent::PayloadRetired {
            identity: self.identity,
        });
    }
}

struct Preparation<'db, 'expr> {
    builder: BuilderId,
    source: &'expr ast::Arguments,
    cursor: ArgumentsIter<'expr>,
    arguments: CallArguments<'expr, 'db>,
}

struct Splat<'db, 'expr> {
    preparation: Preparation<'db, 'expr>,
    source: ast::ArgOrKeyword<'expr>,
    argument: Argument<'expr>,
}

enum PreparationStep<'db, 'expr> {
    Continue(Preparation<'db, 'expr>),
    Infer(Splat<'db, 'expr>, &'expr ast::Expr),
    Complete(CallArguments<'expr, 'db>),
}

/// Owner continuations retain phase tokens; the invocation retains their payloads.
enum Frame<'db, 'expr> {
    AnnotationResume(AnnotationRoot<'expr>, AnnotationPending<'expr>),
    TypeExpressionResume(BuilderId, TypeExpressionPending<'db, 'expr>),
    StringAnnotation(BuilderId, string_annotation::Scope<'expr>),
    TypeExpressionFinish(BuilderId, &'expr ast::Expr, TypeExpressionScope),
    Finish(BuilderId, &'expr ast::Expr, TypeContext<'db>),
    Cache(BuilderId, BuilderId, &'expr ast::Expr, TypeContext<'db>),
    Callee(BuilderId, CalleeState<'db>, CalleeContinuation<'db, 'expr>),
    AssignmentFinish(BuilderId, &'expr ast::Expr, &'expr ast::ExprCall, Type<'db>),
    LegacyTypeVar(BuilderId, legacy::Pending<'db, 'expr>),
    SubscriptReceiver(BuilderId, &'expr ast::ExprSubscript),
    SubscriptSlice(BuilderId, subscript::SubscriptPending<'db, 'expr>),
    Splat(Splat<'db, 'expr>, call::CallData<'db, 'expr>),
    Argument(PendingArgument),
    Specialization(PendingSpecialization),
    CallableAnnotation(callable_annotation::Waiting),
    TupleAnnotation(tuple_annotation::Waiting),
    TupleExpression(tuple_expression::Waiting),
    ParamSpec(BuilderId, bool),
}

struct CalleeState<'db> {
    binding: Option<Definition<'db>>,
    check_unbound: bool,
}

enum CalleeContinuation<'db, 'expr> {
    Return,
    Call(&'expr ast::ExprCall, TypeContext<'db>),
    Assignment {
        target: &'expr ast::Expr,
        call: &'expr ast::ExprCall,
        definition: Definition<'db>,
        context: TypeContext<'db>,
    },
}

enum Work<'db, 'expr> {
    StringAnnotation(BuilderId, &'expr ast::ExprStringLiteral),
    AnnotationStart(
        BuilderId,
        &'expr ast::Expr,
        DeferredExpressionState,
        PEP613Policy,
    ),
    AnnotationBody(AnnotationRoot<'expr>, PEP613Policy),
    AnnotationStep(AnnotationRoot<'expr>, AnnotationStep<'db, 'expr>),
    FinishAnnotation(AnnotationRoot<'expr>, AnnotationExpressionInference<'db>),
    TypeExpression(BuilderId, TypeExpressionRequest<'db, 'expr>),
    Expression(
        BuilderId,
        &'expr ast::Expr,
        TypeContext<'db>,
        ExpressionMode,
    ),
    Callee(BuilderId, &'expr ast::Expr, CalleeContinuation<'db, 'expr>),
    Call(BuilderId, &'expr ast::ExprCall, Type<'db>, TypeContext<'db>),
    LegacyTypeVar(BuilderId, legacy::State<'db, 'expr>),
    Prepare(Preparation<'db, 'expr>, call::CallData<'db, 'expr>),
    Arguments(ActiveArgument),
    StartSpecialization {
        builder: BuilderId,
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
        kind: ClassSpecializationKind,
    },
    Specialization(ActiveSpecialization),
    StartCallableAnnotation(BuilderId, callable_annotation::Request<'expr>),
    CallableAnnotation(callable_annotation::Active),
    StartTupleAnnotation(BuilderId, tuple_annotation::Request<'expr>),
    TupleAnnotation(tuple_annotation::Active),
    StartTupleExpression(BuilderId, &'expr ast::ExprTuple, TypeContext<'db>),
    TupleExpression(tuple_expression::Active),
    Return(Type<'db>),
}

#[derive(Debug, Eq, PartialEq)]
enum LocalResult<'db> {
    Type(Type<'db>),
    Annotation(TypeAndQualifiers<'db>),
}

/// A suspended annotation consumes the complete child result, independently of Type frames.
#[derive(Debug)]
struct AnnotationContinuation<'expr> {
    root: AnnotationRoot<'expr>,
    pending: QualifierPending<'expr>,
}

#[derive(Debug)]
struct AnnotationRoot<'expr> {
    builder: BuilderId,
    annotation: &'expr ast::Expr,
    saved: Option<AnnotationScope>,
}

#[derive(Debug, Clone, Copy)]
struct AnnotationScope {
    deferred_state: DeferredExpressionState,
    requested: DeferredExpressionState,
    check_unbound: bool,
}

impl AnnotationScope {
    fn prepare(
        builder: &TypeInferenceBuilder<'_, '_>,
        requested: DeferredExpressionState,
        in_stub: bool,
    ) -> Self {
        // `DeferredExpressionState::InStringAnnotation` takes precedence over other deferred states.
        // However, if it's not a stringified annotation, we must still ensure that annotation expressions
        // are always deferred in stub files.
        Self {
            deferred_state: builder.deferred_state,
            requested: if requested.in_string_annotation() || !in_stub {
                requested
            } else {
                DeferredExpressionState::Deferred
            },
            check_unbound: builder
                .inference_flags()
                .contains(InferenceFlags::CHECK_UNBOUND_TYPEVARS),
        }
    }

    fn enter(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        builder.replace_deferred_state(self.requested);
        builder
            .context
            .inference_flags
            .insert(InferenceFlags::CHECK_UNBOUND_TYPEVARS);
    }

    fn restore(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        builder
            .context
            .inference_flags
            .set(InferenceFlags::CHECK_UNBOUND_TYPEVARS, self.check_unbound);
        builder.deferred_state = self.deferred_state;
    }
}

#[derive(Clone, Copy)]
struct TypeExpressionScope {
    before_store_deferred: DeferredExpressionState,
    after_store_deferred: Option<DeferredExpressionState>,
    requested: Option<DeferredExpressionState>,
    in_stub: bool,
    in_type_expression: bool,
    in_nested_type_expression: bool,
}

impl TypeExpressionScope {
    fn prepare(
        builder: &TypeInferenceBuilder<'_, '_>,
        mode: TypeExpressionMode,
        in_stub: bool,
    ) -> Option<Self> {
        let requested = match mode {
            TypeExpressionMode::NoStore => return None,
            TypeExpressionMode::Scoped => None,
            TypeExpressionMode::ScopedWithState(state) => Some(state),
        };
        let original = builder.deferred_state;
        let before_store_deferred = if original.in_string_annotation() {
            original
        } else {
            requested.unwrap_or(original)
        };
        Some(Self {
            before_store_deferred,
            after_store_deferred: requested.map(|_| original),
            requested,
            in_stub,
            in_type_expression: builder
                .inference_flags()
                .contains(InferenceFlags::IN_TYPE_EXPRESSION),
            in_nested_type_expression: builder
                .inference_flags()
                .contains(InferenceFlags::IN_NESTED_TYPE_EXPRESSION),
        })
    }

    fn enter(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        if let Some(requested) = self.requested {
            builder.replace_deferred_state(requested);
        }
        builder.context.inference_flags.set(
            InferenceFlags::IN_NESTED_TYPE_EXPRESSION,
            self.in_type_expression || self.in_nested_type_expression,
        );
        builder
            .context
            .inference_flags
            .insert(InferenceFlags::IN_TYPE_EXPRESSION);
        // Annotation expressions are always deferred in stub files.
        if self.in_stub {
            builder.replace_deferred_state(DeferredExpressionState::Deferred);
        }
    }

    fn restore_before_store(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        builder.deferred_state = self.before_store_deferred;
        builder.context.inference_flags.set(
            InferenceFlags::IN_NESTED_TYPE_EXPRESSION,
            self.in_nested_type_expression,
        );
        builder
            .context
            .inference_flags
            .set(InferenceFlags::IN_TYPE_EXPRESSION, self.in_type_expression);
    }

    fn restore_after_store(self, builder: &mut TypeInferenceBuilder<'_, '_>) {
        if let Some(previous) = self.after_store_deferred {
            builder.deferred_state = previous;
        }
    }
}

struct LocalFacts;
type OrdinaryLocalEffects = OrdinaryLocalEffectsWithTarget<NoSpecializationTarget>;
#[cfg(test)]
type OrdinaryOwnedArgumentEffects = OrdinaryOwnedArgumentEffectsWithTarget<Infallible>;

#[derive(Default)]
struct NoSpecializationTarget;

trait OrdinaryCustomSpecialization<'db> {
    type Target;
    fn finish(&self, target: Self::Target, types: &[Option<Type<'db>>]) -> Type<'db>;
}

impl<'db> OrdinaryCustomSpecialization<'db> for NoSpecializationTarget {
    type Target = Infallible;

    fn finish(&self, target: Infallible, _types: &[Option<Type<'db>>]) -> Type<'db> {
        match target {}
    }
}

struct CallbackSpecialization<'target, 'db>(&'target dyn Fn(&[Option<Type<'db>>]) -> Type<'db>);

impl<'db> OrdinaryCustomSpecialization<'db> for CallbackSpecialization<'_, 'db> {
    type Target = ();

    fn finish(&self, (): (), types: &[Option<Type<'db>>]) -> Type<'db> {
        (self.0)(types)
    }
}

#[derive(Default)]
struct OrdinaryLocalEffectsWithTarget<F> {
    finalizer: F,
}

struct OrdinaryOwnedArgumentEffectsWithTarget<T> {
    target: PhantomData<fn() -> T>,
}

impl<T> Default for OrdinaryOwnedArgumentEffectsWithTarget<T> {
    fn default() -> Self {
        Self {
            target: PhantomData,
        }
    }
}

struct LocalSpecializationFinalizer<'effects, F>(&'effects F);

impl<'db, F: OrdinaryCustomSpecialization<'db>> specialization::OrdinarySpecializationFinalizer<'db>
    for LocalSpecializationFinalizer<'_, F>
{
    type Target = SpecializationTarget<'db, F::Target>;

    fn finish_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        target: Self::Target,
        generic_context: GenericContext<'db>,
        types: &[Option<Type<'db>>],
    ) -> Type<'db> {
        match target {
            SpecializationTarget::ClassObject(class) => {
                let db = builder.db();
                Type::from(class.apply_specialization(db, |_| {
                    generic_context.specialize_partial(db, types.iter().copied())
                }))
            }
            SpecializationTarget::ClassSubclass(class) => {
                specialization::finish_class_subclass(builder, class, generic_context, types)
            }
            SpecializationTarget::Custom(target) => self.0.finish(target, types),
        }
    }
}

impl<'db, 'ast, F: OrdinaryCustomSpecialization<'db>>
    SynchronousOwnedSpecializationEffects<'db, 'ast> for OrdinaryLocalEffectsWithTarget<F>
{
    type Error = Infallible;
    type Builder = ConstraintSetBuilder<'db>;
    type CustomSpecializationTarget = F::Target;

    fn take_active<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: ActiveSpecialization,
    ) -> Result<
        TakenSpecialization<SpecializationState<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>>,
        Infallible,
    > {
        Ok(owners.take_specialization_active(owner))
    }

    fn advance<'expr>(
        &self,
        taken: TakenSpecialization<
            SpecializationState<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        >,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<
        TakenSpecialization<SpecializationAction<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>>,
        Infallible,
    > {
        let builder = taken.payload.builder;
        let db = builders.builder(builder).db();
        Ok(taken.map(|state| {
            let Ok(action) = specialization::advance_sync(
                state,
                builders.get_mut(builder),
                specialization::ExplicitSpecializationFacts,
                &specialization::OrdinarySpecializationEffects {
                    db,
                    finalizer: LocalSpecializationFinalizer(&self.finalizer),
                },
            );
            action
        }))
    }

    fn install_action<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        taken: TakenSpecialization<
            SpecializationAction<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        >,
    ) -> Result<SpecializationStep<'expr>, Infallible> {
        Ok(owners.install_specialization_action(taken))
    }

    fn take_pending<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: PendingSpecialization,
    ) -> Result<
        TakenSpecialization<
            SpecializationPending<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        >,
        Infallible,
    > {
        Ok(owners.take_specialization_pending(owner))
    }

    fn resume<'expr>(
        &self,
        taken: TakenSpecialization<
            SpecializationPending<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        >,
        ty: Type<'db>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<
        TakenSpecialization<SpecializationState<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>>,
        Infallible,
    > {
        let builder = taken.payload.builder;
        let db = builders.builder(builder).db();
        Ok(taken.map(|pending| {
            let Ok(state) = specialization::resume_sync(
                pending,
                ty,
                builders.get_mut(builder),
                specialization::ExplicitSpecializationFacts,
                &specialization::OrdinarySpecializationEffects {
                    db,
                    finalizer: LocalSpecializationFinalizer(&self.finalizer),
                },
            );
            state
        }))
    }

    fn install_active<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        taken: TakenSpecialization<
            SpecializationState<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        >,
    ) -> Result<ActiveSpecialization, Infallible> {
        Ok(owners.install_specialization_active(taken))
    }

    fn take_completed<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: CompletedSpecialization,
    ) -> Result<
        TakenSpecialization<
            SpecializationCompleted<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        >,
        Infallible,
    > {
        Ok(owners.take_specialization_completed(owner))
    }

    fn finish<'expr>(
        &self,
        taken: TakenSpecialization<
            SpecializationCompleted<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        >,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<FinishedOwner<'db>, Infallible> {
        let TakenSpecialization { index, payload } = taken;
        let SpecializationPayload {
            builder,
            phase,
            #[cfg(test)]
            lifetime,
        } = payload;
        let db = builders.builder(builder).db();
        let result = specialization::finish_sync(
            phase,
            builders.get_mut(builder),
            &specialization::OrdinarySpecializationEffects {
                db,
                finalizer: LocalSpecializationFinalizer(&self.finalizer),
            },
        );
        #[cfg(test)]
        drop(lifetime);
        result.map(|ty| FinishedOwner { index, ty })
    }

    fn retire<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        finished: FinishedOwner<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(owners.retire(finished))
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousOwnedArgumentEffects)]
    trait OwnedArgumentEffects<'db, 'ast> {
        type Error;
        type Builder: std::borrow::Borrow<ConstraintSetBuilder<'db>>;
        type CustomSpecializationTarget;
        #[operation(source)]
        async fn prepared<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, data: call::CallData<'db, 'expr>, arguments: CallArguments<'expr, 'db>) -> Result<call::Prepared<'db, 'expr>, Self::Error>;
        #[operation(local)]
        async fn install_prepared<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, id: BuilderId, prepared: call::Prepared<'db, 'expr>) -> Result<PreparedCall<'db>, Self::Error>;
        #[operation(local)]
        async fn take_active<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: ActiveArgument) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr, Self::Builder>>, Self::Error>;
        #[operation(source)]
        async fn advance<'expr>(&self, taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr, Self::Builder>>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<TakenArgument<'db, 'expr, OwnedAction<'db, 'expr, Self::Builder>>, Self::Error>;
        #[operation(local)]
        async fn install_action<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, taken: TakenArgument<'db, 'expr, OwnedAction<'db, 'expr, Self::Builder>>) -> Result<ArgumentStep<'db, 'expr>, Self::Error>;
        #[operation(local)]
        async fn take_pending<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: PendingArgument) -> Result<TakenArgument<'db, 'expr, OwnedPending<'db, 'expr, Self::Builder>>, Self::Error>;
        #[operation(source)]
        async fn resume<'expr>(&self, taken: TakenArgument<'db, 'expr, OwnedPending<'db, 'expr, Self::Builder>>, ty: Type<'db>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr, Self::Builder>>, Self::Error>;
        #[operation(local)]
        async fn install_active<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr, Self::Builder>>) -> Result<ActiveArgument, Self::Error>;
        #[operation(local)]
        async fn take_completed<'owner, 'expr>(&self, owners: &'owner mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: CompletedArgument) -> Result<CompletedArgumentLease<'owner, 'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, Self::Error>;
        #[operation(source)]
        async fn finish<'expr>(&self, taken: CompletedArgumentLease<'_, 'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<FinishedOwner<'db>, Self::Error>;
        #[operation(local)]
        async fn retire<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, finished: FinishedOwner<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(owned_prepared_call_sync)]
    #[capabilities(effects = OwnedArgumentEffects)]
    #[passive_values()]
    async fn owned_prepared_call<'db, 'ast, 'expr, E: OwnedArgumentEffects<'db, 'ast>>(
        builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, E::Builder, E::CustomSpecializationTarget>, id: BuilderId, data: call::CallData<'db, 'expr>, arguments: CallArguments<'expr, 'db>, effects: &E,
    ) -> Result<PreparedCall<'db>, E::Error> {
        let prepared = effects.prepared(builders, id, data, arguments).await?;
        effects.install_prepared(owners, id, prepared).await
    }

    #[synchronous(owned_argument_step_sync)]
    #[capabilities(effects = OwnedArgumentEffects)]
    #[passive_values()]
    async fn owned_argument_step<'db, 'ast, 'expr, E: OwnedArgumentEffects<'db, 'ast>>(
        owner: ActiveArgument, owners: &mut LocalOwners<'db, 'expr, E::Builder, E::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>, effects: &E,
    ) -> Result<ArgumentStep<'db, 'expr>, E::Error> {
        let taken = effects.take_active(owners, owner).await?;
        let advanced = effects.advance(taken, builders).await?;
        effects.install_action(owners, advanced).await
    }

    #[synchronous(owned_resume_argument_sync)]
    #[capabilities(effects = OwnedArgumentEffects)]
    #[passive_values()]
    async fn owned_resume_argument<'db, 'ast, 'expr, E: OwnedArgumentEffects<'db, 'ast>>(
        owner: PendingArgument, ty: Type<'db>, owners: &mut LocalOwners<'db, 'expr, E::Builder, E::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>, effects: &E,
    ) -> Result<ActiveArgument, E::Error> {
        let taken = effects.take_pending(owners, owner).await?;
        let resumed = effects.resume(taken, ty, builders).await?;
        effects.install_active(owners, resumed).await
    }

    #[synchronous(owned_finish_call_sync)]
    #[capabilities(effects = OwnedArgumentEffects)]
    #[passive_values()]
    async fn owned_finish_call<'db, 'ast, 'expr, E: OwnedArgumentEffects<'db, 'ast>>(
        builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, E::Builder, E::CustomSpecializationTarget>, owner: CompletedArgument, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let taken = effects.take_completed(owners, owner).await?;
        let finished = effects.finish(taken, builders).await?;
        effects.retire(owners, finished).await
    }
}

fn install_prepared<'db, 'expr, B, T>(
    owners: &mut LocalOwners<'db, 'expr, B, T>,
    id: BuilderId,
    prepared: call::Prepared<'db, 'expr>,
) -> PreparedCall<'db> {
    match prepared {
        call::Prepared::Complete(ty) => PreparedCall::Complete(ty),
        call::Prepared::Arguments(data, storage, recursion_guard) => {
            let policy = if matches!(
                data.callable_type,
                Type::KnownBoundMethod(
                    KnownBoundMethodType::ConstraintSetLowerBound
                        | KnownBoundMethodType::ConstraintSetUpperBound
                        | KnownBoundMethodType::ConstraintSetEquality
                        | KnownBoundMethodType::ConstraintSetRange
                )
            ) {
                arguments::ArgumentPolicy::PermitParamSpec
            } else {
                arguments::ArgumentPolicy::Ordinary
            };
            let state = arguments::State::new(
                id,
                ArgumentsIter::from_ast(&data.call.arguments),
                storage,
                policy,
                data.tcx,
            );
            PreparedCall::Arguments(owners.push(id, data, state, recursion_guard))
        }
    }
}

impl<'db, 'ast, T> SynchronousOwnedArgumentEffects<'db, 'ast>
    for OrdinaryOwnedArgumentEffectsWithTarget<T>
{
    type Error = Infallible;
    type Builder = ConstraintSetBuilder<'db>;
    type CustomSpecializationTarget = T;

    fn prepared<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        data: call::CallData<'db, 'expr>,
        arguments: CallArguments<'expr, 'db>,
    ) -> Result<call::Prepared<'db, 'expr>, Self::Error> {
        call::prepared_sync(
            builders.get_mut(id),
            data,
            arguments,
            call::CallFacts,
            &call::OrdinaryCallEffects,
        )
    }

    fn install_prepared<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, T>,
        id: BuilderId,
        prepared: call::Prepared<'db, 'expr>,
    ) -> Result<PreparedCall<'db>, Self::Error> {
        Ok(install_prepared(owners, id, prepared))
    }

    fn take_active<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, T>,
        owner: ActiveArgument,
    ) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>, Self::Error> {
        Ok(owners.take_active(owner))
    }

    fn advance<'expr>(
        &self,
        taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<TakenArgument<'db, 'expr, OwnedAction<'db, 'expr>>, Self::Error> {
        let taken = taken.map(|state| arguments::advance(state, builders));
        if let arguments::Action::Infer {
            builder,
            argument: (_, expression, _),
            ..
        } = &taken.payload.phase
        {
            debug_assert!(
                !builders
                    .builder(*builder)
                    .index
                    .is_standalone_expression(*expression)
            );
        }
        Ok(taken)
    }

    fn install_action<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, T>,
        taken: TakenArgument<'db, 'expr, OwnedAction<'db, 'expr>>,
    ) -> Result<ArgumentStep<'db, 'expr>, Self::Error> {
        Ok(owners.install_action(taken))
    }

    fn take_pending<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, T>,
        owner: PendingArgument,
    ) -> Result<TakenArgument<'db, 'expr, OwnedPending<'db, 'expr>>, Self::Error> {
        Ok(owners.take_pending(owner))
    }

    fn resume<'expr>(
        &self,
        taken: TakenArgument<'db, 'expr, OwnedPending<'db, 'expr>>,
        ty: Type<'db>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>, Self::Error> {
        Ok(taken.map(|pending| arguments::resume(pending, ty, builders)))
    }

    fn install_active<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, T>,
        taken: TakenArgument<'db, 'expr, OwnedState<'db, 'expr>>,
    ) -> Result<ActiveArgument, Self::Error> {
        Ok(owners.install_active(taken))
    }

    fn take_completed<'owner, 'expr>(
        &self,
        owners: &'owner mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, T>,
        owner: CompletedArgument,
    ) -> Result<CompletedArgumentLease<'owner, 'db, 'expr, ConstraintSetBuilder<'db>, T>, Self::Error>
    {
        Ok(owners.take_completed(owner))
    }

    fn finish<'expr>(
        &self,
        mut taken: CompletedArgumentLease<'_, 'db, 'expr, ConstraintSetBuilder<'db>, T>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<FinishedOwner<'db>, Self::Error> {
        let index = taken.index;
        let (builder, data, storage, result) = taken.parts_mut();
        let result = call::finish_sync(
            builders.get_mut(builder),
            data,
            storage,
            result,
            &call::OrdinaryCallEffects,
        );
        drop(taken);
        result.map(|ty| FinishedOwner { index, ty })
    }

    fn retire<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, T>,
        finished: FinishedOwner<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(owners.retire(finished))
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousOwnedSpecializationEffects)]
    trait OwnedSpecializationEffects<'db, 'ast> {
        type Error;
        type Builder: std::borrow::Borrow<ConstraintSetBuilder<'db>>;
        type CustomSpecializationTarget;
        #[operation(local)]
        async fn take_active<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: ActiveSpecialization) -> Result<TakenSpecialization<SpecializationState<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, Self::Error>;
        #[operation(source)]
        async fn advance<'expr>(&self, taken: TakenSpecialization<SpecializationState<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<TakenSpecialization<SpecializationAction<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, Self::Error>;
        #[operation(local)]
        async fn install_action<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, taken: TakenSpecialization<SpecializationAction<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>) -> Result<SpecializationStep<'expr>, Self::Error>;
        #[operation(local)]
        async fn take_pending<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: PendingSpecialization) -> Result<TakenSpecialization<SpecializationPending<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, Self::Error>;
        #[operation(source)]
        async fn resume<'expr>(&self, taken: TakenSpecialization<SpecializationPending<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, ty: Type<'db>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<TakenSpecialization<SpecializationState<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, Self::Error>;
        #[operation(local)]
        async fn install_active<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, taken: TakenSpecialization<SpecializationState<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>) -> Result<ActiveSpecialization, Self::Error>;
        #[operation(local)]
        async fn take_completed<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: CompletedSpecialization) -> Result<TakenSpecialization<SpecializationCompleted<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, Self::Error>;
        #[operation(source)]
        async fn finish<'expr>(&self, taken: TakenSpecialization<SpecializationCompleted<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<FinishedOwner<'db>, Self::Error>;
        #[operation(local)]
        async fn retire<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, finished: FinishedOwner<'db>) -> Result<Type<'db>, Self::Error>;
    }

    #[synchronous(owned_specialization_step_sync)]
    #[capabilities(effects = OwnedSpecializationEffects)]
    #[passive_values()]
    async fn owned_specialization_step<'db, 'ast, 'expr, E: OwnedSpecializationEffects<'db, 'ast>>(
        owner: ActiveSpecialization, owners: &mut LocalOwners<'db, 'expr, E::Builder, E::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>, effects: &E,
    ) -> Result<SpecializationStep<'expr>, E::Error> {
        let taken = effects.take_active(owners, owner).await?;
        let advanced = effects.advance(taken, builders).await?;
        effects.install_action(owners, advanced).await
    }

    #[synchronous(owned_resume_specialization_sync)]
    #[capabilities(effects = OwnedSpecializationEffects)]
    #[passive_values()]
    async fn owned_resume_specialization<'db, 'ast, 'expr, E: OwnedSpecializationEffects<'db, 'ast>>(
        owner: PendingSpecialization, ty: Type<'db>, owners: &mut LocalOwners<'db, 'expr, E::Builder, E::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>, effects: &E,
    ) -> Result<ActiveSpecialization, E::Error> {
        let taken = effects.take_pending(owners, owner).await?;
        let resumed = effects.resume(taken, ty, builders).await?;
        effects.install_active(owners, resumed).await
    }

    #[synchronous(owned_finish_specialization_sync)]
    #[capabilities(effects = OwnedSpecializationEffects)]
    #[passive_values()]
    async fn owned_finish_specialization<'db, 'ast, 'expr, E: OwnedSpecializationEffects<'db, 'ast>>(
        builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, E::Builder, E::CustomSpecializationTarget>, owner: CompletedSpecialization, effects: &E,
    ) -> Result<Type<'db>, E::Error> {
        let taken = effects.take_completed(owners, owner).await?;
        let finished = effects.finish(taken, builders).await?;
        effects.retire(owners, finished).await
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousLocalEffects)]
    trait LocalEffects<'db, 'ast> {
        type Error;
        type Builder: std::borrow::Borrow<ConstraintSetBuilder<'db>>;
        type CustomSpecializationTarget;
        type StringAnnotations;
        #[operation(source)]
        async fn parse_string_annotation<'expr>(&self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId, string: &ast::ExprStringLiteral, storage: &'expr Self::StringAnnotations) -> Result<Option<&'expr ast::Expr>, Self::Error>;
        #[operation(local)]
        async fn prepare_string_annotation<'expr>(&self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId, string: &'expr ast::ExprStringLiteral, parsed: &'expr ast::Expr) -> Result<string_annotation::Scope<'expr>, Self::Error>;
        #[operation(local)]
        async fn enter_string_annotation(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, scope: string_annotation::Scope<'_>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish_string_annotation(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, scope: string_annotation::Scope<'_>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next<'expr>(&self, work: &mut Option<Work<'db, 'expr>>) -> Result<Option<Work<'db, 'expr>>, Self::Error>;
        #[operation(local)]
        async fn continue_with<'expr>(&self, work: &mut Option<Work<'db, 'expr>>, next: Work<'db, 'expr>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn push<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, Self::Builder, Self::CustomSpecializationTarget>, frame: Frame<'db, 'expr>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn pop<'expr>(&self, frames: &mut Vec<Frame<'db, 'expr>>) -> Result<Option<Frame<'db, 'expr>>, Self::Error>;
        #[operation(local)]
        async fn push_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, Self::Builder, Self::CustomSpecializationTarget>, continuation: AnnotationContinuation<'expr>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn pop_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, Self::Builder, Self::CustomSpecializationTarget>) -> Result<Option<AnnotationContinuation<'expr>>, Self::Error>;
        #[operation(child)]
        async fn canonical(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, tcx: TypeContext<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn existing(&self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(local)]
        async fn cache_enabled(&self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn cache_lookup(&self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, tcx: TypeContext<'db>) -> Result<Option<ExpressionCacheEntry<'db>>, Self::Error>;
        #[operation(source)]
        async fn cache_hit(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, entry: ExpressionCacheEntry<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn speculate(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId) -> Result<BuilderId, Self::Error>;
        #[operation(source)]
        async fn cache_commit(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, parent: BuilderId, child: BuilderId, expression: &ast::Expr, tcx: TypeContext<'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn contextual_dispatch(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, tcx: TypeContext<'db>) -> Result<source_expression::ContextualExpressionResult<'db>, Self::Error>;
        #[operation(source)]
        async fn other_expression(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, tcx: TypeContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn finish_expression(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, ty: Type<'db>, tcx: TypeContext<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn enter_callee(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId) -> Result<CalleeState<'db>, Self::Error>;
        #[operation(local)]
        async fn restore_callee(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, state: CalleeState<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn prepare_annotation_scope(&self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId, state: DeferredExpressionState) -> Result<AnnotationScope, Self::Error>;
        #[operation(local)]
        async fn enter_annotation_scope(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, scope: AnnotationScope) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn restore_annotation_scope(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, scope: AnnotationScope) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn store_annotation_qualifiers(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, qualifiers: TypeQualifiers) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn store_type_expression(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &ast::Expr, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn prepare_type_expression_scope(&self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId, mode: TypeExpressionMode) -> Result<Option<TypeExpressionScope>, Self::Error>;
        #[operation(local)]
        async fn enter_type_expression_scope(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, scope: TypeExpressionScope) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn restore_type_expression_before_store(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, scope: TypeExpressionScope) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn restore_type_expression_after_store(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, scope: TypeExpressionScope) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn start_annotation<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, annotation: &'expr ast::Expr, policy: PEP613Policy) -> Result<AnnotationStep<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_annotation<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, pending: AnnotationPending<'expr>, ty: Type<'db>) -> Result<AnnotationStep<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_qualifier<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, root: &AnnotationRoot<'expr>, pending: QualifierPending<'expr>, ty: TypeAndQualifiers<'db>) -> Result<AnnotationStep<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn start_type_expression<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, request: TypeExpressionRequest<'db, 'expr>) -> Result<TypeExpressionStep<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_type_expression<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, pending: TypeExpressionPending<'db, 'expr>, ty: Type<'db>) -> Result<TypeExpressionStep<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn start_assignment<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, target: &'expr ast::Expr, call: &'expr ast::ExprCall, definition: Definition<'db>, callable_type: Type<'db>) -> Result<assignment::Start<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn finish_assignment(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, target: &ast::Expr, call: &ast::ExprCall, callable_type: Type<'db>, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn legacy_typevar<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, state: legacy::State<'db, 'expr>) -> Result<legacy::Action<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_legacy_typevar<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, pending: legacy::Pending<'db, 'expr>, ty: Type<'db>) -> Result<legacy::State<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn subscript_receiver<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, subscript: &'expr ast::ExprSubscript, ty: Type<'db>) -> Result<subscript::SubscriptStart<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn subscript_slice<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, pending: subscript::SubscriptPending<'db, 'expr>, ty: Type<'db>) -> Result<Result<Type<'db>, Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn start_call<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, expression: &'expr ast::ExprCall, ty: Type<'db>, tcx: TypeContext<'db>) -> Result<call::Start<'db, 'expr>, Self::Error>;
        #[operation(local)]
        async fn prepare<'expr>(&self, id: BuilderId, arguments: &'expr ast::Arguments) -> Result<Preparation<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn preparation_step<'expr>(&self, preparation: Preparation<'db, 'expr>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<PreparationStep<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_splat<'expr>(&self, splat: Splat<'db, 'expr>, ty: Type<'db>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<Preparation<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn prepared_call<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, id: BuilderId, data: call::CallData<'db, 'expr>, arguments: CallArguments<'expr, 'db>) -> Result<PreparedCall<'db>, Self::Error>;
        #[operation(source)]
        async fn argument_step<'expr>(&self, owner: ActiveArgument, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<ArgumentStep<'db, 'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_argument<'expr>(&self, owner: PendingArgument, ty: Type<'db>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<ActiveArgument, Self::Error>;
        #[operation(source)]
        async fn finish_call<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: CompletedArgument) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn start_callable_annotation<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, request: callable_annotation::Request<'expr>) -> Result<callable_annotation::Active, Self::Error>;
        #[operation(source)]
        async fn callable_annotation_step<'expr>(&self, owner: callable_annotation::Active, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<callable_annotation::Step<'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_callable_annotation<'expr>(&self, owner: callable_annotation::Waiting, ty: Type<'db>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<callable_annotation::Active, Self::Error>;
        #[operation(source)]
        async fn finish_callable_annotation<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: callable_annotation::Finished) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn start_tuple_annotation<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, request: tuple_annotation::Request<'expr>) -> Result<tuple_annotation::Active, Self::Error>;
        #[operation(source)]
        async fn tuple_annotation_step<'expr>(&self, owner: tuple_annotation::Active, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<tuple_annotation::Step<'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_tuple_annotation<'expr>(&self, owner: tuple_annotation::Waiting, ty: Type<'db>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<tuple_annotation::Active, Self::Error>;
        #[operation(source)]
        async fn finish_tuple_annotation<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: tuple_annotation::Finished) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn start_tuple_value<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, id: BuilderId, tuple: &'expr ast::ExprTuple, context: TypeContext<'db>) -> Result<tuple_expression::Active, Self::Error>;
        #[operation(source)]
        async fn tuple_value_step<'expr>(&self, owner: tuple_expression::Active, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<tuple_expression::Step<'db, 'expr>, Self::Error>;
        #[operation(local)]
        async fn resume_tuple_value<'expr>(&self, owner: tuple_expression::Waiting, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>) -> Result<tuple_expression::Active, Self::Error>;
        #[operation(local)]
        async fn finish_tuple_value<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: tuple_expression::Finished) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn start_specialization<'expr>(&self, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, id: BuilderId, subscript: &'expr ast::ExprSubscript, value_ty: Type<'db>, class: StaticClassLiteral<'db>, generic_context: GenericContext<'db>, kind: ClassSpecializationKind) -> Result<ActiveSpecialization, Self::Error>;
        #[operation(source)]
        async fn specialization_step<'expr>(&self, owner: ActiveSpecialization, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<SpecializationStep<'expr>, Self::Error>;
        #[operation(source)]
        async fn resume_specialization<'expr>(&self, owner: PendingSpecialization, ty: Type<'db>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, builders: &mut BuilderStore<'_, 'db, 'ast>) -> Result<ActiveSpecialization, Self::Error>;
        #[operation(source)]
        async fn finish_specialization<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, owners: &mut LocalOwners<'db, 'expr, Self::Builder, Self::CustomSpecializationTarget>, owner: CompletedSpecialization) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn enter_paramspec(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn restore_paramspec(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId, previous: bool) -> Result<(), Self::Error>;
        #[operation(source)]
        async fn permit_paramspec(&self, policy: arguments::ArgumentPolicy, expression: &ast::Expr) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn complete<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, Self::Builder, Self::CustomSpecializationTarget>) -> Result<(), Self::Error>;

    }

    #[finite_capability]
    impl LocalFacts {
        fn string_request<'db, 'expr>(&self, scope: string_annotation::Scope<'expr>) -> TypeExpressionRequest<'db, 'expr> { scope.request() }
        fn unknown<'db>(&self) -> Type<'db> { Type::unknown() }
        fn tuple<'expr>(&self, expression: &'expr ast::Expr) -> Option<&'expr ast::ExprTuple> { expression.as_tuple_expr() }
        fn call<'expr>(&self, expression: &'expr ast::Expr) -> Option<&'expr ast::ExprCall> { expression.as_call_expr() }
        fn load_subscript<'expr>(&self, expression: &'expr ast::Expr) -> Option<&'expr ast::ExprSubscript> {
            expression.as_subscript_expr().filter(|subscript| subscript.ctx == ast::ExprContext::Load)
        }
        fn subscript_slice<'expr>(&self, pending: &subscript::SubscriptPending<'_, 'expr>) -> &'expr ast::Expr { pending.slice() }
        fn default_context<'db>(&self) -> TypeContext<'db> { TypeContext::default() }
        fn annotation_qualifiers(&self, annotation: TypeAndQualifiers<'_>) -> TypeQualifiers { annotation.qualifiers() }
    }

    #[synchronous(drive_sync)]
    #[capabilities(effects = LocalEffects, facts = LocalFacts)]
    #[passive_values(Work::StringAnnotation, Frame::StringAnnotation, Work::StartTupleExpression, Work::TupleExpression, Frame::TupleExpression, Work::StartTupleAnnotation, Work::TupleAnnotation, Frame::TupleAnnotation, Work::StartCallableAnnotation, Work::CallableAnnotation, Frame::CallableAnnotation, Work::Expression, Work::Callee, Work::Call, Work::LegacyTypeVar, Work::Prepare, Work::Arguments, Work::StartSpecialization, ClassSpecializationKind::ClassObject, ClassSpecializationKind::Subclass, Work::Specialization, Work::Return, Work::AnnotationBody, Work::AnnotationStep, AnnotationContinuation, PEP613Policy::Disallowed, Work::FinishAnnotation, Work::TypeExpression, AnnotationRoot, LocalResult::Type, LocalResult::Annotation, Frame::AnnotationResume, Frame::TypeExpressionResume, Frame::TypeExpressionFinish, Frame::Finish, Frame::Cache, Frame::Callee, Frame::AssignmentFinish, Frame::LegacyTypeVar, Frame::SubscriptReceiver, Frame::SubscriptSlice, Frame::Splat, Frame::Argument, Frame::Specialization, Frame::ParamSpec, TypeExpressionRequest::Expression, TypeExpressionMode::Scoped, TypeExpressionPending::ClassSpecialization, CalleeContinuation::Call, ExpressionMode::Cached, ExpressionMode::Uncached, ExpressionMode::Value, ExpressionMode::MaybeStandalone, ExpressionMode::GetOrInfer)]
    async fn drive<'root, 'db, 'ast, 'expr, E: LocalEffects<'db, 'ast>>(
        work: &mut Option<Work<'db, 'expr>>, invocation: &mut LocalInvocation<'root, 'db, 'ast, 'expr, E::Builder, E::CustomSpecializationTarget>, facts: LocalFacts, effects: &E, syntax: &'expr E::StringAnnotations,
    ) -> Result<Option<LocalResult<'db>>, E::Error> {
        #[cursor_loop]
        while let Some(current) = effects.next(work).await? {
            match current {
                Work::StringAnnotation(id, string) => {
                    match effects.parse_string_annotation(&invocation.builders, id, string, syntax).await? {
                        Some(parsed) => {
                            let scope = effects.prepare_string_annotation(&invocation.builders, id, string, parsed).await?;
                            effects.push(invocation, Frame::StringAnnotation(id, scope)).await?;
                            effects.enter_string_annotation(&mut invocation.builders, id, scope).await?;
                            effects.continue_with(work, Work::TypeExpression(id, facts.string_request(scope))).await?;
                        }
                        None => effects.continue_with(work, Work::Return(facts.unknown())).await?,
                    }
                }
                Work::AnnotationStart(id, annotation, deferred_state, policy) => {
                    let saved = effects.prepare_annotation_scope(&invocation.builders, id, deferred_state).await?;
                    effects.continue_with(work, Work::AnnotationBody(AnnotationRoot { builder: id, annotation, saved: Some(saved) }, policy)).await?;
                    effects.enter_annotation_scope(&mut invocation.builders, id, saved).await?;
                }
                Work::AnnotationBody(root, policy) => {
                    let step = effects.start_annotation(&mut invocation.builders, root.builder, root.annotation, policy).await?;
                    effects.continue_with(work, Work::AnnotationStep(root, step)).await?;
                }
                Work::AnnotationStep(root, step) => {
                    match step {
                        AnnotationStep::Complete(inference) => effects.continue_with(work, Work::FinishAnnotation(root, inference)).await?,
                        AnnotationStep::InferAnnotation { pending, annotation } => {
                            let builder = root.builder;
                            effects.push_annotation(invocation, AnnotationContinuation { root, pending }).await?;
                            effects.continue_with(work, Work::AnnotationBody(AnnotationRoot { builder, annotation, saved: None }, PEP613Policy::Disallowed)).await?;
                        }
                        AnnotationStep::InferType { pending, request } => {
                            let id = root.builder;
                            effects.push(invocation, Frame::AnnotationResume(root, pending)).await?;
                            effects.continue_with(work, Work::TypeExpression(id, request)).await?;
                        }
                    }
                }
                Work::FinishAnnotation(root, inference) => {
                    if let AnnotationStorage::Store { expression_ty } = inference.storage {
                        effects.store_type_expression(&mut invocation.builders, root.builder, root.annotation, expression_ty).await?;
                        effects.store_annotation_qualifiers(&mut invocation.builders, root.builder, root.annotation, facts.annotation_qualifiers(inference.annotation_ty)).await?;
                    }
                    if let Some(saved) = root.saved {
                        effects.restore_annotation_scope(&mut invocation.builders, root.builder, saved).await?;
                    }
                    match effects.pop_annotation(invocation).await? {
                        Some(AnnotationContinuation { root, pending }) => {
                            let step = effects.resume_qualifier(&mut invocation.builders, &root, pending, inference.annotation_ty).await?;
                            effects.continue_with(work, Work::AnnotationStep(root, step)).await?;
                        }
                        None => {
                            effects.complete(invocation).await?;
                            return Ok(Some(LocalResult::Annotation(inference.annotation_ty)));
                        }
                    }
                }
                Work::TypeExpression(id, request) => {
                    if let TypeExpressionRequest::Expression { expression, mode } = request
                        && let Some(scope) = effects.prepare_type_expression_scope(&invocation.builders, id, mode).await? {
                        effects.push(invocation, Frame::TypeExpressionFinish(id, expression, scope)).await?;
                        effects.enter_type_expression_scope(&mut invocation.builders, id, scope).await?;
                    }
                    let step = effects.start_type_expression(&mut invocation.builders, id, request).await?;
                    match step {
                        TypeExpressionStep::Complete(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                        TypeExpressionStep::String(string) => effects.continue_with(work, Work::StringAnnotation(id, string)).await?,
                        TypeExpressionStep::Callable(request) => effects.continue_with(work, Work::StartCallableAnnotation(id, request)).await?,
                        TypeExpressionStep::Tuple(request) => effects.continue_with(work, Work::StartTupleAnnotation(id, request)).await?,
                        TypeExpressionStep::Infer { pending, request } => {
                            effects.push(invocation, Frame::TypeExpressionResume(id, pending)).await?;
                            effects.continue_with(work, Work::TypeExpression(id, request)).await?;
                        }
                        TypeExpressionStep::RuntimeExpression { expression, pending } => {
                            effects.push(invocation, Frame::TypeExpressionResume(id, pending)).await?;
                            effects.continue_with(work, Work::Expression(id, expression, facts.default_context(), ExpressionMode::Cached)).await?;
                        }
                        TypeExpressionStep::ClassSpecialization { subscript, value_ty, class, generic_context } => {
                            effects.push(invocation, Frame::TypeExpressionResume(id, TypeExpressionPending::ClassSpecialization)).await?;
                            effects.continue_with(work, Work::StartSpecialization { builder: id, subscript, value_ty, class, generic_context, kind: ClassSpecializationKind::ClassObject }).await?;
                        }
                        TypeExpressionStep::SubclassSpecialization { subscript, value_ty, class, generic_context } => {
                            effects.continue_with(work, Work::StartSpecialization { builder: id, subscript, value_ty, class, generic_context, kind: ClassSpecializationKind::Subclass }).await?;
                        }
                    }
                }
                Work::Expression(id, expression, tcx, ExpressionMode::MaybeStandalone) => {
                    let canonical = effects.canonical(&mut invocation.builders, id, expression, tcx).await?;
                    match canonical {
                        Some(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                        None => effects.continue_with(work, Work::Expression(id, expression, tcx, ExpressionMode::Cached)).await?,
                    }
                }
                Work::Expression(id, expression, tcx, ExpressionMode::GetOrInfer) => {
                    let existing = effects.existing(&invocation.builders, id, expression).await?;
                    match existing {
                        Some(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                        None => effects.continue_with(work, Work::Expression(id, expression, tcx, ExpressionMode::Cached)).await?,
                    }
                }
                Work::Expression(id, expression, tcx, ExpressionMode::Cached) => {
                    if effects.cache_enabled(&invocation.builders, id).await? {
                        let entry = effects.cache_lookup(&invocation.builders, id, expression, tcx).await?;
                        match entry {
                            Some(entry) => {
                                let ty = effects.cache_hit(&mut invocation.builders, id, expression, entry).await?;
                                effects.continue_with(work, Work::Return(ty)).await?;
                            }
                            None => {
                                let child = effects.speculate(&mut invocation.builders, id).await?;
                                effects.push(invocation, Frame::Cache(id, child, expression, tcx)).await?;
                                effects.continue_with(work, Work::Expression(child, expression, tcx, ExpressionMode::Uncached)).await?;
                            }
                        }
                    } else {
                        effects.continue_with(work, Work::Expression(id, expression, tcx, ExpressionMode::Uncached)).await?;
                    }
                }
                Work::Expression(id, expression, tcx, ExpressionMode::Uncached) => {
                    let result = effects.contextual_dispatch(&mut invocation.builders, id, expression, tcx).await?;
                    match result {
                        source_expression::ContextualExpressionResult::Complete(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                        source_expression::ContextualExpressionResult::Value => effects.continue_with(work, Work::Expression(id, expression, tcx, ExpressionMode::Value)).await?,
                    }
                }
                Work::Expression(id, expression, tcx, ExpressionMode::Value) => {
                    match facts.call(expression) {
                        Some(call) => {
                            effects.push(invocation, Frame::Finish(id, expression, tcx)).await?;
                            effects.continue_with(work, Work::Callee(id, &call.func, CalleeContinuation::Call(call, tcx))).await?;
                        }
                        None => {
                            if let Some(subscript) = facts.load_subscript(expression) {
                                effects.push(invocation, Frame::Finish(id, expression, tcx)).await?;
                                effects.push(invocation, Frame::SubscriptReceiver(id, subscript)).await?;
                                effects.continue_with(work, Work::Expression(id, &subscript.value, facts.default_context(), ExpressionMode::Cached)).await?;
                            } else if let Some(tuple) = facts.tuple(expression) {
                                effects.push(invocation, Frame::Finish(id, expression, tcx)).await?;
                                effects.continue_with(work, Work::StartTupleExpression(id, tuple, tcx)).await?;
                            } else {
                                let ty = effects.other_expression(&mut invocation.builders, id, expression, tcx).await?;
                                effects.continue_with(work, Work::Return(ty)).await?;
                            }
                        }
                    }
                }
                Work::Callee(id, expression, call) => {
                    let saved = effects.enter_callee(&mut invocation.builders, id).await?;
                    effects.push(invocation, Frame::Callee(id, saved, call)).await?;
                    effects.continue_with(work, Work::Expression(id, expression, facts.default_context(), ExpressionMode::MaybeStandalone)).await?;
                }
                Work::Call(id, expression, ty, tcx) => {
                    let start = effects.start_call(&mut invocation.builders, id, expression, ty, tcx).await?;
                    match start {
                        call::Start::Complete(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                        call::Start::Prepare(data) => {
                            let preparation = effects.prepare(id, &expression.arguments).await?;
                            effects.continue_with(work, Work::Prepare(preparation, data)).await?;
                        }
                    }
                }
                Work::LegacyTypeVar(id, state) => {
                    let action = effects.legacy_typevar(&mut invocation.builders, id, state).await?;
                    match action {
                        legacy::Action::Infer { pending, expression, context } => {
                            effects.push(invocation, Frame::LegacyTypeVar(id, pending)).await?;
                            effects.continue_with(work, Work::Expression(id, expression, context, ExpressionMode::Cached)).await?;
                        }
                        legacy::Action::Complete(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                    }
                }
                Work::Prepare(preparation, data) => {
                    let id = preparation.builder;
                    let step = effects.preparation_step(preparation, &mut invocation.builders).await?;
                    match step {
                        PreparationStep::Continue(preparation) => effects.continue_with(work, Work::Prepare(preparation, data)).await?,
                        PreparationStep::Infer(splat, expression) => {
                            effects.push(invocation, Frame::Splat(splat, data)).await?;
                            effects.continue_with(work, Work::Expression(id, expression, facts.default_context(), ExpressionMode::GetOrInfer)).await?;
                        }
                        PreparationStep::Complete(arguments) => {
                            let prepared = effects.prepared_call(&mut invocation.builders, &mut invocation.owners, id, data, arguments).await?;
                            match prepared {
                                PreparedCall::Complete(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                                PreparedCall::Arguments(owner) => effects.continue_with(work, Work::Arguments(owner)).await?,
                            }
                        }
                    }
                }
                Work::Arguments(owner) => {
                    let step = effects.argument_step(owner, &mut invocation.owners, &mut invocation.builders).await?;
                    match step {
                        ArgumentStep::Continue(owner) => effects.continue_with(work, Work::Arguments(owner)).await?,
                        ArgumentStep::Infer { pending, builder, expression, tcx, policy } => {
                            effects.push(invocation, Frame::Argument(pending)).await?;
                            if effects.permit_paramspec(policy, expression).await? {
                                let previous = effects.enter_paramspec(&mut invocation.builders, builder).await?;
                                effects.push(invocation, Frame::ParamSpec(builder, previous)).await?;
                            }
                            effects.continue_with(work, Work::Expression(builder, expression, tcx, ExpressionMode::Cached)).await?;
                        }
                        ArgumentStep::Complete(owner) => {
                            let ty = effects.finish_call(&mut invocation.builders, &mut invocation.owners, owner).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                    }
                }
                Work::StartCallableAnnotation(builder, request) => {
                    let owner = effects.start_callable_annotation(&mut invocation.owners, &mut invocation.builders, builder, request).await?;
                    effects.continue_with(work, Work::CallableAnnotation(owner)).await?;
                }
                Work::CallableAnnotation(owner) => {
                    let step = effects.callable_annotation_step(owner, &mut invocation.owners, &mut invocation.builders).await?;
                    match step {
                        callable_annotation::Step::Infer { owner, builder, expression } => {
                            effects.push(invocation, Frame::CallableAnnotation(owner)).await?;
                            effects.continue_with(work, Work::TypeExpression(builder, TypeExpressionRequest::Expression { expression, mode: TypeExpressionMode::Scoped })).await?;
                        }
                        callable_annotation::Step::Complete(owner) => {
                            let ty = effects.finish_callable_annotation(&mut invocation.builders, &mut invocation.owners, owner).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                    }
                }
                Work::StartTupleExpression(builder, tuple, context) => {
                    let owner = effects.start_tuple_value(&mut invocation.owners, builder, tuple, context).await?;
                    effects.continue_with(work, Work::TupleExpression(owner)).await?;
                }
                Work::TupleExpression(owner) => {
                    let step = effects.tuple_value_step(owner, &mut invocation.owners, &mut invocation.builders).await?;
                    match step {
                        tuple_expression::Step::Continue(owner) => effects.continue_with(work, Work::TupleExpression(owner)).await?,
                        tuple_expression::Step::Infer { owner, builder, expression, context } => {
                            effects.push(invocation, Frame::TupleExpression(owner)).await?;
                            effects.continue_with(work, Work::Expression(builder, expression, context, ExpressionMode::Cached)).await?;
                        }
                        tuple_expression::Step::Complete(owner) => {
                            let ty = effects.finish_tuple_value(&mut invocation.owners, owner).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                    }
                }
                Work::StartTupleAnnotation(builder, request) => {
                    let owner = effects.start_tuple_annotation(&mut invocation.owners, &mut invocation.builders, builder, request).await?;
                    effects.continue_with(work, Work::TupleAnnotation(owner)).await?;
                }
                Work::TupleAnnotation(owner) => {
                    let step = effects.tuple_annotation_step(owner, &mut invocation.owners, &mut invocation.builders).await?;
                    match step {
                        tuple_annotation::Step::InferType { owner, builder, expression } => {
                            effects.push(invocation, Frame::TupleAnnotation(owner)).await?;
                            effects.continue_with(work, Work::TypeExpression(builder, TypeExpressionRequest::Expression { expression, mode: TypeExpressionMode::Scoped })).await?;
                        }
                        tuple_annotation::Step::InferValue { owner, builder, expression } => {
                            effects.push(invocation, Frame::TupleAnnotation(owner)).await?;
                            effects.continue_with(work, Work::Expression(builder, expression, facts.default_context(), ExpressionMode::Cached)).await?;
                        }
                        tuple_annotation::Step::Continue(owner) => effects.continue_with(work, Work::TupleAnnotation(owner)).await?,
                        tuple_annotation::Step::Complete(owner) => {
                            let ty = effects.finish_tuple_annotation(&mut invocation.builders, &mut invocation.owners, owner).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                    }
                }
                Work::StartSpecialization { builder, subscript, value_ty, class, generic_context, kind } => {
                    let owner = effects.start_specialization(&mut invocation.owners, builder, subscript, value_ty, class, generic_context, kind).await?;
                    effects.continue_with(work, Work::Specialization(owner)).await?;
                }
                Work::Specialization(owner) => {
                    let step = effects.specialization_step(owner, &mut invocation.owners, &mut invocation.builders).await?;
                    match step {
                        SpecializationStep::Continue(owner) => effects.continue_with(work, Work::Specialization(owner)).await?,
                        SpecializationStep::Infer { pending, builder, request } => {
                            effects.push(invocation, Frame::Specialization(pending)).await?;
                            match request {
                                specialization::ChildRequest::TypeExpression(expression) => effects.continue_with(work, Work::TypeExpression(builder, TypeExpressionRequest::Expression { expression, mode: TypeExpressionMode::Scoped })).await?,
                                specialization::ChildRequest::Expression(expression) => effects.continue_with(work, Work::Expression(builder, expression, facts.default_context(), ExpressionMode::Cached)).await?,
                            }
                        }
                        SpecializationStep::Complete(owner) => {
                            let ty = effects.finish_specialization(&mut invocation.builders, &mut invocation.owners, owner).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                    }
                }
                Work::Return(ty) => {
                    let frame = effects.pop(&mut invocation.frames).await?;
                    match frame {
                        None => {
                            effects.complete(invocation).await?;
                            return Ok(Some(LocalResult::Type(ty)));
                        }
                        Some(Frame::AnnotationResume(root, pending)) => {
                            let step = effects.resume_annotation(&mut invocation.builders, root.builder, pending, ty).await?;
                            effects.continue_with(work, Work::AnnotationStep(root, step)).await?;
                        }
                        Some(Frame::TypeExpressionResume(id, pending)) => {
                            let step = effects.resume_type_expression(&mut invocation.builders, id, pending, ty).await?;
                            match step {
                                TypeExpressionStep::Complete(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                        TypeExpressionStep::String(string) => effects.continue_with(work, Work::StringAnnotation(id, string)).await?,
                                TypeExpressionStep::Callable(request) => effects.continue_with(work, Work::StartCallableAnnotation(id, request)).await?,
                        TypeExpressionStep::Tuple(request) => effects.continue_with(work, Work::StartTupleAnnotation(id, request)).await?,
                                TypeExpressionStep::Infer { pending, request } => {
                                    effects.push(invocation, Frame::TypeExpressionResume(id, pending)).await?;
                                    effects.continue_with(work, Work::TypeExpression(id, request)).await?;
                                }
                                TypeExpressionStep::RuntimeExpression { expression, pending } => {
                                    effects.push(invocation, Frame::TypeExpressionResume(id, pending)).await?;
                                    effects.continue_with(work, Work::Expression(id, expression, facts.default_context(), ExpressionMode::Cached)).await?;
                                }
                                TypeExpressionStep::ClassSpecialization { subscript, value_ty, class, generic_context } => {
                                    effects.push(invocation, Frame::TypeExpressionResume(id, TypeExpressionPending::ClassSpecialization)).await?;
                                    effects.continue_with(work, Work::StartSpecialization { builder: id, subscript, value_ty, class, generic_context, kind: ClassSpecializationKind::ClassObject }).await?;
                                }
                                TypeExpressionStep::SubclassSpecialization { subscript, value_ty, class, generic_context } => {
                                    effects.continue_with(work, Work::StartSpecialization { builder: id, subscript, value_ty, class, generic_context, kind: ClassSpecializationKind::Subclass }).await?;
                                }
                            }
                        }
                        Some(Frame::StringAnnotation(id, scope)) => {
                            effects.finish_string_annotation(&mut invocation.builders, id, scope).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                        Some(Frame::TypeExpressionFinish(id, expression, scope)) => {
                            effects.restore_type_expression_before_store(&mut invocation.builders, id, scope).await?;
                            effects.store_type_expression(&mut invocation.builders, id, expression, ty).await?;
                            effects.restore_type_expression_after_store(&mut invocation.builders, id, scope).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                        Some(Frame::Finish(id, expression, tcx)) => {
                            let ty = effects.finish_expression(&mut invocation.builders, id, expression, ty, tcx).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                        Some(Frame::Cache(parent, child, expression, tcx)) => {
                            effects.cache_commit(&mut invocation.builders, parent, child, expression, tcx, ty).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                        Some(Frame::Callee(id, saved, call)) => {
                            effects.restore_callee(&mut invocation.builders, id, saved).await?;
                            match call {
                                CalleeContinuation::Call(expression, tcx) => effects.continue_with(work, Work::Call(id, expression, ty, tcx)).await?,
                                CalleeContinuation::Return => effects.continue_with(work, Work::Return(ty)).await?,
                                CalleeContinuation::Assignment { target, call, definition, context } => {
                                    effects.push(invocation, Frame::AssignmentFinish(id, target, call, ty)).await?;
                                    let start = effects.start_assignment(&mut invocation.builders, id, target, call, definition, ty).await?;
                                    match start {
                                        assignment::Start::Complete(ty) => effects.continue_with(work, Work::Return(ty)).await?,
                                        assignment::Start::LegacyTypeVar(state) => effects.continue_with(work, Work::LegacyTypeVar(id, state)).await?,
                                        assignment::Start::Call => effects.continue_with(work, Work::Call(id, call, ty, context)).await?,
                                    }
                                }
                            }
                        }
                        Some(Frame::AssignmentFinish(id, target, call, callable_type)) => {
                            let ty = effects.finish_assignment(&mut invocation.builders, id, target, call, callable_type, ty).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                        Some(Frame::LegacyTypeVar(id, pending)) => {
                            let state = effects.resume_legacy_typevar(&mut invocation.builders, id, pending, ty).await?;
                            effects.continue_with(work, Work::LegacyTypeVar(id, state)).await?;
                        }
                        Some(Frame::SubscriptReceiver(id, subscript)) => {
                            let start = effects.subscript_receiver(&mut invocation.builders, id, subscript, ty).await?;
                            match start {
                                subscript::SubscriptStart::Complete(result) => {
                                    let ty = match result { Ok(ty) | Err(ty) => ty };
                                    effects.continue_with(work, Work::Return(ty)).await?;
                                }
                                subscript::SubscriptStart::Slice(pending) => {
                                    let slice = facts.subscript_slice(&pending);
                                    effects.push(invocation, Frame::SubscriptSlice(id, pending)).await?;
                                    effects.continue_with(work, Work::Expression(id, slice, facts.default_context(), ExpressionMode::Cached)).await?;
                                }
                                subscript::SubscriptStart::ClassSpecialization { subscript, value_ty, class, generic_context } => {
                                    effects.continue_with(work, Work::StartSpecialization { builder: id, subscript, value_ty, class, generic_context, kind: ClassSpecializationKind::ClassObject }).await?;
                                }
                            }
                        }
                        Some(Frame::SubscriptSlice(id, pending)) => {
                            let result = effects.subscript_slice(&mut invocation.builders, id, pending, ty).await?;
                            let ty = match result { Ok(ty) | Err(ty) => ty };
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                        Some(Frame::Splat(splat, data)) => {
                            let preparation = effects.resume_splat(splat, ty, &mut invocation.builders).await?;
                            effects.continue_with(work, Work::Prepare(preparation, data)).await?;
                        }
                        Some(Frame::Argument(pending)) => {
                            let owner = effects.resume_argument(pending, ty, &mut invocation.owners, &mut invocation.builders).await?;
                            effects.continue_with(work, Work::Arguments(owner)).await?;
                        }
                        Some(Frame::CallableAnnotation(owner)) => {
                            let owner = effects.resume_callable_annotation(owner, ty, &mut invocation.owners, &mut invocation.builders).await?;
                            effects.continue_with(work, Work::CallableAnnotation(owner)).await?;
                        }
                        Some(Frame::TupleExpression(owner)) => {
                            let owner = effects.resume_tuple_value(owner, &mut invocation.owners).await?;
                            effects.continue_with(work, Work::TupleExpression(owner)).await?;
                        }
                        Some(Frame::TupleAnnotation(owner)) => {
                            let owner = effects.resume_tuple_annotation(owner, ty, &mut invocation.owners, &mut invocation.builders).await?;
                            effects.continue_with(work, Work::TupleAnnotation(owner)).await?;
                        }
                        Some(Frame::Specialization(pending)) => {
                            let owner = effects.resume_specialization(pending, ty, &mut invocation.owners, &mut invocation.builders).await?;
                            effects.continue_with(work, Work::Specialization(owner)).await?;
                        }
                        Some(Frame::ParamSpec(id, previous)) => {
                            effects.restore_paramspec(&mut invocation.builders, id, previous).await?;
                            effects.continue_with(work, Work::Return(ty)).await?;
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}

impl<'db, 'ast, F: OrdinaryCustomSpecialization<'db>> SynchronousLocalEffects<'db, 'ast>
    for OrdinaryLocalEffectsWithTarget<F>
{
    type Error = Infallible;
    type Builder = ConstraintSetBuilder<'db>;
    type CustomSpecializationTarget = F::Target;
    type StringAnnotations = string_annotation::OrdinaryStorage;

    fn parse_string_annotation<'expr>(
        &self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        string: &ast::ExprStringLiteral, storage: &'expr Self::StringAnnotations,
    ) -> Result<Option<&'expr ast::Expr>, Infallible> {
        let builder = builders.builder(id);
        Ok(crate::types::string_annotation::parse_string_annotation(
            &builder.context, builder.inference_flags(), string,
        ).map(|parsed| &*storage.get_or_init(|| typed_arena::Arena::with_capacity(1)).alloc(parsed)).map(|parsed| parsed.expr()))
    }

    fn prepare_string_annotation<'expr>(
        &self, builders: &BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        string: &'expr ast::ExprStringLiteral, parsed: &'expr ast::Expr,
    ) -> Result<string_annotation::Scope<'expr>, Infallible> {
        Ok(string_annotation::Scope::prepare(builders.builder(id), string, parsed))
    }

    fn enter_string_annotation(
        &self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        scope: string_annotation::Scope<'_>,
    ) -> Result<(), Infallible> {
        let builder = builders.get_mut(id);
        builder.string_annotations.insert(scope.original_key());
        scope.enter(builder);
        Ok(())
    }

    fn finish_string_annotation(
        &self, builders: &mut BuilderStore<'_, 'db, 'ast>, id: BuilderId,
        scope: string_annotation::Scope<'_>,
    ) -> Result<(), Infallible> {
        let builder = builders.get_mut(id);
        scope.restore(builder);
        scope.store_flags(builder, scope.parsed_flags(builder));
        Ok(())
    }


    fn next<'expr>(
        &self,
        work: &mut Option<Work<'db, 'expr>>,
    ) -> Result<Option<Work<'db, 'expr>>, Self::Error> {
        Ok(work.take())
    }
    fn continue_with<'expr>(
        &self,
        work: &mut Option<Work<'db, 'expr>>,
        next: Work<'db, 'expr>,
    ) -> Result<(), Self::Error> {
        *work = Some(next);
        Ok(())
    }
    fn push<'expr>(
        &self,
        invocation: &mut LocalInvocation<
            '_,
            'db,
            'ast,
            'expr,
            ConstraintSetBuilder<'db>,
            F::Target,
        >,
        frame: Frame<'db, 'expr>,
    ) -> Result<(), Self::Error> {
        invocation.frames.push(frame);
        Ok(())
    }
    fn pop<'expr>(
        &self,
        frames: &mut Vec<Frame<'db, 'expr>>,
    ) -> Result<Option<Frame<'db, 'expr>>, Self::Error> {
        Ok(frames.pop())
    }

    fn push_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, ConstraintSetBuilder<'db>, F::Target>, continuation: AnnotationContinuation<'expr>) -> Result<(), Self::Error> {
        invocation.annotations.push(continuation);
        Ok(())
    }

    fn pop_annotation<'expr>(&self, invocation: &mut LocalInvocation<'_, 'db, 'ast, 'expr, ConstraintSetBuilder<'db>, F::Target>) -> Result<Option<AnnotationContinuation<'expr>>, Self::Error> {
        Ok(invocation.annotations.pop())
    }

    fn canonical(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let builder = builders.get_mut(id);
        Ok(builder.index.try_expression(expression).map(|standalone| {
            builder.infer_standalone_expression_impl(expression, standalone, tcx)
        }))
    }
    fn existing(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        let builder = builders.builder(id);
        let existing = builder.try_expression_type(expression);
        debug_assert!(existing.is_some() || !builder.index.is_standalone_expression(expression));
        Ok(existing)
    }
    fn cache_enabled(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
    ) -> Result<bool, Self::Error> {
        Ok(builders.builder(id).expression_cache.is_some())
    }
    fn cache_lookup(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Option<ExpressionCacheEntry<'db>>, Self::Error> {
        Ok(builders
            .builder(id)
            .expression_cache
            .as_ref()
            .and_then(|cache| cache.borrow().get(expression.into(), tcx).cloned()))
    }
    fn cache_hit(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        entry: ExpressionCacheEntry<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let builder = builders.get_mut(id);
        Ok(match entry {
            ExpressionCacheEntry::Small(ty) => {
                builder.store_expression_type(expression, ty);
                ty
            }
            ExpressionCacheEntry::Full(inference) => {
                let ty = inference.expression_type(expression.into());
                builder.extend_expression_cache_entry(&inference);
                ty
            }
        })
    }
    fn speculate(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
    ) -> Result<BuilderId, Self::Error> {
        Ok(builders.speculate(id, false))
    }
    fn cache_commit(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        parent: BuilderId,
        child: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        let inference = builders
            .take_speculative(child)
            .into_expression_cache_entry();
        let builder = builders.get_mut(parent);
        let key = expression.into();
        let cached = if inference.is_single_expression(key, ty) {
            builder.store_expression_type(expression, ty);
            ExpressionCacheEntry::Small(ty)
        } else {
            builder.extend_expression_cache_entry(&inference);
            ExpressionCacheEntry::Full(Rc::new(inference))
        };
        if let Some(cache) = &builder.expression_cache {
            cache.borrow_mut().insert(key, tcx, cached);
        }
        Ok(())
    }
    fn contextual_dispatch(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<source_expression::ContextualExpressionResult<'db>, Self::Error> {
        source_expression::contextual_expression_sync(
            builders.get_mut(id),
            expression,
            tcx,
            &source_expression::OrdinaryContextualExpressionEffects,
        )
    }
    fn other_expression(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(crate::types::signatures::effects::legacy_inline(
            builders.get_mut(id).infer_value_expression_with(
                &source_expression::LegacySourceExpressionEffects,
                expression,
                tcx,
            ),
        ))
    }
    fn finish_expression(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builders
            .get_mut(id)
            .finish_expression_type(expression, ty, tcx))
    }
    fn enter_callee(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
    ) -> Result<CalleeState<'db>, Self::Error> {
        let builder = builders.get_mut(id);
        Ok(CalleeState {
            binding: builder.typevar_binding_context.take(),
            check_unbound: builder
                .context
                .inference_flags
                .replace(InferenceFlags::CHECK_UNBOUND_TYPEVARS, true),
        })
    }
    fn restore_callee(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        state: CalleeState<'db>,
    ) -> Result<(), Self::Error> {
        let builder = builders.get_mut(id);
        builder
            .context
            .inference_flags
            .set(InferenceFlags::CHECK_UNBOUND_TYPEVARS, state.check_unbound);
        builder.typevar_binding_context = state.binding;
        Ok(())
    }
    fn prepare_annotation_scope(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        state: DeferredExpressionState,
    ) -> Result<AnnotationScope, Self::Error> {
        let builder = builders.builder(id);
        let in_stub = !state.in_string_annotation() && builder.in_stub();
        Ok(AnnotationScope::prepare(builder, state, in_stub))
    }
    fn enter_annotation_scope(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: AnnotationScope,
    ) -> Result<(), Self::Error> {
        scope.enter(builders.get_mut(id));
        Ok(())
    }
    fn restore_annotation_scope(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: AnnotationScope,
    ) -> Result<(), Self::Error> {
        scope.restore(builders.get_mut(id));
        Ok(())
    }
    fn store_annotation_qualifiers(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        qualifiers: TypeQualifiers,
    ) -> Result<(), Self::Error> {
        builders
            .get_mut(id)
            .store_qualifiers(expression, qualifiers);
        Ok(())
    }
    fn store_type_expression(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &ast::Expr,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        builders.get_mut(id).store_expression_type(expression, ty);
        Ok(())
    }
    fn prepare_type_expression_scope(
        &self,
        builders: &BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        mode: TypeExpressionMode,
    ) -> Result<Option<TypeExpressionScope>, Self::Error> {
        if matches!(mode, TypeExpressionMode::NoStore) {
            return Ok(None);
        }
        let builder = builders.builder(id);
        Ok(TypeExpressionScope::prepare(
            builder,
            mode,
            builder.in_stub(),
        ))
    }
    fn enter_type_expression_scope(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: TypeExpressionScope,
    ) -> Result<(), Self::Error> {
        scope.enter(builders.get_mut(id));
        Ok(())
    }
    fn restore_type_expression_before_store(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: TypeExpressionScope,
    ) -> Result<(), Self::Error> {
        scope.restore_before_store(builders.get_mut(id));
        Ok(())
    }
    fn restore_type_expression_after_store(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        scope: TypeExpressionScope,
    ) -> Result<(), Self::Error> {
        scope.restore_after_store(builders.get_mut(id));
        Ok(())
    }
    fn start_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        annotation: &'expr ast::Expr,
        policy: PEP613Policy,
    ) -> Result<AnnotationStep<'db, 'expr>, Self::Error> {
        annotation_expression::start_annotation_sync(
            builders.get_mut(id),
            annotation,
            policy,
            &annotation_expression::OrdinaryAnnotationEffects,
        )
    }
    fn resume_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: AnnotationPending<'expr>,
        ty: Type<'db>,
    ) -> Result<AnnotationStep<'db, 'expr>, Self::Error> {
        annotation_expression::resume_annotation_sync(
            builders.get_mut(id),
            pending,
            ty,
            &annotation_expression::OrdinaryAnnotationEffects,
        )
    }
    fn resume_qualifier<'expr>(&self, builders: &mut BuilderStore<'_, 'db, 'ast>, root: &AnnotationRoot<'expr>, pending: QualifierPending<'expr>, ty: TypeAndQualifiers<'db>) -> Result<AnnotationStep<'db, 'expr>, Self::Error> {
        super::annotation_expression::resume_qualifier_sync(builders.get_mut(root.builder), root.annotation, pending, ty, &super::annotation_expression::OrdinaryAnnotationEffects)
    }

    fn start_type_expression<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        request: TypeExpressionRequest<'db, 'expr>,
    ) -> Result<TypeExpressionStep<'db, 'expr>, Self::Error> {
        type_expression::start_type_expression_sync(
            builders.get_mut(id),
            request,
            &type_expression::OrdinaryTypeExpressionEffects,
        )
    }
    fn resume_type_expression<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: TypeExpressionPending<'db, 'expr>,
        ty: Type<'db>,
    ) -> Result<TypeExpressionStep<'db, 'expr>, Self::Error> {
        type_expression::resume_type_expression_sync(
            builders.get_mut(id),
            pending,
            ty,
            &type_expression::OrdinaryTypeExpressionEffects,
        )
    }
    fn start_call<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        expression: &'expr ast::ExprCall,
        ty: Type<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<call::Start<'db, 'expr>, Self::Error> {
        call::start_sync(
            builders.get_mut(id),
            expression,
            ty,
            tcx,
            call::CallFacts,
            &call::OrdinaryCallEffects,
        )
    }
    fn start_assignment<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        target: &'expr ast::Expr,
        call: &'expr ast::ExprCall,
        definition: Definition<'db>,
        callable_type: Type<'db>,
    ) -> Result<assignment::Start<'db, 'expr>, Self::Error> {
        assignment::start_sync(
            builders.get_mut(id),
            target,
            call,
            definition,
            callable_type,
            assignment::AssignmentFacts,
            &assignment::OrdinaryAssignmentEffects,
        )
    }
    fn finish_assignment(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        target: &ast::Expr,
        call: &ast::ExprCall,
        callable_type: Type<'db>,
        ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        assignment::finish_sync(
            builders.get_mut(id),
            target,
            call,
            callable_type,
            ty,
            assignment::AssignmentFacts,
            &assignment::OrdinaryAssignmentEffects,
        )
    }
    fn legacy_typevar<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        state: legacy::State<'db, 'expr>,
    ) -> Result<legacy::Action<'db, 'expr>, Self::Error> {
        legacy::advance_sync(
            state,
            builders.get_mut(id),
            &legacy::OrdinaryLegacyTypeVarEffects,
        )
    }
    fn resume_legacy_typevar<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: legacy::Pending<'db, 'expr>,
        ty: Type<'db>,
    ) -> Result<legacy::State<'db, 'expr>, Self::Error> {
        legacy::resume_sync(
            pending,
            ty,
            builders.get_mut(id),
            &legacy::OrdinaryLegacyTypeVarEffects,
        )
    }
    fn subscript_receiver<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        subscript: &'expr ast::ExprSubscript,
        ty: Type<'db>,
    ) -> Result<subscript::SubscriptStart<'db, 'expr>, Self::Error> {
        subscript::subscript_after_receiver_sync(
            builders.get_mut(id),
            subscript,
            ty,
            &subscript::OrdinarySubscriptEffects,
        )
    }
    fn subscript_slice<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        pending: subscript::SubscriptPending<'db, 'expr>,
        ty: Type<'db>,
    ) -> Result<Result<Type<'db>, Type<'db>>, Self::Error> {
        subscript::subscript_after_slice_sync(
            builders.get_mut(id),
            pending,
            ty,
            &subscript::OrdinarySubscriptEffects,
        )
    }
    fn prepare<'expr>(
        &self,
        id: BuilderId,
        source: &'expr ast::Arguments,
    ) -> Result<Preparation<'db, 'expr>, Self::Error> {
        Ok(Preparation {
            builder: id,
            source,
            cursor: ArgumentsIter::from_ast(source),
            arguments: CallArguments::with_capacity(source.len()),
        })
    }
    fn preparation_step<'expr>(
        &self,
        preparation: Preparation<'db, 'expr>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<PreparationStep<'db, 'expr>, Self::Error> {
        Ok(crate::types::signatures::effects::legacy_inline(
            preparation::advance(
                preparation,
                builders,
                &preparation::OrdinaryPreparationEffects,
            ),
        ))
    }
    fn resume_splat<'expr>(
        &self,
        splat: Splat<'db, 'expr>,
        ty: Type<'db>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<Preparation<'db, 'expr>, Self::Error> {
        let Splat {
            mut preparation,
            source,
            argument,
        } = splat;
        let builder = builders.get_mut(preparation.builder);
        let ty = if let ast::ArgOrKeyword::Arg(expression) = source
            && expression.is_starred_expr()
        {
            builder.store_expression_type(expression, ty);
            ty
        } else {
            builder.try_narrow_dict_kwargs(ty, &source).unwrap_or(ty)
        };
        preparation.arguments.push_argument(argument, Some(ty));
        Ok(preparation)
    }
    fn prepared_call<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        id: BuilderId,
        data: call::CallData<'db, 'expr>,
        arguments: CallArguments<'expr, 'db>,
    ) -> Result<PreparedCall<'db>, Self::Error> {
        owned_prepared_call_sync(
            builders,
            owners,
            id,
            data,
            arguments,
            &OrdinaryOwnedArgumentEffectsWithTarget::<F::Target>::default(),
        )
    }
    fn argument_step<'expr>(
        &self,
        owner: ActiveArgument,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<ArgumentStep<'db, 'expr>, Self::Error> {
        owned_argument_step_sync(
            owner,
            owners,
            builders,
            &OrdinaryOwnedArgumentEffectsWithTarget::<F::Target>::default(),
        )
    }
    fn resume_argument<'expr>(
        &self,
        owner: PendingArgument,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<ActiveArgument, Self::Error> {
        owned_resume_argument_sync(
            owner,
            ty,
            owners,
            builders,
            &OrdinaryOwnedArgumentEffectsWithTarget::<F::Target>::default(),
        )
    }
    fn finish_call<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: CompletedArgument,
    ) -> Result<Type<'db>, Self::Error> {
        owned_finish_call_sync(
            builders,
            owners,
            owner,
            &OrdinaryOwnedArgumentEffectsWithTarget::<F::Target>::default(),
        )
    }
    fn start_callable_annotation<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        request: callable_annotation::Request<'expr>,
    ) -> Result<callable_annotation::Active, Infallible> {
        Ok(owners.push_callable_annotation(id, request, builders.get_mut(id)))
    }

    fn callable_annotation_step<'expr>(
        &self,
        owner: callable_annotation::Active,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<callable_annotation::Step<'expr>, Infallible> {
        let taken = owners.take_callable_active(owner);
        let builder = taken.payload.builder;
        let taken = taken.map(|state| {
            let Ok(action) = callable_annotation::advance_sync(state, builders.get_mut(builder), callable_annotation::Facts, &callable_annotation::OrdinaryEffects);
            action
        });
        Ok(owners.install_callable_action(taken))
    }

    fn resume_callable_annotation<'expr>(
        &self,
        owner: callable_annotation::Waiting,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<callable_annotation::Active, Infallible> {
        let taken = owners.take_callable_pending(owner);
        let builder = taken.payload.builder;
        let taken = taken.map(|pending| {
            let Ok(state) = callable_annotation::resume_sync(pending, ty, builders.builder(builder), &callable_annotation::OrdinaryEffects);
            state
        });
        Ok(owners.install_callable_active(taken))
    }

    fn finish_callable_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: callable_annotation::Finished,
    ) -> Result<Type<'db>, Infallible> {
        let mut lease = owners.callable_completion(owner);
        let index = lease.index;
        let (builder, completed) = lease.parts_mut();
        let Ok(ty) = callable_annotation::finish_sync(completed, builders.get_mut(builder), callable_annotation::Facts, &callable_annotation::OrdinaryEffects);
        drop(lease);
        Ok(owners.retire(FinishedOwner { index, ty }))
    }

    fn start_tuple_annotation<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        _builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        request: tuple_annotation::Request<'expr>,
    ) -> Result<tuple_annotation::Active, Infallible> {
        Ok(owners.push_tuple_annotation(id, request))
    }

    fn tuple_annotation_step<'expr>(
        &self,
        owner: tuple_annotation::Active,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<tuple_annotation::Step<'expr>, Infallible> {
        let taken = owners.take_tuple_active(owner);
        let builder = taken.payload.builder;
        let taken = taken.map(|state| {
            let Ok(action) = tuple_annotation::advance_sync(state, builders.get_mut(builder), tuple_annotation::Facts, &tuple_annotation::OrdinaryEffects);
            action
        });
        Ok(owners.install_tuple_action(taken))
    }

    fn resume_tuple_annotation<'expr>(
        &self,
        owner: tuple_annotation::Waiting,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<tuple_annotation::Active, Infallible> {
        let taken = owners.take_tuple_pending(owner);
        let builder = taken.payload.builder;
        let taken = taken.map(|pending| {
            let Ok(state) = tuple_annotation::resume_sync(pending, ty, builders.get_mut(builder), tuple_annotation::Facts, &tuple_annotation::OrdinaryEffects);
            state
        });
        Ok(owners.install_tuple_active(taken))
    }

    fn finish_tuple_annotation<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: tuple_annotation::Finished,
    ) -> Result<Type<'db>, Infallible> {
        let mut lease = owners.tuple_completion(owner);
        let index = lease.index;
        let (builder, completed) = lease.parts_mut();
        let Ok(ty) = tuple_annotation::finish_sync(completed, builders.get_mut(builder), tuple_annotation::Facts, &tuple_annotation::OrdinaryEffects);
        drop(lease);
        Ok(owners.retire(FinishedOwner { index, ty }))
    }

    fn start_tuple_value<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        id: BuilderId,
        tuple: &'expr ast::ExprTuple,
        context: TypeContext<'db>,
    ) -> Result<tuple_expression::Active, Infallible> {
        Ok(owners.push_tuple_value(id, tuple, context))
    }
    fn tuple_value_step<'expr>(
        &self,
        owner: tuple_expression::Active,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<tuple_expression::Step<'db, 'expr>, Infallible> {
        let lease = owners.tuple_value_lease(owner.0);
        let Ok(action) = super::tuple_expression::advance_tuple_expression_sync(
            lease.state,
            builders,
            super::tuple_expression::TupleExpressionFacts,
            &super::tuple_expression::OrdinaryTupleExpressionEffects,
        );
        Ok(LocalOwners::<
            'db,
            'expr,
            ConstraintSetBuilder<'db>,
            F::Target,
        >::tuple_value_step(owner.0, action))
    }
    fn resume_tuple_value<'expr>(
        &self,
        owner: tuple_expression::Waiting,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
    ) -> Result<tuple_expression::Active, Infallible> {
        assert!(matches!(
            owners.tuple_value_lease(owner.0).state.phase,
            tuple_expression::Phase::Waiting
        ));
        Ok(tuple_expression::Active(owner.0))
    }
    fn finish_tuple_value<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: tuple_expression::Finished,
    ) -> Result<Type<'db>, Infallible> {
        Ok(owners.retire_tuple_value(owner))
    }

    fn start_specialization<'expr>(
        &self,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        id: BuilderId,
        subscript: &'expr ast::ExprSubscript,
        value_ty: Type<'db>,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
        kind: ClassSpecializationKind,
    ) -> Result<ActiveSpecialization, Infallible> {
        Ok(owners.push_specialization(
            id,
            specialization::Request {
                subscript,
                value_ty,
                generic_context,
                target: match kind {
                    ClassSpecializationKind::ClassObject => SpecializationTarget::ClassObject(class),
                    ClassSpecializationKind::Subclass => SpecializationTarget::ClassSubclass(class),
                },
                protocol_guard: Some(class),
            },
        ))
    }

    fn specialization_step<'expr>(
        &self,
        owner: ActiveSpecialization,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<SpecializationStep<'expr>, Infallible> {
        owned_specialization_step_sync(owner, owners, builders, self)
    }

    fn resume_specialization<'expr>(
        &self,
        owner: PendingSpecialization,
        ty: Type<'db>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
    ) -> Result<ActiveSpecialization, Infallible> {
        owned_resume_specialization_sync(owner, ty, owners, builders, self)
    }

    fn finish_specialization<'expr>(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        owners: &mut LocalOwners<'db, 'expr, ConstraintSetBuilder<'db>, F::Target>,
        owner: CompletedSpecialization,
    ) -> Result<Type<'db>, Infallible> {
        owned_finish_specialization_sync(builders, owners, owner, self)
    }
    fn enter_paramspec(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
    ) -> Result<bool, Self::Error> {
        Ok(builders
            .get_mut(id)
            .context
            .inference_flags
            .replace(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true))
    }
    fn restore_paramspec(
        &self,
        builders: &mut BuilderStore<'_, 'db, 'ast>,
        id: BuilderId,
        previous: bool,
    ) -> Result<(), Self::Error> {
        builders
            .get_mut(id)
            .context
            .inference_flags
            .set(InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, previous);
        Ok(())
    }
    fn permit_paramspec(
        &self,
        policy: arguments::ArgumentPolicy,
        expression: &ast::Expr,
    ) -> Result<bool, Self::Error> {
        Ok(matches!(policy, arguments::ArgumentPolicy::PermitParamSpec)
            && is_dotted_name(expression))
    }
    fn complete<'expr>(
        &self,
        invocation: &mut LocalInvocation<
            '_,
            'db,
            'ast,
            'expr,
            ConstraintSetBuilder<'db>,
            F::Target,
        >,
    ) -> Result<(), Self::Error> {
        invocation.complete();
        Ok(())
    }
}

fn run_result<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    work: Work<'db, '_>,
) -> Option<LocalResult<'db>> {
    let syntax = string_annotation::OrdinaryStorage::new();
    let mut invocation = LocalInvocation::new(builder);
    let Ok(result) = drive_sync(
        &mut Some(work),
        &mut invocation,
        LocalFacts,
        &OrdinaryLocalEffects::default(),
        &syntax,
    );
    result
}

fn run<'db>(builder: &mut TypeInferenceBuilder<'db, '_>, work: Work<'db, '_>) -> Type<'db> {
    let result = match run_result(builder, work) {
        Some(LocalResult::Type(ty)) => Some(ty),
        Some(LocalResult::Annotation(_)) | None => None,
    };
    // Every entry installs one operation, and every transition installs its successor or returns.
    result.expect("local inference returned without completing its type entry")
}

fn run_annotation<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    work: Work<'db, '_>,
) -> TypeAndQualifiers<'db> {
    let result = match run_result(builder, work) {
        Some(LocalResult::Annotation(annotation)) => Some(annotation),
        Some(LocalResult::Type(_)) | None => None,
    };
    result.expect("local inference returned without completing its annotation entry")
}

pub(super) fn annotation<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    annotation: &ast::Expr,
    deferred_state: DeferredExpressionState,
    policy: PEP613Policy,
) -> TypeAndQualifiers<'db> {
    run_annotation(
        builder,
        Work::AnnotationStart(BuilderId::ROOT, annotation, deferred_state, policy),
    )
}

pub(super) fn annotation_body<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    annotation: &ast::Expr,
    policy: PEP613Policy,
) -> TypeAndQualifiers<'db> {
    run_annotation(
        builder,
        Work::AnnotationBody(
            AnnotationRoot {
                builder: BuilderId::ROOT,
                annotation,
                saved: None,
            },
            policy,
        ),
    )
}

/// Infers a tuple value through the existing local invocation without storing its outer expression.
pub(super) fn tuple_value<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    tuple: &ast::ExprTuple,
    context: TypeContext<'db>,
) -> Type<'db> {
    run(
        builder,
        Work::StartTupleExpression(BuilderId::ROOT, tuple, context),
    )
}

pub(super) fn tuple_annotation<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    request: tuple_annotation::Request<'_>,
) -> Type<'db> {
    run(
        builder,
        Work::StartTupleAnnotation(BuilderId::ROOT, request),
    )
}

pub(super) fn callable_annotation<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    request: callable_annotation::Request<'_>,
) -> Type<'db> {
    run(
        builder,
        Work::StartCallableAnnotation(BuilderId::ROOT, request),
    )
}

pub(super) fn type_expression<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    mode: TypeExpressionMode,
) -> Type<'db> {
    type_expression_request(
        builder,
        TypeExpressionRequest::Expression { expression, mode },
    )
}

pub(super) fn type_expression_request<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    request: TypeExpressionRequest<'db, '_>,
) -> Type<'db> {
    run(builder, Work::TypeExpression(BuilderId::ROOT, request))
}

/// Infers and stores the parsed root, transferring its flags to the original string.
///
/// Returns the inferred type without storing that result on the original string;
/// the caller's enclosing expression-storage path is responsible for that result.
pub(super) fn string_type_expression<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    string: &ast::ExprStringLiteral,
) -> Type<'db> {
    run(builder, Work::StringAnnotation(BuilderId::ROOT, string))
}

pub(super) fn expression<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
    tcx: TypeContext<'db>,
    mode: ExpressionMode,
) -> Type<'db> {
    run(
        builder,
        Work::Expression(BuilderId::ROOT, expression, tcx, mode),
    )
}

pub(super) fn specialization<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    subscript: &ast::ExprSubscript,
    value_ty: Type<'db>,
    generic_context: GenericContext<'db>,
    specialize: &dyn Fn(&[Option<Type<'db>>]) -> Type<'db>,
) -> Type<'db> {
    let effects = OrdinaryLocalEffectsWithTarget {
        finalizer: CallbackSpecialization(specialize),
    };
    let syntax = string_annotation::OrdinaryStorage::new();
    let mut invocation = LocalInvocation::with_target(builder);
    let owner = invocation.owners.push_specialization(
        BuilderId::ROOT,
        specialization::Request {
            subscript,
            value_ty,
            generic_context,
            target: SpecializationTarget::Custom(()),
            protocol_guard: None,
        },
    );
    let Ok(result) = drive_sync(
        &mut Some(Work::Specialization(owner)),
        &mut invocation,
        LocalFacts,
        &effects,
        &syntax,
    );
    let result = match result {
        Some(LocalResult::Type(ty)) => Some(ty),
        Some(LocalResult::Annotation(_)) | None => None,
    };
    result.expect("local specialization returned without completing its type entry")
}

pub(super) fn class_specialization<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    subscript: &ast::ExprSubscript,
    value_ty: Type<'db>,
    class: StaticClassLiteral<'db>,
    generic_context: GenericContext<'db>,
) -> Type<'db> {
    run(
        builder,
        Work::StartSpecialization {
            builder: BuilderId::ROOT,
            subscript,
            value_ty,
            class,
            generic_context,
            kind: ClassSpecializationKind::ClassObject,
        },
    )
}

pub(super) fn callee<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    expression: &ast::Expr,
) -> Type<'db> {
    run(
        builder,
        Work::Callee(BuilderId::ROOT, expression, CalleeContinuation::Return),
    )
}

pub(super) fn call<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    call: &ast::ExprCall,
    ty: Option<Type<'db>>,
    tcx: TypeContext<'db>,
) -> Type<'db> {
    let work = match ty {
        Some(ty) => Work::Call(BuilderId::ROOT, call, ty, tcx),
        None => Work::Callee(BuilderId::ROOT, &call.func, CalleeContinuation::Call(call, tcx)),
    };
    run(builder, work)
}

pub(super) fn assignment_call<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    target: &ast::Expr,
    call: &ast::ExprCall,
    definition: Definition<'db>,
    context: TypeContext<'db>,
) -> Type<'db> {
    run(
        builder,
        Work::Callee(
            BuilderId::ROOT,
            &call.func,
            CalleeContinuation::Assignment {
                target,
                call,
                definition,
                context,
            },
        ),
    )
}

pub(super) fn legacy_typevar<'db>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    target: &ast::Expr,
    call: &ast::ExprCall,
    definition: Definition<'db>,
    known_class: KnownClass,
) -> Type<'db> {
    run(
        builder,
        Work::LegacyTypeVar(
            BuilderId::ROOT,
            legacy::new(target, call, definition, known_class),
        ),
    )
}

pub(super) fn prepare_arguments<'db, 'expr>(
    builder: &mut TypeInferenceBuilder<'db, '_>,
    source: &'expr ast::Arguments,
) -> CallArguments<'expr, 'db> {
    let mut builders = BuilderStore::new(builder);
    let Ok(mut preparation) = OrdinaryLocalEffects::default().prepare(BuilderId::ROOT, source);
    loop {
        let Ok(step) = OrdinaryLocalEffects::default().preparation_step(preparation, &mut builders);
        match step {
            PreparationStep::Continue(next) => preparation = next,
            PreparationStep::Complete(arguments) => {
                builders.complete();
                return arguments;
            }
            PreparationStep::Infer(splat, expression) => {
                let ty = self::expression(
                    builders.get_mut(BuilderId::ROOT),
                    expression,
                    TypeContext::default(),
                    ExpressionMode::GetOrInfer,
                );
                let Ok(next) = OrdinaryLocalEffects::default().resume_splat(splat, ty, &mut builders);
                preparation = next;
            }
        }
    }
}

pub(super) fn check_arguments<'db, 'ast, 'arg, 'call>(
    builder: &mut TypeInferenceBuilder<'db, 'ast>,
    ast_arguments: ArgumentsIter<'arg>,
    argument_types: &mut CallArguments<'call, 'db>,
    infer: &mut dyn FnMut(&mut TypeInferenceBuilder<'db, 'ast>, ArgExpr<'db, 'arg>) -> Type<'db>,
    bindings: &mut Bindings<'db>,
    tcx: TypeContext<'db>,
) -> Result<(), CallErrorKind> {
    let mut builders = BuilderStore::new(builder);
    let storage = arguments::BorrowedArguments {
        arguments: argument_types,
        bindings,
    };
    let mut state = arguments::State::new(
        BuilderId::ROOT,
        ast_arguments,
        storage,
        arguments::ArgumentPolicy::External,
        tcx,
    );
    loop {
        match arguments::advance(state, &mut builders) {
            arguments::Action::Continue(next) => state = next,
            arguments::Action::Complete { result, .. } => {
                builders.complete();
                return result;
            }
            arguments::Action::Infer {
                pending,
                builder,
                argument,
                ..
            } => {
                let ty = infer(builders.get_mut(builder), argument);
                state = arguments::resume(pending, ty, &mut builders);
            }
        }
    }
}
