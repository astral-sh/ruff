#[cfg(test)]
use super::frame_free::trace_owner;
use super::frame_free::{
    CallbackScope, Caller, FrameFreeRequest, OwnedFrameFree, admit_phase, enter_operation,
};
use super::registration::TaskEndpoint;
use super::validation_run::verify_admitted;
use super::{
    ExecutionProvider, ExecutionWork, RunContext, RunError, RunOperation, RunResult, callback,
    execute_owned,
};
use crate::Id;
use crate::function::fetch::selection::{
    ColdCycleInitial, ColdCycleReady, PreparedColdCycleReady, Refresh, SelectionStep,
};
use crate::function::fetch::{EagerFetchVerification, FetchProbe};
use crate::function::memo::{MemoOutputCheck, SelectedMemo};
use crate::function::{Configuration, IngredientImpl};
use crate::zalsa::ZalsaDatabase;

#[cfg(test)]
mod tests;

impl<C: Configuration> FrameFreeRequest for EagerFetchVerification<'_, C> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        trace_owner(
            "fetch.verification.event",
            Some(self.key()),
            operation,
            caller,
        );
    }
}

async fn verify_eager<'run, 'db: 'run, C: Configuration>(
    endpoint: &TaskEndpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    caller: Caller,
    verification: EagerFetchVerification<'db, C>,
) -> SelectedMemo<'db, C> {
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
        callback::complete_immediate(
            &endpoint.inner,
            &owned,
            || endpoint.admit_work(units),
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
    callback::complete_immediate(
        &endpoint.inner,
        &owned,
        || {
            request.event();
            caller.check_resume(operation)
        },
    )
    .await;
    match owned.take_admitted() {
        Ok(request) => request.finish(),
        Err((error, owned)) => match callback::reject(&endpoint.inner, error, owned).await {},
    }
}

impl<C: Configuration> FrameFreeRequest for SelectedMemo<'_, C> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, _operation: &RunOperation<'_>, _caller: Caller) {
        // Selection already emitted its trace before transferring this guarded handle.
    }
}

impl<C: Configuration> FrameFreeRequest for SelectionStep<'_, C> {
    fn abort(self) {
        match self {
            Self::Reload(_, claim) => claim.abort(),
            Self::Verify(_, verification) => verification.into_claim().abort(),
            Self::Execute(request) => request.claim.abort(),
            Self::Participant(_, request) => request.abort(),
            Self::Wait(_, running) => drop(running),
            // A returned initial value must drop after the owner has marked refusal.
            Self::ColdInitialReady(request) => drop(request),
            Self::Probe(_)
            | Self::Claim(_)
            | Self::CycleProbe(_)
            | Self::ColdInitial(_)
            | Self::Selected(_) => {}
        }
    }

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        let (phase, key) = match self {
            Self::Probe(refresh) => (
                "fetch.probe",
                Some(refresh.ingredient.database_key_index(refresh.id)),
            ),
            Self::Claim(refresh) => (
                "fetch.claim",
                Some(refresh.ingredient.database_key_index(refresh.id)),
            ),
            Self::Reload(_, claim) => ("fetch.reload", Some(claim.database_key_index())),
            Self::Verify(_, verification) => {
                (verification.phase_name(), Some(verification.owner()))
            }
            Self::Execute(request) => ("fetch.execute", Some(request.claim.database_key_index())),
            Self::Participant(_, request) => ("participant", request.key()),
            Self::Wait(refresh, _) => (
                "fetch.wait",
                Some(refresh.ingredient.database_key_index(refresh.id)),
            ),
            Self::CycleProbe(refresh) => (
                "fetch.cycle",
                Some(refresh.ingredient.database_key_index(refresh.id)),
            ),
            Self::ColdInitial(request) => (
                "fetch.initial",
                Some(
                    request
                        .refresh
                        .ingredient
                        .database_key_index(request.refresh.id),
                ),
            ),
            Self::ColdInitialReady(_) => ("fetch.initial.ready", None),
            Self::Selected(_) => ("fetch.selected", None),
        };
        trace_owner(phase, key, operation, caller);
    }
}

impl<C: Configuration> FrameFreeRequest for ColdCycleInitial<'_, C> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        trace_owner(
            "fetch.initial.callback",
            Some(self.refresh.ingredient.database_key_index(self.refresh.id)),
            operation,
            caller,
        );
    }
}

