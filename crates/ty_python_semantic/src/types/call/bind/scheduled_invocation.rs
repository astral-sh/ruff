//! Invocation completion through the ordinary overload checker and queued argument relations.

use std::cell::{Cell, RefCell};

use salsa::execution_probe::FieldRequest;

use super::effects::{self, BinderEffects, BinderLegacyEffect};
use super::{
    Argument, ArgumentTypeChecker, BinderComparison, BinderCondition, CallArguments,
    CallableBinding,
};
use crate::Db;
use crate::types::callable::CallableTypeKind;
use crate::types::callable::scheduled_probe::{Boundary, Router, run_with};
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::relation::scheduled_requests::RelationRequest;
use crate::types::signatures::ParametersKind;
use crate::types::typevar::TypeVarSet;
use crate::types::{CallableType, LiteralValueTypeKind, ProgramEnvironment, Type, TypeContext};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InvocationBoundary {
    Preparation,
    PreparationAllowance,
    PreparationCostOverflow,
    Legacy(BinderLegacyEffect),
    Relation(Boundary),
    NonterminalConstraints,
    TypeVarTupleInspection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArgumentDependency<'db> {
    pub(crate) source: Type<'db>,
    pub(crate) target: Type<'db>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ArgumentEvidence<'db> {
    pub(crate) dependency: ArgumentDependency<'db>,
    /// The assignability relation is unsatisfiable; the binder may still need a later check.
    pub(crate) is_never_assignable: bool,
}

pub(crate) enum InvocationCompletion<'db> {
    Complete(Box<CallableBinding<'db>>),
    Incomplete {
        boundary: Option<InvocationBoundary>,
        argument: Option<ArgumentDependency<'db>>,
    },
}

pub(crate) struct InvocationOutcome<'db> {
    pub(crate) completion: InvocationCompletion<'db>,
    pub(crate) completed_arguments: Box<[ArgumentEvidence<'db>]>,
    pub(crate) preparation_work: usize,
    pub(crate) scheduler_work: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct InvocationPolicy {
    pub(crate) preparation_allowance: usize,
    pub(crate) scheduler_allowance: usize,
    pub(crate) reverse_execution: bool,
    pub(crate) reverse_merge: bool,
}

#[derive(Default)]
struct Progress<'db> {
    completed: RefCell<Vec<ArgumentEvidence<'db>>>,
    pending: Cell<Option<ArgumentDependency<'db>>>,
    preparation_work: Cell<usize>,
    live_frames: Cell<usize>,
}

struct InvocationFrame<'a, 'db>(&'a Progress<'db>);

impl<'a, 'db> InvocationFrame<'a, 'db> {
    fn new(progress: &'a Progress<'db>) -> Self {
        progress.live_frames.set(progress.live_frames.get() + 1);
        Self(progress)
    }
}

impl Drop for InvocationFrame<'_, '_> {
    fn drop(&mut self) {
        self.0.live_frames.set(self.0.live_frames.get() - 1);
    }
}

struct Preparation<'a, 'db> {
    remaining: usize,
    progress: &'a Progress<'db>,
}

impl<'db> Preparation<'_, 'db> {
    fn charge(&mut self) -> Result<(), InvocationBoundary> {
        self.reserve(1)
    }

    fn reserve(&mut self, work: usize) -> Result<(), InvocationBoundary> {
        let Some(remaining) = self.remaining.checked_sub(work) else {
            return Err(InvocationBoundary::PreparationAllowance);
        };
        let used = self
            .progress
            .preparation_work
            .get()
            .checked_add(work)
            .ok_or(InvocationBoundary::PreparationCostOverflow)?;
        self.remaining = remaining;
        self.progress.preparation_work.set(used);
        Ok(())
    }

    fn stored_types(&mut self, db: &'db dyn Db, root: Type<'db>) -> Result<(), InvocationBoundary> {
        let mut pending = vec![root];
        while let Some(ty) = pending.pop() {
            self.charge()?;
            match ty {
                Type::LiteralValue(literal)
                    if matches!(literal.kind(), LiteralValueTypeKind::Enum(_)) =>
                {
                    return Err(InvocationBoundary::Preparation);
                }
                Type::Never
                | Type::Dynamic(_)
                | Type::LiteralValue(_)
                | Type::NominalInstance(_) => {}
                Type::Callable(callable) => {
                    if callable.kind(db) != CallableTypeKind::Regular {
                        return Err(InvocationBoundary::Preparation);
                    }
                    for signature in &callable.signatures(db).overloads {
                        self.charge()?;
                        if signature.definition().is_some()
                            || signature.generic_context.is_some()
                            || signature.receiver_constraints().is_some()
                            || !matches!(signature.parameters().kind(), ParametersKind::Standard)
                        {
                            return Err(InvocationBoundary::Preparation);
                        }
                        pending.push(signature.return_ty);
                        for parameter in signature.parameters() {
                            self.charge()?;
                            if !parameter.is_positional_only()
                                || parameter.has_default()
                                || parameter.has_starred_annotation()
                                || parameter.definition().is_some()
                            {
                                return Err(InvocationBoundary::Preparation);
                            }
                            pending.push(parameter.annotated_type());
                        }
                    }
                }
                _ => return Err(InvocationBoundary::Preparation),
            }
        }
        Ok(())
    }
}

