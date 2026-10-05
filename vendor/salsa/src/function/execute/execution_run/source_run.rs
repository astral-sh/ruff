use std::future::{pending, ready};
use std::rc::Rc;

use super::callback::{self, CallbackOwner};
use super::fetch_run::{SelectedReadOwner, deliver_selected};
use super::frame_free::EntryScope;
use super::registration::{FinalSourceRoute, FinalSources, TaskEndpoint};
use super::{RunError, RunResult};
use crate::function::memo::{FinalSourceMemo, SelectedMemo};
use crate::function::{Configuration, VerifyResult};
use crate::{Id, Revision};

pub(super) enum SourceWork {
    RegistrationKey,
    Read,
    Validation,
}

impl SourceWork {
    pub(super) fn units(self, keys: usize) -> RunResult<usize> {
        let base: usize = match self {
            Self::RegistrationKey => return Ok(16),
            Self::Read => 64,
            Self::Validation => 32,
        };
        let comparisons = if keys == 0 {
            0
        } else {
            1 + (usize::BITS - (keys - 1).leading_zeros()) as usize
        };
        base.checked_add(comparisons)
            .ok_or(RunError::Contract("final source work size overflow"))
    }
}

struct SourceReadOwner<'call, 'db, C: Configuration> {
    scope: EntryScope<'call, 'db>,
    token: &'call FinalSourceMemo<'db, C>,
}

impl<C: Configuration> SourceReadOwner<'_, '_, C> {
    fn check_current(&self) -> RunResult<()> {
        if !self.scope.is_current() {
            return Err(RunError::Contract(
                "final source changed its enclosing scope",
            ));
        }
        self.token
            .check_current()
            .map_err(|error| RunError::Contract(error.message()))
    }
}

impl<C: Configuration> CallbackOwner for SourceReadOwner<'_, '_, C> {
    fn check_resume(&self) -> RunResult<()> {
        self.scope.check_resume()?;
        self.token
            .check_current()
            .map_err(|error| RunError::Contract(error.message()))
    }

    fn is_current(&self) -> bool {
        self.check_current().is_ok()
    }
}

impl<'db, C: Configuration> SelectedReadOwner<'db, C> for SourceReadOwner<'_, 'db, C> {
    fn selected(&self) -> RunResult<&SelectedMemo<'db, C>> {
        self.check_current()?;
        Ok(self.token.selected())
    }
}

fn select<'call, 'db, C: Configuration>(
    sources: &'call FinalSources<'db, C>,
    id: Id,
) -> RunResult<&'call FinalSourceMemo<'db, C>> {
    let index = sources
        .prepared
        .binary_search_by_key(&id, FinalSourceMemo::id)
        .map_err(|_| RunError::Contract("final source key is not registered"))?;
    let token = &sources.prepared[index];
    token
        .check_current()
        .map_err(|error| RunError::Contract(error.message()))?;
    Ok(token)
}

pub(super) async fn read<'call, 'run: 'call, 'db: 'run, C: Configuration>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    route: &'call FinalSourceRoute<'db, C>,
    id: Id,
) -> &'db C::Output<'db> {
    if !endpoint.inner.local_call_is_eligible() {
        let _held = route;
        return pending().await;
    }
    let scope = match EntryScope::capture(&endpoint.inner.context) {
        Ok(scope) => scope,
        Err(error) => match callback::reject(&endpoint.inner, error, route).await {},
    };
    let token = callback::complete(
        &endpoint.inner,
        &scope,
        callback::CallbackKind::Canonical,
        || {
            ready((|| {
                endpoint.admit_work(SourceWork::Read.units(route.sources.prepared.len())?)?;
                endpoint.check_final_source(route)?;
                select(&route.sources, id)
            })())
        },
    )
    .await;
    let owner = SourceReadOwner { scope, token };
    let value = deliver_selected(
        endpoint,
        &owner,
        &endpoint.inner.context,
        route.sources.ingredient,
        route.sources.db,
        id,
    )
    .await;
    if let Err(error) = owner.check_current() {
        match callback::reject(&endpoint.inner, error, owner).await {}
    }
    value
}

pub(super) async fn validate<'run, 'db: 'run, C: Configuration>(
    endpoint: TaskEndpoint<'run, 'db>,
    sources: Rc<FinalSources<'db, C>>,
    id: Id,
    revision: Revision,
) -> RunResult<VerifyResult> {
    if !endpoint.inner.local_call_is_eligible() {
        let _held = sources;
        return pending().await;
    }
    let scope = match EntryScope::capture(&endpoint.inner.context) {
        Ok(scope) => scope,
        Err(error) => match callback::reject(&endpoint.inner, error, sources).await {},
    };
    let token = callback::complete(
        &endpoint.inner,
        &scope,
        callback::CallbackKind::Canonical,
        || {
            ready((|| {
                endpoint.admit_work(SourceWork::Validation.units(sources.prepared.len())?)?;
                select(&sources, id)
            })())
        },
    )
    .await;
    let owner = SourceReadOwner { scope, token };
    let result = callback::complete(
        &endpoint.inner,
        &owner,
        callback::CallbackKind::Canonical,
        || {
            ready(owner.check_resume().map(|()| {
                token
                    .selected()
                    .memo()
                    .header
                    .current_revision_result(revision)
            }))
        },
    )
    .await;
    if let Err(error) = owner.check_current() {
        match callback::reject(&endpoint.inner, error, owner).await {}
    }
    Ok(result)
}
