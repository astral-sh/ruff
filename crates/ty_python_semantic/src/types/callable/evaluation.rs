//! Drives callable and constructor continuations without recursively entering each other.

use std::borrow::Cow;

use super::conversion::{self, ConversionStep, PendingConversion};
use super::{CallableConversionRequest, CallableType, CallableTypes};
use crate::types::constructor::callable::{ConstructorCallableStep, PendingConstructorConversion};
use crate::types::cyclic::{
    CallableEntry, CallableExpansion, CallableRecursionGuard, CallableVisitScope,
    ConstructorCacheScope, ConstructorEntry, DescriptorDispatchScope,
};
use crate::types::{ClassType, DescriptorOrigin, Signature, Type};
use crate::{Db, ProgramEnvironment};

pub(super) fn conversion<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    request: CallableConversionRequest<'db>,
    recursion_guard: Option<&CallableRecursionGuard<'db>>,
) -> Option<CallableTypes<'db>> {
    // The caller has already entered the root's recursion guard. Child conversions enter
    // their guards here and leave them before their parent's continuation resumes.
    let mut suspended = Suspended::default();
    let mut context = EvaluationContext::new(Cow::Borrowed(env));
    let mut step = EvaluationStep::Conversion(conversion::start(
        db,
        env,
        request,
        recursion_guard.is_some(),
    ));

    loop {
        step = match step {
            EvaluationStep::Conversion(ConversionStep::Convert(pending)) => {
                let request = pending.request;
                let child = context.child(recursion_guard, pending.origin);
                suspended
                    .0
                    .push(SuspendedFrame::Conversion { pending, context });
                context = child;
                start_child(db, request, recursion_guard, &mut context)
            }
            EvaluationStep::Conversion(ConversionStep::Constructor { class, receiver }) => {
                let Some(guard) = recursion_guard else {
                    step = EvaluationStep::Conversion(ConversionStep::Complete(Some(
                        class.into_callable_with_receiver(db, receiver),
                    )));
                    continue;
                };
                match guard.enter_constructor(class, receiver) {
                    ConstructorEntry::SharedQuery => {
                        EvaluationStep::Conversion(ConversionStep::Complete(Some(
                            class.into_callable_with_receiver(db, receiver),
                        )))
                    }
                    ConstructorEntry::Ready(callables) => {
                        EvaluationStep::Conversion(ConversionStep::Complete(Some(callables)))
                    }
                    ConstructorEntry::Expand(scope) => {
                        context.constructor = Some(scope);
                        context.env = Cow::Owned(ProgramEnvironment::from_file(
                            class.class_literal(db).program_file(db),
                        ));
                        EvaluationStep::Constructor(
                            ConstructorCallableStep::start(db, &context.env, class, receiver),
                            guard,
                        )
                    }
                }
            }
            EvaluationStep::Conversion(ConversionStep::CallMember(pending)) => {
                EvaluationStep::Conversion(pending.evaluate(db, &context.env, recursion_guard))
            }
            EvaluationStep::Conversion(ConversionStep::CachedBoundMethod(method)) => {
                EvaluationStep::Conversion(ConversionStep::Complete(method.callables(db).cloned()))
            }
            EvaluationStep::Conversion(ConversionStep::Complete(callables)) => {
                context.close();
                let Some(parent) = suspended.0.pop() else {
                    return callables;
                };
                (context, step) = parent.resume(db, callables);
                continue;
            }
            EvaluationStep::Constructor(ConstructorCallableStep::Convert(pending), guard) => {
                let request = pending.request();
                let child = context.child(Some(guard), Some(pending.origin()));
                suspended.0.push(SuspendedFrame::Constructor {
                    pending,
                    context,
                    guard,
                });
                context = child;
                start_child(db, request, Some(guard), &mut context)
            }
            EvaluationStep::Constructor(ConstructorCallableStep::Complete(callables), _) => {
                if let Some(scope) = context.constructor.take() {
                    scope.finish(&callables);
                }
                EvaluationStep::Conversion(ConversionStep::Complete(Some(callables)))
            }
            EvaluationStep::Constructor(ConstructorCallableStep::Member(pending), guard) => {
                EvaluationStep::Constructor(pending.evaluate(db, &context.env, guard), guard)
            }
            EvaluationStep::Constructor(ConstructorCallableStep::Lookup(pending), guard) => {
                EvaluationStep::Constructor(pending.evaluate(db, &context.env, guard), guard)
            }
            EvaluationStep::Constructor(
                ConstructorCallableStep::BindInitializer(pending),
                guard,
            ) => EvaluationStep::Constructor(pending.evaluate(db, &context.env, guard), guard),
            EvaluationStep::Constructor(
                ConstructorCallableStep::CheckNewReturn(pending),
                guard,
            ) => EvaluationStep::Constructor(pending.evaluate(db, &context.env, guard), guard),
        };
    }
}