fn matching_preparation_work(
    argument_count: usize,
    parameter_counts: impl IntoIterator<Item = usize>,
) -> Result<usize, InvocationBoundary> {
    let parameter_work = parameter_counts.into_iter().try_fold(0usize, |sum, count| {
        count
            .checked_add(1)
            .and_then(|count| sum.checked_add(count))
    });
    argument_count
        .checked_add(1)
        .and_then(|arguments| parameter_work?.checked_mul(arguments))
        .ok_or(InvocationBoundary::PreparationCostOverflow)
}

struct QueuedBinderEffects<'a, 'db, 'c> {
    router: &'a Router<'db, 'c>,
    constraints: &'c ConstraintSetBuilder<'db>,
    env: &'a ProgramEnvironment<'db>,
    progress: &'a Progress<'db>,
}

impl effects::sealed::Sealed for QueuedBinderEffects<'_, '_, '_> {}

impl<'db> BinderEffects<'db> for QueuedBinderEffects<'_, 'db, '_> {
    type Error = InvocationBoundary;

    fn recursion_guard(&self) -> Option<&CallableRecursionGuard<'db>> {
        None
    }

    async fn field<R: FieldRequest<'db>>(&self, _request: R) -> Result<R::Output, Self::Error> {
        Err(InvocationBoundary::Legacy(
            BinderLegacyEffect::KnownFunction,
        ))
    }

    fn legacy<T>(
        &self,
        effect: BinderLegacyEffect,
        _operation: impl FnOnce() -> T,
    ) -> Result<T, InvocationBoundary> {
        Err(InvocationBoundary::Legacy(effect))
    }

    fn inspect_argument_expansions(
        &self,
        arguments: &CallArguments<'_, 'db>,
        inspect: impl FnOnce() -> bool,
    ) -> Result<bool, InvocationBoundary> {
        // These cases return directly from the existing type expander. In particular, a stored
        // callable does not inspect its parameter or return types while checking for expansion.
        if arguments.iter().all(|(argument, types)| {
            matches!(argument, Argument::Positional)
                && matches!(
                    types.get_default(),
                    Some(
                        Type::Callable(_) | Type::Never | Type::Dynamic(_) | Type::LiteralValue(_)
                    )
                )
        }) {
            Ok(inspect())
        } else {
            Err(InvocationBoundary::Legacy(
                BinderLegacyEffect::ArgumentExpansion,
            ))
        }
    }

    async fn condition(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        constraints: &ConstraintSetBuilder<'db>,
        condition: BinderCondition<'db>,
    ) -> Result<bool, InvocationBoundary> {
        if !std::ptr::eq(constraints, self.constraints) || !std::ptr::eq(env, self.env) {
            return Err(InvocationBoundary::Relation(Boundary::ConstraintDomain));
        }
        let BinderCondition::Never(BinderComparison::Assignable {
            source,
            target,
            inferable_typevars: TypeVarSet::None,
        }) = condition
        else {
            return Err(InvocationBoundary::Preparation);
        };
        let dependency = ArgumentDependency { source, target };
        self.progress.pending.set(Some(dependency));
        let request = RelationRequest::binder_assignability(self.constraints, source, target)
            .map_err(InvocationBoundary::Relation)?;
        let result = self
            .router
            .consumer_relation_demand(request)
            .await
            .map_err(InvocationBoundary::Relation)?;
        let rejected = if result.constraints.is_trivially_never_satisfied() {
            true
        } else if result.constraints.is_trivially_always_satisfied() {
            false
        } else {
            return Err(InvocationBoundary::NonterminalConstraints);
        };
        self.progress.completed.borrow_mut().push(ArgumentEvidence {
            dependency,
            is_never_assignable: rejected,
        });
        self.progress.pending.set(None);
        Ok(rejected)
    }

    fn defer_typevartuple_check(
        &self,
        checker: &ArgumentTypeChecker<'_, 'db>,
        declared: Type<'db>,
        _expected: Type<'db>,
        _argument: Type<'db>,
    ) -> Result<bool, InvocationBoundary> {
        // An empty callback parameter list cannot contain a TypeVarTuple. No callable conversion
        // or generic inspection is needed to rule out the legacy deferral workaround here.
        match declared {
            Type::Callable(callable)
                if callable
                    .signatures(checker.db)
                    .overloads
                    .iter()
                    .all(|signature| signature.parameters().as_slice().is_empty()) =>
            {
                Ok(false)
            }
            Type::Never | Type::Dynamic(_) => Ok(false),
            Type::LiteralValue(literal)
                if !matches!(literal.kind(), LiteralValueTypeKind::Enum(_)) =>
            {
                Ok(false)
            }
            _ => Err(InvocationBoundary::TypeVarTupleInspection),
        }
    }

    fn trace_span(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _arguments: &CallArguments<'_, 'db>,
        _signature: Type<'db>,
    ) -> tracing::Span {
        // Formatting an arbitrary type can itself need semantic information.
        tracing::trace_span!("CallableBinding::check_types")
    }
}

