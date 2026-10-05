use std::future::ready;

#[cfg(test)]
use super::frame_free::trace_owner;
use super::frame_free::{
    CallbackScope, Caller, FrameFreeRequest, OwnedFrameFree, admit_phase, enter_operation,
};
use super::registration::TaskEndpoint;
use super::{ExecutionProvider, RunError, RunOperation, RunResult, callback, execute_owned};
use crate::function::maybe_changed_after::validation::{
    PendingVerificationEvent, Validation, ValidationStep, Verification, VerificationAction,
    VerifiedMemo,
};
use crate::function::maybe_changed_after::{
    EagerValidationVerification, ValidationProbe, VerifyResult,
};
use crate::function::memo::MemoOutputCheck;
use crate::function::{ClaimResult, Configuration, IngredientImpl};
use crate::{Id, Revision};

impl<C: Configuration> FrameFreeRequest for ValidationStep<'_, C> {
    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        let (phase, key) = match self {
            Self::Probe(validation) => (
                "probe",
                Some(validation.ingredient.database_key_index(validation.id())),
            ),
            Self::Claim(validation) => (
                "claim",
                Some(validation.ingredient.database_key_index(validation.id())),
            ),
            Self::Reload(_, claim) => ("reload", Some(claim.database_key_index())),
            Self::Verify(validation, verification) => (
                verification.phase_name(),
                Some(validation.ingredient.database_key_index(validation.id())),
            ),
            Self::Execute(request) => ("execute", Some(request.claim.database_key_index())),
            Self::Participant(_, request) => ("participant", request.key()),
            Self::Wait(validation, _) => (
                "wait",
                Some(validation.ingredient.database_key_index(validation.id())),
            ),
            Self::Complete(_) => ("validation.complete", None),
        };
        trace_owner(phase, key, operation, caller);
    }

    fn abort(self) {
        match self {
            Self::Reload(_, claim) => claim.abort(),
            Self::Verify(_, verification) => verification.into_claim().abort(),
            Self::Execute(request) => request.claim.abort(),
            Self::Participant(_, request) => request.abort(),
            Self::Wait(_, running) => drop(running),
            Self::Probe(_) | Self::Claim(_) | Self::Complete(_) => {}
        }
    }
}

impl FrameFreeRequest for Verification<'_> {
    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        trace_owner(self.phase_name(), Some(self.owner()), operation, caller);
    }

    fn abort(self) {
        self.into_claim().abort();
    }
}

impl<C: Configuration> FrameFreeRequest for Validation<'_, C> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        trace_owner(
            "probe",
            Some(self.ingredient.database_key_index(self.id())),
            operation,
            caller,
        );
    }
}

impl FrameFreeRequest for EagerValidationVerification<'_> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        trace_owner(
            "validation.verification.event",
            Some(self.key()),
            operation,
            caller,
        );
    }
}

async fn accept_event<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    caller: Caller,
    scope: &CallbackScope<'_, 'db>,
    event: PendingVerificationEvent<'_, 'db>,
) {
    callback::complete(
        &endpoint.inner,
        scope,
        callback::CallbackKind::Canonical,
        || {
            event.event();
            ready(caller.check_resume(operation))
        },
    )
    .await;
    event.finish();
}

async fn verify_eager<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    caller: Caller,
    verification: EagerValidationVerification<'db>,
) -> VerifyResult {
    let owned = OwnedFrameFree::new(verification, operation, caller);
    let request = match owned.admitted() {
        Ok(request) => request,
        Err(error) => match callback::reject(&endpoint.inner, error, owned).await {},
    };
    let check = if operation.policy.declared == crate::attempt_probe::QueryPolicy::CompleteOnly {
        MemoOutputCheck::Controlled
    } else {
        MemoOutputCheck::OutputsOnly
    };
    let units = request.output_check_work(check);
    if units != 0 {
        callback::complete(
            &endpoint.inner,
            &owned,
            callback::CallbackKind::Canonical,
            || ready(endpoint.admit_work(units)),
        )
        .await;
    }
    let (empty, error) = match check {
        MemoOutputCheck::Controlled => (
            request.controlled_outputs_are_empty(),
            "controlled callable requires a memo without outputs or direct accumulators",
        ),
        MemoOutputCheck::OutputsOnly => (
            request.outputs_are_empty(),
            "controlled verification requires a memo without outputs",
        ),
    };
    if !empty {
        match callback::reject(
            &endpoint.inner,
            RunError::Contract(error),
            owned,
        )
        .await {}
    }
    callback::complete(
        &endpoint.inner,
        &owned,
        callback::CallbackKind::Canonical,
        || {
            request.event();
            ready(caller.check_resume(operation))
        },
    )
    .await;
    match owned.take_admitted() {
        Ok(request) => request.finish(),
        Err((error, owned)) => match callback::reject(&endpoint.inner, error, owned).await {},
    }
}