/// A constructor root has a nonoptional result; its nested conversions use the shared driver.
pub(in crate::types) fn constructor_callables<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    class: ClassType<'db>,
    receiver: Type<'db>,
    guard: &CallableRecursionGuard<'db>,
) -> CallableTypes<'db> {
    let mut step = ConstructorCallableStep::start(db, env, class, receiver);
    loop {
        step = match step {
            ConstructorCallableStep::Convert(pending) => {
                let callables = guard.with_dependency(db, pending.origin(), || {
                    pending.request().evaluate(db, env, Some(guard))
                });
                pending.resume(db, env, callables)
            }
            ConstructorCallableStep::Member(pending) => pending.evaluate(db, env, guard),
            ConstructorCallableStep::Lookup(pending) => pending.evaluate(db, env, guard),
            ConstructorCallableStep::BindInitializer(pending) => pending.evaluate(db, env, guard),
            ConstructorCallableStep::CheckNewReturn(pending) => pending.evaluate(db, env, guard),
            ConstructorCallableStep::Complete(callables) => return callables,
        };
    }
}

fn start_child<'a, 'db>(
    db: &'db dyn Db,
    request: CallableConversionRequest<'db>,
    guard: Option<&'a CallableRecursionGuard<'db>>,
    context: &mut EvaluationContext<'a, 'db>,
) -> EvaluationStep<'a, 'db> {
    if let Some(guard) = guard {
        match guard.enter(db, &context.env, (CallableExpansion::Upcast, request.ty)) {
            CallableEntry::Entered(scope) => context.visit = Some(scope),
            CallableEntry::ExactCycle => {
                return EvaluationStep::Conversion(ConversionStep::Complete(Some(
                    CallableTypes::one(CallableType::bottom(db)),
                )));
            }
            CallableEntry::Growth => {
                return EvaluationStep::Conversion(ConversionStep::Complete(Some(
                    CallableTypes::one(CallableType::single(db, Signature::recursion_recovery())),
                )));
            }
        }
    } else if matches!(
        request.ty,
        Type::NominalInstance(_) | Type::ProtocolInstance(_)
    ) {
        // This boundary creates a fresh guard. Keep that query/guard boundary while its
        // dependencies still use the existing recursion and caching policy.
        return EvaluationStep::Conversion(ConversionStep::Complete(request.evaluate(
            db,
            &context.env,
            None,
        )));
    }
    EvaluationStep::Conversion(conversion::start(
        db,
        &context.env,
        request,
        guard.is_some(),
    ))
}

enum EvaluationStep<'a, 'db> {
    Conversion(ConversionStep<'db>),
    Constructor(
        ConstructorCallableStep<'db>,
        &'a CallableRecursionGuard<'db>,
    ),
}

struct EvaluationContext<'a, 'db> {
    env: Cow<'a, ProgramEnvironment<'db>>,
    constructor: Option<ConstructorCacheScope<'a, 'db>>,
    visit: Option<CallableVisitScope<'a, 'db>>,
    dependency: Option<DescriptorDispatchScope<'a, 'db>>,
}

impl<'a, 'db> EvaluationContext<'a, 'db> {
    fn new(env: Cow<'a, ProgramEnvironment<'db>>) -> Self {
        Self {
            env,
            constructor: None,
            visit: None,
            dependency: None,
        }
    }

    fn child(
        &self,
        guard: Option<&'a CallableRecursionGuard<'db>>,
        origin: Option<DescriptorOrigin<'db>>,
    ) -> Self {
        let mut child = Self::new(self.env.clone());
        if let (Some(guard), Some(origin)) = (guard, origin) {
            child.dependency = Some(guard.enter_dependency(origin));
        }
        child
    }

    fn close(&mut self) {
        // Constructor dependencies must be merged while the enclosing conversion is still
        // active. Restore descriptor dispatch last, before resuming its caller.
        drop(self.constructor.take());
        drop(self.visit.take());
        drop(self.dependency.take());
    }
}

impl Drop for EvaluationContext<'_, '_> {
    fn drop(&mut self) {
        self.close();
    }
}

enum SuspendedFrame<'a, 'db> {
    Conversion {
        pending: PendingConversion<'db>,
        context: EvaluationContext<'a, 'db>,
    },
    Constructor {
        pending: PendingConstructorConversion<'db>,
        context: EvaluationContext<'a, 'db>,
        guard: &'a CallableRecursionGuard<'db>,
    },
}

impl<'a, 'db> SuspendedFrame<'a, 'db> {
    fn resume(
        self,
        db: &'db dyn Db,
        callables: Option<CallableTypes<'db>>,
    ) -> (EvaluationContext<'a, 'db>, EvaluationStep<'a, 'db>) {
        match self {
            Self::Conversion { pending, context } => {
                let step = pending.resume(db, &context.env, callables);
                (context, EvaluationStep::Conversion(step))
            }
            Self::Constructor {
                pending,
                context,
                guard,
            } => {
                let step = pending.resume(db, &context.env, callables);
                (context, EvaluationStep::Constructor(step, guard))
            }
        }
    }
}

#[derive(Default)]
struct Suspended<'a, 'db>(Vec<SuspendedFrame<'a, 'db>>);

impl Drop for Suspended<'_, '_> {
    fn drop(&mut self) {
        // Restoring an outer scope before an inner one would corrupt the active dependency
        // sets if a synchronous query unwound. Vec's element drop order is not stack order.
        while self.0.pop().is_some() {}
    }
}
