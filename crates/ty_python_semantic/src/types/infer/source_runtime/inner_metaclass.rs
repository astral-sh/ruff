use salsa::execution_probe::{
    CallableRouteProvider, ExecutionWork, NativeValueOperation, NativeValueQuote, RetainedInput,
    RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::function::Configuration;

use super::native_values::DirectInput;
use super::{SourceAccess, SourceEffects, create_source_access};
use crate::types::class::metaclass_selection::MetaclassSelectionResult;
use crate::types::class::{ClassMetaclass, MetaclassErrorKind, StaticClassLiteral};
use crate::types::{ClassBase, DynamicType, Type};
use crate::{Db, Program};

pub(super) trait InnerMetaclassConfiguration:
    for<'a> Configuration<
        DbView = dyn Db,
        Input<'a> = StaticClassLiteral<'a>,
        Output<'a> = MetaclassSelectionResult<'a>,
    >
{
}

impl<C> InnerMetaclassConfiguration for C where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = StaticClassLiteral<'a>,
            Output<'a> = MetaclassSelectionResult<'a>,
        >
{
}

/// Executes the canonical inner metaclass query with the registered source routes.
#[derive(Debug)]
pub(super) struct InnerMetaclassProvider<'db, MakeAccess> {
    pub(super) program: Program<'db>,
    pub(super) access: MakeAccess,
}

/// Quotes generated class-handle conversion or native metaclass-result equality.
/// Equality compares interned identities, but may also compare a Type's inline Todo message.
pub(super) async fn quote<'call, 'run: 'call, 'db: 'run, C>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, C>,
) -> RunResult<NativeValueQuote>
where
    C: InnerMetaclassConfiguration,
{
    Ok(endpoint
        .local_call(|| {
            endpoint.admit_work(4)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: transfer_bytes::<NativeValueQuote>(2)?,
            })?;
            endpoint.check_completion()?;
            match operation {
                NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                    Ok(NativeValueQuote {
                        work: <StaticClassLiteral<'db> as DirectInput>::CONVERSION_WORK,
                        requested_bytes: size_of::<StaticClassLiteral<'db>>(),
                        cleanup_work: 0,
                    })
                }
                NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => Err(
                    RunError::Contract("metaclass input requires generated handle conversion"),
                ),
                NativeValueOperation::Comparison { left, right } => {
                    endpoint.admit_work(32)?;
                    // Conflict results contain up to two bases per operand. Inspecting their
                    // payloads creates temporary Type values; the string data stays borrowed.
                    let requested_bytes = size_of::<Type<'db>>()
                        .checked_mul(8)
                        .and_then(|bytes| bytes.checked_add(size_of::<DynamicType<'db>>() * 4))
                        .and_then(|bytes| bytes.checked_add(size_of::<usize>() * 8))
                        .ok_or(RunError::Contract("metaclass native quotation overflow"))?;
                    endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
                    endpoint.check_completion()?;
                    let work = inline_payload(left)?
                        .checked_add(inline_payload(right)?)
                        .and_then(|bytes| bytes.checked_add(32))
                        .ok_or(RunError::Contract("metaclass equality quotation overflow"))?;
                    Ok(NativeValueQuote {
                        work,
                        requested_bytes: size_of::<bool>(),
                        cleanup_work: 0,
                    })
                }
            }
        })
        .await)
}

/// Bounds inline string comparison without following interned class or transform metadata.
fn inline_payload(value: &MetaclassSelectionResult<'_>) -> RunResult<usize> {
    match value {
        Ok((ClassMetaclass::Selected(ty), _)) => Ok(ty.inline_payload_bytes()),
        Ok((ClassMetaclass::ProtocolFallback, _)) => Ok(0),
        Err(error) => match error.reason() {
            MetaclassErrorKind::Conflict {
                candidate, base, ..
            } => candidate
                .base
                .as_ref()
                .map_or(0, base_inline_payload)
                .checked_add(base_inline_payload(base))
                .ok_or(RunError::Contract(
                    "metaclass base payload quotation overflow",
                )),
            MetaclassErrorKind::NotCallable(ty) | MetaclassErrorKind::PartlyNotCallable(ty) => {
                Ok(ty.inline_payload_bytes())
            }
            MetaclassErrorKind::GenericMetaclass | MetaclassErrorKind::Cycle => Ok(0),
        },
    }
}

/// Counts the borrowed inline string payload that native equality can visit in a class base.
fn base_inline_payload(base: &ClassBase<'_>) -> usize {
    match base {
        ClassBase::Dynamic(dynamic) => Type::Dynamic(*dynamic).inline_payload_bytes(),
        ClassBase::Any
        | ClassBase::Divergent(_)
        | ClassBase::Class(_)
        | ClassBase::Protocol
        | ClassBase::Generic
        | ClassBase::TypedDict(_) => 0,
    }
}

impl<'run, 'db: 'run, C, A, MakeAccess> CallableRouteProvider<'run, 'db, C>
    for InnerMetaclassProvider<'db, MakeAccess>
where
    C: InnerMetaclassConfiguration,
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
        #[cfg(test)]
        super::tests::inner_metaclass::observe_native(_db);
        quote(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        #[cfg(test)]
        super::tests::inner_metaclass::observe_body(_db, class);
        SourceEffects::new(&access, self.program)
            .infer_inner_metaclass(class)
            .await
    }

    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        id: salsa::Id,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .check_metaclass_program(class)
            .await?;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(4)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: cycle_transfer_bytes(2)?,
                })?;
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
        last: &'call MetaclassSelectionResult<'db>,
        value: MetaclassSelectionResult<'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>>
    where
        'run: 'call,
    {
        let access = create_source_access(&endpoint, &self.access).await?;
        SourceEffects::new(&access, self.program)
            .check_metaclass_program(class)
            .await?;
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(6)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: cycle_transfer_bytes(3)?,
                })?;
                endpoint.check_completion()?;
                Ok(C::recover_from_cycle(db, cycle, last, value, class))
            })
            .await)
    }
}

/// Counts fixed value copies and the result carrier used by an admitted local call.
fn transfer_bytes<T>(copies: usize) -> RunResult<usize> {
    size_of::<T>()
        .checked_mul(copies)
        .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>()))
        .ok_or(RunError::Contract("metaclass transfer quotation overflow"))
}

/// Includes the direct class input alongside the generated cycle callback's result transfers.
fn cycle_transfer_bytes(copies: usize) -> RunResult<usize> {
    transfer_bytes::<MetaclassSelectionResult<'_>>(copies)?
        .checked_add(size_of::<StaticClassLiteral<'_>>())
        .ok_or(RunError::Contract("metaclass transfer quotation overflow"))
}