/// The outer loop has admitted and checked the first phase before entering this fixed layer.
pub(super) async fn verify_admitted<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    caller: Caller,
    verification: Verification<'db>,
) -> RunResult<VerifiedMemo<'db>> {
    let mut owned = OwnedFrameFree::new(verification, operation, caller);
    let request = match owned.admitted() {
        Ok(verification) => verification,
        Err(error) => match callback::reject(&endpoint.inner, error, owned).await {},
    };
    let check = if operation.policy.declared == crate::attempt_probe::QueryPolicy::CompleteOnly {
        MemoOutputCheck::Controlled
    } else {
        MemoOutputCheck::OutputsOnly
    };
    let units = request.output_check_work(check);
    if units != 0 {
        callback::complete(
            &endpoint.inner,
            &owned,
            callback::CallbackKind::Canonical,
            || ready(endpoint.admit_work(units)),
        )
        .await;
    }
    let (empty, error) = match check {
        MemoOutputCheck::Controlled => (
            request.controlled_outputs_are_empty(),
            "controlled callable requires a memo without outputs or direct accumulators",
        ),
        MemoOutputCheck::OutputsOnly => (
            request.outputs_are_empty(),
            "controlled verification requires a memo without outputs",
        ),
    };
    if !empty {
        match callback::reject(
            &endpoint.inner,
            RunError::Contract(error),
            owned,
        )
        .await {}
    }
    loop {
        {
            let (verification, scope) = match owned.split() {
                Ok(parts) => parts,
                Err(error) => match callback::reject(&endpoint.inner, error, owned).await {},
            };
            match verification.action() {
                VerificationAction::Work(work) => {
                    let (units, bytes) = match work.provisional_work() {
                        Some(quote) => quote,
                        None => match callback::reject(
                            &endpoint.inner,
                            super::RunError::Contract("provisional snapshot work overflow"),
                            scope,
                        )
                        .await {},
                    };
                    if units != 0 {
                        callback::complete(
                            &endpoint.inner,
                            &scope,
                            callback::CallbackKind::Canonical,
                            || {
                                ready(
                                    endpoint
                                        .inner
                                        .admit(super::ExecutionWork::Resource {
                                            requested_bytes: bytes,
                                        })
                                        .and_then(|()| {
                                            crate::attempt_probe::charge(
                                                endpoint.inner.context.db,
                                                units,
                                            )
                                            .map_err(super::RunError::Refused)
                                        })
                                        .and_then(|()| {
                                            endpoint
                                                .inner
                                                .admit(super::ExecutionWork::Work { units })
                                        }),
                                )
                            },
                        )
                        .await;
                    }
                    work.advance();
                }

                VerificationAction::Event(event) => {
                    accept_event(endpoint, operation, caller, &scope, event).await;
                }
                VerificationAction::Dependency { request, reply } => {
                    let result = callback::complete(
                        &endpoint.inner,
                        &scope,
                        callback::CallbackKind::Canonical,
                        || async {
                            let result = endpoint
                                .validate(request.key, request.changed_after)?
                                .await?;
                            caller.check_resume(operation)?;
                            Ok(result)
                        },
                    )
                    .await;
                    #[cfg(test)]
                    trace_owner("dependency.resume", Some(reply.owner()), operation, caller);
                    reply.resume(result);
                }
                VerificationAction::Complete => {
                    return match owned.complete_admitted() {
                        Ok(verified) => Ok(verified),
                        Err((error, owned)) => {
                            match callback::reject(&endpoint.inner, error, owned).await {}
                        }
                    };
                }
            }

            // The event and stamp belong to the local phase that selected them, so they do not
            // introduce another work admission between that phase and its completion.
            if let Some(event) = verification.pending_event() {
                accept_event(endpoint, operation, caller, &scope, event).await;
            }
        }

        // Every subsequent phase has the same admission and ownership checks as the outer loop.
        admit_phase(endpoint, &owned).await;
        #[cfg(test)]
        owned.trace();
    }
}