impl<C: Configuration> FrameFreeRequest for ColdCycleReady<'_, C> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        trace_owner("fetch.initial.ready", None, operation, caller);
    }
}

impl<C: Configuration> FrameFreeRequest for PreparedColdCycleReady<'_, C> {
    fn abort(self) {}

    #[cfg(test)]
    fn trace(&self, operation: &RunOperation<'_>, caller: Caller) {
        trace_owner("fetch.initial.commit", None, operation, caller);
    }
}

async fn insert_cold_ready<'run, 'db: 'run, C: Configuration>(
    endpoint: &TaskEndpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    caller: Caller,
    owned: OwnedFrameFree<'_, 'db, ColdCycleReady<'db, C>>,
) -> SelectionStep<'db, C> {
    let request = match owned.admitted() {
        Ok(request) => request,
        Err(error) => match callback::reject(&endpoint.inner, error, owned).await {},
    };
    callback::complete_immediate(
        &endpoint.inner,
        &owned,
        || {
            request
                .storage_bytes()
                .ok_or(RunError::Contract("memo preparation size overflow"))
                .and_then(|requested_bytes| {
                    endpoint
                        .inner
                        .admit(ExecutionWork::Resource { requested_bytes })
                })
        },
    )
    .await;
    let request = match owned.take_admitted() {
        Ok(request) => request,
        Err((error, owned)) => match callback::reject(&endpoint.inner, error, owned).await {},
    };
    let request = match request.prepare() {
        Ok(request) => request,
        Err((error, request)) => {
            let owned = OwnedFrameFree::new(request, operation, caller);
            match callback::reject(&endpoint.inner, RunError::Contract(error), owned).await {}
        }
    };
    let owned = OwnedFrameFree::new(request, operation, caller);
    let request = match owned.admitted() {
        Ok(request) => request,
        Err(error) => match callback::reject(&endpoint.inner, error, owned).await {},
    };
    callback::complete_immediate(
        &endpoint.inner,
        &owned,
        || endpoint.inner.admit_work(request.publication_work()),
    )
    .await;
    if !request.is_current() {
        match callback::reject(
            &endpoint.inner,
            RunError::Contract("prepared cold insertion changed its selected memo"),
            owned,
        )
        .await {}
    }
    match owned.take_admitted() {
        Ok(request) => {
            let retirement = request.retirement();
            request.insert(Some(retirement))
        }
        Err((error, owned)) => match callback::reject(&endpoint.inner, error, owned).await {},
    }
}

pub(super) async fn fetch<'run, 'db: 'run, C, P>(
    endpoint: TaskEndpoint<'run, 'db>,
    ingredient: &'db IngredientImpl<C>,
    db: &'db C::DbView,
    id: Id,
    provider: P,
) -> RunResult<&'db C::Output<'db>>
where
    C: Configuration,
    P: ExecutionProvider<'run, 'db, C>,
{
    let (operation, caller) = enter_operation::<C>(&endpoint, provider.operation_policy()).await;
    let zalsa = db.zalsa();
    let zalsa_local = db.zalsa_local();
    let scope = CallbackScope::new(&operation, caller);
    let refresh = callback::complete_immediate(
        &endpoint.inner,
        &scope,
        || {
            zalsa.unwind_if_revision_cancelled(zalsa_local);
            Ok(Refresh::new(
                ingredient,
                db,
                zalsa,
                zalsa_local,
                id,
                ingredient.memo_ingredient_index(zalsa, id),
            ))
        },
    )
    .await;
    let selected = match select(&endpoint, &operation, caller, refresh, &provider).await {
        Ok(selected) => selected,
        Err(error) => match callback::reject(&endpoint.inner, error, scope).await {},
    };
    complete_selected(&endpoint, &operation, ingredient, db, id, selected).await
}

