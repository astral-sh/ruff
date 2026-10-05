use salsa::execution_probe::{
    CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunError, RunResult,
    TaskEndpoint,
};
use salsa::plumbing::function::Configuration;
use ty_python_core::scope::ScopeId;

use super::{SourceAccess, SourceEffects, create_source_access, native_values};
use crate::place::module_type_body_scope_with;
use crate::{Db, Program, ProgramEnvironment};

pub(super) trait ModuleTypeScopeConfiguration:
    for<'a> Configuration<DbView = dyn Db, Input<'a> = Program<'a>, Output<'a> = Option<ScopeId<'a>>>
{
}

impl<C> ModuleTypeScopeConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = Program<'a>,
            Output<'a> = Option<ScopeId<'a>>,
        >
{
}

pub(super) struct ModuleTypeScopeProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for ModuleTypeScopeProvider<'db, MakeAccess>
where
    C: ModuleTypeScopeConfiguration,
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
        db: &'db dyn Db,
        program: Program<'db>,
    ) -> RunResult<Option<ScopeId<'db>>>
    where
        'run: 'call,
    {
        let env = endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                if program != self.program {
                    return Err(RunError::Contract("module type scope program is foreign"));
                }
                Ok(ProgramEnvironment::from_program(program))
            })
            .await;
        let access = create_source_access(&endpoint, &self.access).await?;
        // Module resolution and global-scope access prepare the file before the shared
        // selector reads its source tables and AST. It obtains the body scope directly from
        // the vendored class declaration, without inferring its bases or decorators.
        module_type_body_scope_with(db, &env, &SourceEffects::new(&access, self.program)).await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _program: Program<'db>,
    ) -> RunResult<Option<ScopeId<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Err(RunError::Contract(
                    "module type scope query unexpectedly formed a cycle",
                ))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call Option<ScopeId<'db>>,
        _value: Option<ScopeId<'db>>,
        _program: Program<'db>,
    ) -> RunResult<Option<ScopeId<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                endpoint.check_completion()?;
                Err(RunError::Contract(
                    "module type scope query unexpectedly required cycle recovery",
                ))
            })
            .await)
    }
}