pub(super) async fn validate<'run, 'db: 'run, C, P>(
    endpoint: TaskEndpoint<'run, 'db>,
    ingredient: &'db IngredientImpl<C>,
    db: &'db C::DbView,
    id: Id,
    revision: Revision,
    provider: P,
) -> RunResult<VerifyResult>
where
    C: Configuration,
    P: ExecutionProvider<'run, 'db, C>,
{
    let (operation, caller) = enter_operation::<C>(&endpoint, provider.operation_policy()).await;
    let scope = CallbackScope::new(&operation, caller);
    let validation = callback::complete(
        &endpoint.inner,
        &scope,
        callback::CallbackKind::Canonical,
        || ready(Ok(Validation::new(ingredient, db, id, revision))),
    )
    .await;
    let mut owned = OwnedFrameFree::new(ValidationStep::Probe(validation), &operation, caller);

    loop {
        // Every edge and metadata phase is admitted while its real claim is still guarded.
        admit_phase(&endpoint, &owned).await;
        let current = match owned.take_admitted() {
            Ok(step) => step,
            Err((error, owned)) => match callback::reject(&endpoint.inner, error, owned).await {},
        };
        let step = match current {
            ValidationStep::Probe(mut validation) => {
                let memo = validation.lookup_memo();
                if operation.policy.declared == crate::attempt_probe::QueryPolicy::CompleteOnly
                    && let Some(memo) = memo
                {
                    let units = memo.header.output_check_work(MemoOutputCheck::Controlled);
                    if units != 0 {
                        let selected = OwnedFrameFree::new(validation, &operation, caller);
                        callback::complete(
                            &endpoint.inner,
                            &selected,
                            callback::CallbackKind::Canonical,
                            || ready(endpoint.admit_work(units)),
                        )
                        .await;
                        validation = match selected.into_admitted() {
                            Ok(validation) => validation,
                            Err((error, selected)) => {
                                match callback::reject(&endpoint.inner, error, selected).await {}
                            }
                        };
                    }
                    if !memo.header.controlled_outputs_are_empty() {
                        match callback::reject(
                            &endpoint.inner,
                            RunError::Contract("controlled callable requires a memo without outputs or direct accumulators"),
                            validation,
                        ).await {}
                    }
                }
                match validation.probe_memo(memo) {
                    ValidationProbe::Ready(result) => ValidationStep::Complete(result),
                    ValidationProbe::Miss => ValidationStep::Claim(validation),
                    ValidationProbe::Verify(verification) => ValidationStep::Complete(
                        verify_eager(&endpoint, &operation, caller, verification).await,
                    ),
                }
            }
            ValidationStep::Claim(validation) => match validation.try_claim() {
                ClaimResult::Claimed(claim) => ValidationStep::Reload(validation, claim),
                ClaimResult::Running(running) => {
                    #[cfg(test)]
                    let key = validation.ingredient.database_key_index(validation.id());
                    let waiting =
                        OwnedFrameFree::new(ValidationStep::Probe(validation), &operation, caller);
                    #[cfg(test)]
                    trace_owner("validation.wait", Some(key), &operation, caller);
                    super::frame_free::wait_for_query(&endpoint, &waiting, running).await;
                    owned = match waiting.into_admitted() {
                        Ok(step) => OwnedFrameFree::new(step, &operation, caller),
                        Err((error, waiting)) => {
                            match callback::reject(&endpoint.inner, error, waiting).await {}
                        }
                    };
                    continue;
                }
                ClaimResult::Cycle { .. } => {
                    let result = callback::complete(
                        &endpoint.inner,
                        &scope,
                        callback::CallbackKind::Canonical,
                        || ready(Ok(validation.cold_cycle())),
                    )
                    .await;
                    ValidationStep::Complete(result)
                }
            },
            ValidationStep::Reload(validation, claim) => {
                #[cfg(test)]
                super::tests::observation::record(super::tests::observation::Event::Claim {
                    key: claim.database_key_index(),
                    serial: claim.test_serial(),
                    operation: operation.ordinal,
                });
                validation.reload(claim)
            }
            ValidationStep::Verify(validation, verification) => {
                let verified =
                    match verify_admitted(&endpoint, &operation, caller, verification).await {
                        Ok(verified) => verified,
                        Err(error) => {
                            match callback::reject(&endpoint.inner, error, validation).await {}
                        }
                    };
                validation.verified(verified)
            }
            ValidationStep::Execute(request) => {
                // No fallible boundary separates taking this claim from installing execution's
                // owning frame guards. The original operation stays installed through comparison.
                let validation = request.validation;
                let memo = match execute_owned(
                    endpoint.inner.clone(),
                    &operation,
                    validation.ingredient,
                    validation.db,
                    request.claim,
                    Some(request.old_memo),
                    super::super::participant::Consumer::Validation,
                    &provider,
                )
                .await
                {
                    Ok(memo) => memo,
                    Err(error) => {
                        match callback::reject(&endpoint.inner, error, validation).await {}
                    }
                };
                validation.executed(memo)
            }
            ValidationStep::Participant(validation, participant) => {
                match super::retire_participant_owned(&endpoint.inner, &operation, participant)
                    .await
                {
                    super::super::participant::ParticipantProgress::Complete(memo) => {
                        validation.executed(memo)
                    }
                    super::super::participant::ParticipantProgress::Pending(request) => {
                        ValidationStep::Participant(validation, request)
                    }
                    super::super::participant::ParticipantProgress::Execute(request) => {
                        let memo = match super::execute_request_owned(
                            endpoint.inner.clone(),
                            &operation,
                            request,
                            &provider,
                        )
                        .await
                        {
                            Ok(memo) => memo,
                            Err(error) => {
                                match callback::reject(&endpoint.inner, error, validation).await {}
                            }
                        };
                        validation.executed(memo)
                    }
                }
            }
            ValidationStep::Complete(result) => {
                let result = callback::complete(
                    &endpoint.inner,
                    &scope,
                    callback::CallbackKind::Canonical,
                    || ready(endpoint.inner.context.observe(Ok(result))),
                )
                .await;
                return Ok(result);
            }
            ValidationStep::Wait(validation, running) => {
                drop(running);
                match callback::reject(
                    &endpoint.inner,
                    RunError::Contract("validation wait escaped immediate handling"),
                    validation,
                )
                .await {}
            }
        };
        owned = OwnedFrameFree::new(step, &operation, caller);
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{Caller, OwnedFrameFree};
    use crate::attempt_probe::{AttemptOutcome, Incomplete, try_with_attempt};
    use crate::function::execute::execution_run::{RunContext, RunError, RunOperation, RunResult};
    use crate::function::maybe_changed_after::validation::ClaimedMemo;
    use crate::function::{ClaimResult, Configuration, IngredientImpl, Reentrancy};
    use crate::plumbing::AsId;
    use crate::zalsa::ZalsaDatabase;
    use crate::{Database, DatabaseImpl, Id};

    #[crate::input]
    struct Input {
        value: u32,
    }

    #[crate::tracked(returns(copy), attempt = ReturnOnly)]
    fn value(db: &dyn Database, input: Input) -> u32 {
        *input.value(db)
    }

    fn premature<C: Configuration>(
        db: &dyn Database,
        ingredient: &IngredientImpl<C>,
        id: Id,
    ) -> RunResult<()> {
        let context = Rc::new(RunContext::new(db)?);
        let operation = RunOperation::enter::<C>(context.clone())?;
        let caller = Caller::capture(&operation)?;
        let claim = match ingredient.sync_table.try_claim(
            db.zalsa(),
            db.zalsa_local(),
            id,
            Reentrancy::Deny,
        ) {
            ClaimResult::Claimed(claim) => claim,
            _ => panic!("fixture memo must be unclaimed"),
        };
        let serial = claim.test_serial();
        let memo = ingredient
            .memo_slot(
                db.zalsa(),
                id,
                ingredient.memo_ingredient_index(db.zalsa(), id),
            )
            .get_erased()
            .unwrap();
        let owned = OwnedFrameFree::new(
            ClaimedMemo { claim, memo }.verify(C::CYCLE_STRATEGY),
            &operation,
            caller,
        );
        caller.check_resume(&operation)?;
        let owned = match owned.complete_admitted() {
            Err((RunError::Contract("verification has not completed"), owned)) => owned,
            _ => panic!("premature verification must retain its owner"),
        };
        assert_eq!(context.reason.get(), None);
        drop(owned);
        assert_eq!(context.reason.get(), Some(Incomplete::Interrupted));
        assert!(caller.is_current(&operation));
        let claim = match ingredient.sync_table.try_claim(
            db.zalsa(),
            db.zalsa_local(),
            id,
            Reentrancy::Deny,
        ) {
            ClaimResult::Claimed(claim) => claim,
            _ => panic!("failed checked completion must abort its original claim"),
        };
        assert_ne!(claim.test_serial(), serial);
        claim.abort();
        Ok(())
    }

    #[test]
    fn premature_checked_completion_marks_before_releasing_the_claim() {
        let db = DatabaseImpl::default();
        let input = Input::new(&db, 7);
        assert_eq!(value(&db, input), 7);
        assert!(matches!(
            try_with_attempt(&db, 1_000, || premature(
                &db,
                value::fn_ingredient_(&db, db.zalsa()),
                input.as_id()
            )),
            Ok(AttemptOutcome::Incomplete(Incomplete::Interrupted))
        ));
        assert_eq!(value(&db, input), 7);
    }
}
