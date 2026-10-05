use salsa::execution_probe::{
    CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunError, RunResult,
    TaskEndpoint,
};

use super::{SourceAccess, SourceEffects, create_source_access};
use crate::types::descriptor::{DescriptorRequest, DescriptorResult};
use crate::types::member_lookup::runtime_profile::{
    DescriptorConfiguration, quote_descriptor_native_value,
};
use crate::{Db, Program};

pub(super) struct DescriptorProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for DescriptorProvider<'db, MakeAccess>
where
    C: DescriptorConfiguration,
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
        quote_descriptor_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (program, ty, instance, owner): C::Input<'db>,
    ) -> RunResult<DescriptorResult<'db>>
    where
        'run: 'call,
    {
        endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                #[cfg(test)]
                super::tests::nominal_members::observe_descriptor_body(_db);
                if program != self.program {
                    return Err(RunError::Contract("descriptor lookup program is foreign"));
                }
                Ok(())
            })
            .await;
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, program)
            .descriptor_protocol(DescriptorRequest {
                ty,
                instance,
                owner,
            })
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        input: C::Input<'db>,
    ) -> RunResult<DescriptorResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<DescriptorResult<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, input))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call DescriptorResult<'db>,
        value: DescriptorResult<'db>,
        input: C::Input<'db>,
    ) -> RunResult<DescriptorResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<DescriptorResult<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, input))
            })
            .await)
    }
}