async fn select<'a, 'run, 'db: 'run, C, P>(
    endpoint: &TaskEndpoint<'run, 'db>,
    operation: &'a RunOperation<'db>,
    caller: Caller,
    refresh: Refresh<'db, C>,
    provider: &P,
) -> RunResult<OwnedFrameFree<'a, 'db, SelectedMemo<'db, C>>>
where
    C: Configuration,
    P: ExecutionProvider<'run, 'db, C>,
{
    let mut owned = OwnedFrameFree::new(SelectionStep::Probe(refresh), operation, caller);
    loop {
        admit_phase(endpoint, &owned).await;
        let current = match owned.take_admitted() {
            Ok(step) => step,
            Err((error, owned)) => match callback::reject(&endpoint.inner, error, owned).await {},
        };
        let step = match current {
            SelectionStep::Probe(refresh) => match refresh.probe_deferred() {
                FetchProbe::Ready(memo) => SelectionStep::Selected(memo),
                FetchProbe::Miss => SelectionStep::Claim(refresh),
                FetchProbe::Verify(verification) => SelectionStep::Selected(
                    verify_eager(endpoint, operation, caller, verification).await,
                ),
            },
            SelectionStep::Claim(refresh) => match refresh.claim() {
                SelectionStep::Wait(refresh, running) => {
                    #[cfg(test)]
                    let key = refresh.ingredient.database_key_index(refresh.id);
                    let waiting =
                        OwnedFrameFree::new(SelectionStep::Probe(refresh), operation, caller);
                    #[cfg(test)]
                    trace_owner("fetch.wait", Some(key), operation, caller);
                    super::frame_free::wait_for_query(endpoint, &waiting, running).await;
                    owned = match waiting.into_admitted() {
                        Ok(step) => OwnedFrameFree::new(step, operation, caller),
                        Err((error, waiting)) => {
                            match callback::reject(&endpoint.inner, error, waiting).await {}
                        }
                    };
                    continue;
                }
                step => step,
            },
            SelectionStep::Reload(refresh, claim) => {
                #[cfg(test)]
                super::tests::observation::record(super::tests::observation::Event::Claim {
                    key: claim.database_key_index(),
                    serial: claim.test_serial(),
                    operation: operation.ordinal,
                });
                refresh.reload(claim)
            }
            SelectionStep::Verify(refresh, verification) => {
                let verified = match verify_admitted(endpoint, operation, caller, verification)
                    .await
                {
                    Ok(verified) => verified,
                    Err(error) => match callback::reject(&endpoint.inner, error, refresh).await {},
                };
                refresh.verified(verified)
            }
            SelectionStep::Execute(request) => {
                let refresh = request.refresh;
                let memo = match execute_owned(
                    endpoint.inner.clone(),
                    operation,
                    refresh.ingredient,
                    refresh.db,
                    request.claim,
                    request.old_memo,
                    refresh.requirement(),
                    provider,
                )
                .await
                {
                    Ok(memo) => memo,
                    Err(error) => match callback::reject(&endpoint.inner, error, refresh).await {},
                };
                refresh.executed(memo)
            }
            SelectionStep::Participant(refresh, participant) => {
                match super::retire_participant_owned(&endpoint.inner, operation, participant).await
                {
                    super::super::participant::ParticipantProgress::Complete(memo) => {
                        refresh.executed(memo)
                    }
                    super::super::participant::ParticipantProgress::Pending(request) => {
                        SelectionStep::Participant(refresh, request)
                    }
                    super::super::participant::ParticipantProgress::Execute(request) => {
                        let memo = match super::execute_request_owned(
                            endpoint.inner.clone(),
                            operation,
                            request,
                            provider,
                        )
                        .await
                        {
                            Ok(memo) => memo,
                            Err(error) => {
                                match callback::reject(&endpoint.inner, error, refresh).await {}
                            }
                        };
                        refresh.executed(memo)
                    }
                }
            }
            SelectionStep::CycleProbe(refresh) => {
                let scope = CallbackScope::new(operation, caller);
                let decision = callback::complete_immediate(
                    &endpoint.inner,
                    &scope,
                    || Ok(refresh.probe_cold_cycle()),
                )
                .await;
                refresh.resume_cold_cycle(decision)
            }
            SelectionStep::ColdInitial(request) => {
                let initial = OwnedFrameFree::new(request, operation, caller);
                let refresh = match initial.admitted() {
                    Ok(request) => &request.refresh,
                    Err(error) => match callback::reject(&endpoint.inner, error, initial).await {},
                };
                let input = super::native_values::input(
                    &endpoint.inner,
                    &initial,
                    provider,
                    refresh.db,
                    refresh.id,
                )
                .await;
                let value = callback::complete(
                    &endpoint.inner,
                    &initial,
                    callback::CallbackKind::ColdInitial,
                    || provider.initial(refresh.db, refresh.id, input, endpoint.inner.clone()),
                )
                .await;
                let ready = match initial.returned(value) {
                    Ok(ready) => ready,
                    Err((error, value, initial)) => {
                        match callback::reject(&endpoint.inner, error, (value, initial)).await {}
                    }
                };
                insert_cold_ready(endpoint, operation, caller, ready).await
            }
            SelectionStep::ColdInitialReady(request) => {
                let ready = OwnedFrameFree::new(request, operation, caller);
                insert_cold_ready(endpoint, operation, caller, ready).await
            }
            SelectionStep::Selected(selected) => {
                // Transfer the admitted selection without exposing an unguarded return value.
                let owned = OwnedFrameFree::new(selected, operation, caller);
                let selected = match owned.admitted() {
                    Ok(selected) => selected,
                    Err(error) => match callback::reject(&endpoint.inner, error, owned).await {},
                };
                if operation.policy.declared == crate::attempt_probe::QueryPolicy::CompleteOnly {
                    let units = selected.memo().header.output_check_work(MemoOutputCheck::Controlled);
                    if units != 0 {
                        callback::complete_immediate(
                            &endpoint.inner,
                            &owned,
                            || endpoint.admit_work(units),
                        )
                        .await;
                    }
                    if !selected.memo().header.controlled_outputs_are_empty() {
                        match callback::reject(
                            &endpoint.inner,
                            RunError::Contract("controlled callable requires a memo without outputs or direct accumulators"),
                            owned,
                        ).await {}
                    }
                }
                return Ok(owned);
            }
            SelectionStep::Wait(refresh, running) => {
                drop(running);
                match callback::reject(
                    &endpoint.inner,
                    RunError::Contract("fetch wait escaped immediate handling"),
                    refresh,
                )
                .await {}
            }
        };
        owned = OwnedFrameFree::new(step, operation, caller);
    }
}

