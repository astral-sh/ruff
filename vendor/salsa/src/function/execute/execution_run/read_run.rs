use super::callback::{self, CallbackOwner};
use super::fetch_run::SelectedReadOwner;
use super::registration::TaskEndpoint;
use super::{ExecutionWork, RunError, RunResult};
use crate::Id;
use crate::function::{Configuration, IngredientImpl};
use crate::zalsa::ZalsaDatabase;
use crate::zalsa_local::ZalsaLocal;
use crate::zalsa_local::dependency_read::DependencyRead;

pub(super) async fn selected<'run, 'db: 'run, C, O>(
    endpoint: &TaskEndpoint<'run, 'db>,
    owner: &O,
    ingredient: &IngredientImpl<C>,
    db: &'db C::DbView,
    id: Id,
) -> &'db C::Output<'db>
where
    C: Configuration,
    O: SelectedReadOwner<'db, C>,
{
    transaction(
        endpoint,
        owner,
        db.zalsa_local(),
        owner.recheck_work(),
        || {
            let selected = owner.selected()?;
            Ok((
                selected,
                ingredient.prepare_memo_dependency_read(db.zalsa(), id, selected),
            ))
        },
        |(_, read)| &read.read,
        |(selected, read)| {
            Ok(ingredient.record_memo_dependencies_with_read(
                db.zalsa(),
                db.zalsa_local(),
                id,
                selected,
                Some(read),
            ))
        },
    )
    .await
}

pub(super) async fn dependency(
    endpoint: &TaskEndpoint<'_, '_>,
    owner: &impl CallbackOwner,
    local: &ZalsaLocal,
    read: &DependencyRead<'_>,
) {
    transaction(
        endpoint,
        owner,
        local,
        0,
        || Ok(read),
        |read| read,
        |read| {
            local.apply_dependency_read(read);
            Ok(())
        },
    )
    .await;
}

/// Selection borrows canonical storage. Commit applies the freshly selected metadata without
/// calling observers, so no mutation can invalidate the reservation before the read is recorded.
async fn transaction<'metadata, O, S, T>(
    endpoint: &TaskEndpoint<'_, '_>,
    owner: &O,
    local: &ZalsaLocal,
    recheck_work: usize,
    select: impl Fn() -> RunResult<S>,
    metadata: impl for<'a> Fn(&'a S) -> &'a DependencyRead<'metadata>,
    commit: impl FnOnce(S) -> RunResult<T>,
) -> T
where
    O: CallbackOwner,
{
    let mut commit = Some(commit);
    loop {
        let plan = callback::complete_immediate(&endpoint.inner, owner, || {
            let plan = {
                let selected = select()?;
                local
                    .prepare_dependency_read(metadata(&selected))
                    .map_err(RunError::Contract)?
            };
            let work = plan.work();
            // After charging: this callback checks once and selection/commit checks twice.
            // Successful delivery adds two observation checks and one final prepared-owner
            // check. A retry instead needs only the next quotation's initial check.
            // Prepared-source reads prepay this callback's first initial check.
            let units = recheck_work
                .checked_mul(6)
                .and_then(|checks| work.units.checked_add(checks));
            units
                .ok_or(RunError::Contract("dependency owner work overflow"))
                .and_then(|units| endpoint.admit_work(units))
                .and_then(|()| {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: work.requested_bytes,
                    })
                })?;
            Ok(plan)
        })
        .await;
        let value = callback::complete_immediate(&endpoint.inner, owner, || {
            let selected = select()?;
            if !local
                .reserve_dependency_read(plan, metadata(&selected))
                .map_err(RunError::Contract)?
            {
                return Ok(None);
            }
            let commit = commit
                .take()
                .ok_or(RunError::Contract("dependency read already committed"))?;
            commit(selected).map(Some)
        })
        .await;
        if let Some(value) = value {
            return value;
        }
        // An admission observer can add reads to the same frame. Its new storage requires a
        // fresh quote; the previous accepted work and allocation charges remain spent.
    }
}
