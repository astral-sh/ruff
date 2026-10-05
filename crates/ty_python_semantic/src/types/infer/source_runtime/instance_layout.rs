use salsa::execution_probe::{
    CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunError, RunResult,
    TaskEndpoint,
};
use salsa::plumbing::function::Configuration;

use super::{SourceAccess, SourceEffects, create_source_access, native_values};
use crate::types::class::StaticClassLiteral;
use crate::types::class::slots::InstanceLayout;
use crate::{Db, Program};

pub(super) trait InstanceLayoutConfiguration:
    for<'a> Configuration<
        DbView = dyn Db,
        Input<'a> = StaticClassLiteral<'a>,
        Output<'a> = InstanceLayout,
    >
{
}

impl<C> InstanceLayoutConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = InstanceLayout,
        >
{
}

pub(super) struct InstanceLayoutProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for InstanceLayoutProvider<'db, MakeAccess>
where
    C: InstanceLayoutConfiguration,
    A: SourceAccess<'run, 'db>,
    MakeAccess: Fn(TaskEndpoint<'run, 'db>) -> A + 'run,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        native_values::quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<InstanceLayout>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        #[cfg(test)]
        super::tests::instance_layout::observe_body(access.db(), class);
        SourceEffects::new(&access, self.program)
            .infer_instance_layout(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<InstanceLayout>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<InstanceLayout>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, class))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call InstanceLayout,
        value: InstanceLayout,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<InstanceLayout>
    where
        'run: 'call,
    {
        let mut value = Some(value);
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<InstanceLayout>() * 2 + 1)?;
                endpoint.check_completion()?;
                let value = value.take().ok_or(RunError::Contract(
                    "instance layout candidate already consumed",
                ))?;
                Ok(C::recover_from_cycle(db, cycle, last, value, class))
            })
            .await)
    }
}
