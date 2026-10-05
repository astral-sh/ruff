use salsa::execution_probe::{
    CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::Configuration;

use super::{SourceAccess, SourceEffects, create_source_access, native_values};
use crate::types::class::{CodeGeneratorKind, StaticClassLiteral};
use crate::{Db, Program};

pub(super) trait CodeGeneratorConfiguration:
    for<'a> Configuration<
        DbView = dyn Db,
        Input<'a> = StaticClassLiteral<'a>,
        Output<'a> = Option<CodeGeneratorKind<'a>>,
    >
{
}

impl<C> CodeGeneratorConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = Option<CodeGeneratorKind<'a>>,
        >
{
}

pub(super) struct CodeGeneratorProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for CodeGeneratorProvider<'db, MakeAccess>
where
    C: CodeGeneratorConfiguration,
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
    ) -> RunResult<Option<CodeGeneratorKind<'db>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        #[cfg(test)]
        super::tests::nominal_members::observe_code_generator_body(access.db(), class);
        SourceEffects::new(&access, self.program)
            .infer_code_generator(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<Option<CodeGeneratorKind<'db>>>() * 2 + 1)?;
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
        last: &'call Option<CodeGeneratorKind<'db>>,
        value: Option<CodeGeneratorKind<'db>>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<Option<CodeGeneratorKind<'db>>>() * 2 + 1)?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, class))
            })
            .await)
    }
}