async fn complete_selected<'run, 'db: 'run, C: Configuration>(
    endpoint: &TaskEndpoint<'run, 'db>,
    operation: &RunOperation<'db>,
    ingredient: &IngredientImpl<C>,
    db: &'db C::DbView,
    id: Id,
    selected: OwnedFrameFree<'_, 'db, SelectedMemo<'db, C>>,
) -> RunResult<&'db C::Output<'db>> {
    // Selection remains owned through eviction and both existing delivery checks. A rejected
    // callback may have queued a child which must be destroyed before this caller is released.
    let value = deliver_selected(endpoint, &selected, &operation.context, ingredient, db, id).await;
    match selected.into_admitted() {
        Ok(_) => Ok(value),
        Err((error, selected)) => match callback::reject(&endpoint.inner, error, selected).await {},
    }
}

pub(super) trait SelectedReadOwner<'db, C: Configuration>: callback::CallbackOwner {
    /// Borrows the selected memo without checking this owner's validity.
    ///
    /// Callers must use a protected callback that checks the owner before and after access.
    fn selected(&self) -> RunResult<&SelectedMemo<'db, C>>;

    fn recheck_work(&self) -> usize {
        0
    }
}

impl<'db, C: Configuration> SelectedReadOwner<'db, C>
    for OwnedFrameFree<'_, 'db, SelectedMemo<'db, C>>
{
    fn selected(&self) -> RunResult<&SelectedMemo<'db, C>> {
        self.admitted()
    }
}

pub(super) async fn deliver_selected<'run, 'db: 'run, C, O>(
    endpoint: &TaskEndpoint<'run, 'db>,
    owner: &O,
    context: &RunContext<'db>,
    ingredient: &IngredientImpl<C>,
    db: &'db C::DbView,
    id: Id,
) -> &'db C::Output<'db>
where
    C: Configuration,
    O: SelectedReadOwner<'db, C>,
{
    callback::complete_immediate(&endpoint.inner, owner, || owner.check_resume()).await;
    callback::complete_immediate(&endpoint.inner, owner, || {
        let request = owner.selected()?;
        ingredient.record_memo_use(db.zalsa_local(), id, request);
        Ok(())
    })
    .await;
    let value = super::read_run::selected(endpoint, owner, ingredient, db, id).await;
    let value =
        callback::complete_immediate(&endpoint.inner, owner, || context.observe(Ok(value))).await;
    value
}
