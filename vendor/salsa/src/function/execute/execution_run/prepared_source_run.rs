use std::future::{pending, ready};
use std::marker::PhantomData;

use super::callback::{self, CallbackKind, CallbackOwner};
use super::fetch_run::{SelectedReadOwner, deliver_selected};
use super::frame_free::EntryScope;
use super::registration::TaskEndpoint;
use super::{RunError, RunResult};
use crate::function::Configuration;
use crate::function::memo::{PreparedSourceMemo, SelectedMemo};

mod sealed {
    pub trait Sealed {}
}

/// A passive request branded by the query that declares the prepared memo.
pub trait PreparedSourceRead<'db>: sealed::Sealed {
    type Output: 'db;
    #[doc(hidden)]
    type Configuration: Configuration<Output<'db> = Self::Output>;

    #[doc(hidden)]
    fn prepared(&self) -> &PreparedSourceMemo<'db, Self::Output>;
}

#[doc(hidden)]
pub struct PreparedSourceRequest<'call, 'db, C: Configuration> {
    memo: &'call PreparedSourceMemo<'db, C::Output<'db>>,
    configuration: PhantomData<fn() -> C>,
}

impl<'call, 'db, C: Configuration> PreparedSourceRequest<'call, 'db, C> {
    pub fn new(memo: &'call PreparedSourceMemo<'db, C::Output<'db>>) -> Self {
        Self {
            memo,
            configuration: PhantomData,
        }
    }
}

impl<C: Configuration> sealed::Sealed for PreparedSourceRequest<'_, '_, C> {}

impl<'db, C: Configuration> PreparedSourceRead<'db> for PreparedSourceRequest<'_, 'db, C> {
    type Output = C::Output<'db>;
    type Configuration = C;

    fn prepared(&self) -> &PreparedSourceMemo<'db, Self::Output> {
        self.memo
    }
}

// A freshness check bounds stamp construction/comparison by 8 scalar operations, retained-slot
// loading and type metadata access by 12, and allocation/type comparison by 4. Publication facts
// were checked when the certificate was created. No key-family lookup, dependency traversal or
// user output code is involved.
const CURRENT_CHECK_WORK: usize = 8 + 12 + 4;
// Typed recovery adds configuration/ingredient type checks and the already initialized caster.
const RECOVERY_WORK: usize = 16;

struct PreparedReadOwner<'call, 'db, C: Configuration> {
    scope: EntryScope<'call, 'db>,
    memo: &'call PreparedSourceMemo<'db, C::Output<'db>>,
    selected: SelectedMemo<'db, C>,
}

impl<C: Configuration> PreparedReadOwner<'_, '_, C> {
    fn check_current(&self) -> RunResult<()> {
        if !self.scope.is_current() {
            return Err(RunError::Contract(
                "prepared source changed its enclosing scope",
            ));
        }
        self.memo
            .check_current_inner()
            .map_err(|error| RunError::Contract(error.message()))
    }
}

impl<C: Configuration> CallbackOwner for PreparedReadOwner<'_, '_, C> {
    fn check_resume(&self) -> RunResult<()> {
        self.scope.check_resume()?;
        self.memo
            .check_current_inner()
            .map_err(|error| RunError::Contract(error.message()))
    }

    fn is_current(&self) -> bool {
        self.check_current().is_ok()
    }
}

impl<'db, C: Configuration> SelectedReadOwner<'db, C> for PreparedReadOwner<'_, 'db, C> {
    fn selected(&self) -> RunResult<&SelectedMemo<'db, C>> {
        Ok(&self.selected)
    }

    fn recheck_work(&self) -> usize {
        CURRENT_CHECK_WORK
    }
}

pub(super) async fn read<'call, 'run: 'call, 'db: 'run, R>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    request: R,
) -> &'db R::Output
where
    R: PreparedSourceRead<'db> + 'call,
{
    if !endpoint.inner.local_call_is_eligible() {
        let _held = request;
        return pending().await;
    }
    let scope = match EntryScope::capture(&endpoint.inner.context) {
        Ok(scope) => scope,
        Err(error) => match callback::reject(&endpoint.inner, error, request).await {},
    };
    callback::complete(
        &endpoint.inner,
        &scope,
        CallbackKind::NativeAdmission,
        || {
            // Each complete_immediate callback checks the owner before and after its closure.
            // Recovery checks once; deliver_selected's first callback adds an explicit check
            // (three total), and its memo-use callback checks twice. read_run::transaction
            // checks once before charging and then pays for the remaining checks.
            ready(endpoint.admit_work(7 * CURRENT_CHECK_WORK + RECOVERY_WORK))
        },
    )
    .await;
    let memo = request.prepared();
    let (db, ingredient, id, selected) =
        callback::complete(&endpoint.inner, &scope, CallbackKind::NativeCall, || {
            ready((|| {
                if !memo.belongs_to(endpoint.inner.context.db) {
                    return Err(RunError::Contract(
                        "prepared source belongs to another database",
                    ));
                }
                memo.recover::<R::Configuration>()
                    .map_err(|error| RunError::Contract(error.message()))
            })())
        })
        .await;
    let owner = PreparedReadOwner {
        scope,
        memo,
        selected,
    };
    let value = deliver_selected(
        endpoint,
        &owner,
        &endpoint.inner.context,
        ingredient,
        db,
        id,
    )
    .await;
    if let Err(error) = owner.check_current() {
        match callback::reject(&endpoint.inner, error, owner).await {}
    }
    value
}