#[expect(clippy::too_many_arguments)]
async fn evaluate<'db, 'c>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
    router: &Router<'db, 'c>,
    callable: CallableType<'db>,
    arguments: &[Type<'db>],
    preparation_allowance: usize,
    progress: &Progress<'db>,
) -> Result<Box<CallableBinding<'db>>, InvocationBoundary> {
    let _frame = InvocationFrame::new(progress);
    let mut preparation = Preparation {
        remaining: preparation_allowance,
        progress,
    };
    preparation.stored_types(db, Type::Callable(callable))?;
    for &argument in arguments {
        preparation.charge()?;
        preparation.stored_types(db, argument)?;
    }

    let matching_work = matching_preparation_work(
        arguments.len(),
        callable
            .signatures(db)
            .overloads
            .iter()
            .map(|signature| signature.parameters().len()),
    )?;
    preparation.reserve(matching_work)?;

    let arguments = CallArguments::positional(arguments.iter().copied());
    let mut binding = CallableBinding::from_overloads(
        Type::Callable(callable),
        callable.signatures(db).overloads.iter().cloned(),
    );
    binding.match_parameters(db, env, &arguments);
    let effects = QueuedBinderEffects {
        router,
        constraints,
        env,
        progress,
    };
    binding
        .check_types_with(
            db,
            env,
            constraints,
            &arguments,
            TypeContext::default(),
            &effects,
        )
        .await?;
    Ok(Box::new(binding))
}

/// Runs one invocation in the supplied session, returning only completed bindings and committed
/// argument evidence. A dropped or unsupported dependency never exposes a partial binding.
pub(crate) fn run_invocation<'db, 'c>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
    router: &Router<'db, 'c>,
    callable: CallableType<'db>,
    arguments: &[Type<'db>],
    policy: InvocationPolicy,
) -> Result<InvocationOutcome<'db>, Boundary> {
    let progress = Progress::default();
    let result = run_with(
        db,
        env,
        router,
        policy.scheduler_allowance,
        policy.reverse_execution,
        policy.reverse_merge,
        |router| {
            evaluate(
                db,
                env,
                constraints,
                router,
                callable,
                arguments,
                policy.preparation_allowance,
                &progress,
            )
        },
    )?;
    assert_eq!(progress.live_frames.get(), 0);
    let scheduler_work = result.work();
    let completion = match result.consumer {
        Some(Ok(binding)) => InvocationCompletion::Complete(binding),
        result => InvocationCompletion::Incomplete {
            boundary: result.and_then(Result::err),
            argument: progress.pending.get(),
        },
    };
    Ok(InvocationOutcome {
        completion,
        completed_arguments: progress.completed.into_inner().into_boxed_slice(),
        preparation_work: progress.preparation_work.get(),
        scheduler_work,
    })
}

#[cfg(test)]
mod tests;
