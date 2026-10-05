use ruff_python_ast::name::Name;
use salsa::execution_probe::{
    CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunError, RunResult,
    TaskEndpoint,
};
use salsa::plumbing::function::Configuration;
use ty_python_core::scope::ScopeId;

use super::{SourceAccess, SourceEffects, create_source_access, native_values};
use crate::{Db, Program};

pub(super) trait ImplicitNamesConfiguration:
    for<'a> Configuration<DbView = dyn Db, Input<'a> = ScopeId<'a>, Output<'a> = Box<[Name]>>
{
}

impl<C> ImplicitNamesConfiguration for C where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = ScopeId<'a>, Output<'a> = Box<[Name]>>
{
}

pub(super) struct ImplicitNamesProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for ImplicitNamesProvider<'db, MakeAccess>
where
    C: ImplicitNamesConfiguration,
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
        scope: ScopeId<'db>,
    ) -> RunResult<Box<[Name]>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_implicit_attribute_names(scope)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _scope: ScopeId<'db>,
    ) -> RunResult<Box<[Name]>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Err(RunError::Contract(
                    "implicit attribute names query unexpectedly formed a cycle",
                ))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Box<[Name]>,
        value: Box<[Name]>,
        _scope: ScopeId<'db>,
    ) -> RunResult<Box<[Name]>>
    where
        'run: 'call,
    {
        let result = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Err(RunError::Contract(
                    "implicit attribute names query unexpectedly required cycle recovery",
                ))
            })
            .await;
        drop(value);
        Ok(result)
    }
}
