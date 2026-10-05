use salsa::execution_probe::{
    BorrowOrCopy, CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunError,
    RunResult, TaskEndpoint,
};

use super::{SourceAccess, SourceEffects, create_source_access};
use crate::place::PlaceAndQualifiers;
use crate::types::member_lookup::runtime_profile::{
    ClassMemberConfiguration, quote_class_member_native_value,
};
use crate::{Db, Program, ProgramEnvironment};

pub(super) struct ClassMemberProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for ClassMemberProvider<'db, MakeAccess>
where
    C: ClassMemberConfiguration,
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
        quote_class_member_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        key: C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .class_member_body(key)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        key: C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<PlaceAndQualifiers<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, key))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call PlaceAndQualifiers<'db>,
        value: PlaceAndQualifiers<'db>,
        key: C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        let program = endpoint
            .read_field(
                key.field_requests(endpoint.field_request_context())
                    .program(),
                &BorrowOrCopy,
            )
            .await;
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<ProgramEnvironment<'db>>() * 2 + 1)?;
                endpoint.check_completion()?;
                if program != self.program {
                    return Err(RunError::Contract(
                        "class-member recovery program is foreign",
                    ));
                }
                Ok(ProgramEnvironment::from_program(program))
            })
            .await;
        SourceEffects::new(&access, program)
            .normalize_place_cycle(&env, value, *last, cycle)
            .await
    }
}
