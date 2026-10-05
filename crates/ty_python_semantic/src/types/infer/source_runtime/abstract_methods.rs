use ruff_python_ast::name::Name;
use salsa::execution_probe::{
    CallableRouteProvider, NativeValueOperation, NativeValueQuote, RunError, RunResult,
    TaskEndpoint,
};
use salsa::plumbing::function::Configuration;
use ty_python_core::definition::Definition;

use super::{SourceAccess, SourceEffects, create_source_access, native_values};
use crate::types::ClassType;
use crate::types::abstract_methods::AbstractMethod;
use crate::{Db, FxIndexMap, Program};

pub(super) trait AbstractMethodsConfiguration:
    for<'a> Configuration<
        DbView = dyn Db,
        Input<'a> = ClassType<'a>,
        Output<'a> = FxIndexMap<Name, AbstractMethod<'a>>,
    >
{
}

impl<C> AbstractMethodsConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = ClassType<'a>,
            Output<'a> = FxIndexMap<Name, AbstractMethod<'a>>,
        >
{
}

pub(super) struct AbstractMethodsProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for AbstractMethodsProvider<'db, MakeAccess>
where
    C: AbstractMethodsConfiguration,
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
        class: ClassType<'db>,
    ) -> RunResult<FxIndexMap<Name, AbstractMethod<'db>>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_abstract_methods(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        class: ClassType<'db>,
    ) -> RunResult<FxIndexMap<Name, AbstractMethod<'db>>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<FxIndexMap<Name, AbstractMethod<'db>>>() * 2 + 1)?;
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
        last: &'call FxIndexMap<Name, AbstractMethod<'db>>,
        value: FxIndexMap<Name, AbstractMethod<'db>>,
        class: ClassType<'db>,
    ) -> RunResult<FxIndexMap<Name, AbstractMethod<'db>>>
    where
        'run: 'call,
    {
        let mut value = Some(value);
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(size_of::<FxIndexMap<Name, AbstractMethod<'db>>>() * 2 + 1)?;
                endpoint.check_completion()?;
                let value = value.take().ok_or(RunError::Contract(
                    "abstract methods candidate already consumed",
                ))?;
                Ok(C::recover_from_cycle(db, cycle, last, value, class))
            })
            .await)
    }
}

pub(super) trait MightBeExplicitlyAbstractConfiguration:
    for<'a> Configuration<DbView = dyn Db, Input<'a> = Definition<'a>, Output<'a> = bool>
{
}

impl<C> MightBeExplicitlyAbstractConfiguration for C where
    C: for<'a> Configuration<DbView = dyn Db, Input<'a> = Definition<'a>, Output<'a> = bool>
{
}

pub(super) struct MightBeExplicitlyAbstractProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for MightBeExplicitlyAbstractProvider<'db, MakeAccess>
where
    C: MightBeExplicitlyAbstractConfiguration,
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
        definition: Definition<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .infer_might_be_explicitly_abstract(definition)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        definition: Definition<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                Ok(C::cycle_initial(db, id, definition))
            })
            .await)
    }

    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        cycle: &'call salsa::Cycle<'call>,
        last: &'call bool,
        value: bool,
        definition: Definition<'db>,
    ) -> RunResult<bool>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(3)?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, definition))
            })
            .await)
    }
}
