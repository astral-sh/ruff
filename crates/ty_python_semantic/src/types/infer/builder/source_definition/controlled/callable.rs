//! Callable conversion borrows canonical signatures and the source root's value registry.

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::callable_guard::EXACT_CALLABLE_ENTRY_QUOTE;
use super::local_transfer::boxed_future_with_fixed_transfers_at;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::Place;
use crate::types::callable::conversion::{
    self, CallMemberEffects, CallMemberFacts, ConversionAdmission, ConversionStep,
    FunctionConversionEffects, SubclassCallableEffects, call_member_with, subclass_callable_with,
};
use crate::types::callable::{
    CallableConversionOperation, CallableConversionRequest, CallableType, CallableTypeKind,
    CallableTypes, UpcastPolicy,
};
use crate::types::cyclic::entry::{
    ExactCallableEntryDecision, callable_enter_exact_in_place_with,
};
use crate::types::cyclic::{CallableExpansion, CallableRecursionGuard, CallableVisitScope};
use crate::types::function::FunctionType;
use crate::types::signatures::{CallableSignature, Parameters, Signature};
use crate::types::signatures::source::parameters_storage_quote;
use crate::types::{DescriptorOrigin, MemberLookupResult, ResolvedMember, SubclassOfType, Type};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn reachability_callables(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<CallableTypes<'db>>> {
        let request = CallableConversionRequest::new(ty, UpcastPolicy::default());
        if self
            .local(1, 0, || request.requires_recursion_guard())
            .await?
        {
            return self
                .unavailable(SourceOperation::CallableConversion(
                    CallableConversionOperation::RecursionGuard,
                ))
                .await;
        }
        self.callable_conversion_body(env, request, None).await
    }

    /// Converts a type using the relation's upcast policy and the caller's exact guard, if supplied.
    /// A nominal conversion without a supplied guard owns its constructor-selected guard until
    /// member lookup and conversion finish, independently of any enclosing binding preparation.
    pub(in crate::types::infer) async fn callables_with_policy(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        policy: UpcastPolicy,
        guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<Option<CallableTypes<'db>>> {
        self.environment_program(env).await?;
        let request = self
            .initialize_value(|| CallableConversionRequest::new(ty, policy))
            .await?;
        let needs_root = self
            .local(2, size_of::<bool>(), || {
                guard.is_none() && request.requires_recursion_guard()
            })
            .await?;
        let owner = if needs_root {
            Some(self.admitted_constructor_guard(ty).await?)
        } else {
            None
        };
        let Some(guard) = guard.or(owner.as_ref()) else {
            return self.callable_conversion_body(env, request, None).await;
        };
        let mut scope = None;
        self.local(1, size_of::<CallableVisitScope<'_, 'db>>() * 2, || {
            scope = Some(guard.begin_scope());
        })
        .await?;
        let mut scope = scope.ok_or(RunError::Contract(
            "callable guard scope was not constructed",
        ))?;
        let decision = boxed_future_with_fixed_transfers_at(
            self.access.endpoint(),
            Ok(EXACT_CALLABLE_ENTRY_QUOTE),
            || {
                callable_enter_exact_in_place_with(
                    (CallableExpansion::Upcast, ty),
                    &mut scope,
                    self,
                )
            },
        )
        .await?
        .await?;
        match decision {
            ExactCallableEntryDecision::Entered => {
                self.callable_conversion_body(env, request, Some(guard))
                    .await
            }
            ExactCallableEntryDecision::ExactCycle => {
                self.unavailable(SourceOperation::CallableConversion(
                    CallableConversionOperation::GuardCycle,
                ))
                .await
            }
        }
    }

    async fn callable_conversion_body(
        &self,
        env: &ProgramEnvironment<'db>,
        request: CallableConversionRequest<'db>,
        guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<Option<CallableTypes<'db>>> {
        let program = self.environment_program(env).await?;
        let env = self
            .initialize_value(|| ProgramEnvironment::from_program(program))
            .await?;
        let endpoint = self.access.endpoint();
        let started = endpoint
            .local_call(|| {
                match conversion::start_with(
                    self.db(),
                    &env,
                    request,
                    guard.is_some(),
                    &SourceConversionAdmission { endpoint },
                ) {
                    Ok(step) => Ok(Ok(step)),
                    Err(ConversionFailure::Runtime(error)) => Err(error),
                    Err(ConversionFailure::Unavailable(operation)) => Ok(Err(operation)),
                }
            })
            .await;
        let mut step = match started {
            Ok(step) => step,
            Err(operation) => {
                return self
                    .unavailable(SourceOperation::CallableConversion(operation))
                    .await;
            }
        };
        loop {
            let operation = match step {
                ConversionStep::Complete(callables) => return Ok(callables),
                ConversionStep::Function(pending) => {
                    let callable = pending
                        .function
                        .into_callable_type_with(self.db(), self)
                        .await?;
                    step = self
                        .local(1, size_of::<ConversionStep<'db>>() * 2, || {
                            pending.resume(callable)
                        })
                        .await?;
                    continue;
                }
                ConversionStep::SubclassInstance(subclass) => {
                    let callables = self
                        .allocate_future(|| subclass_callable_with(subclass, self))
                        .await?
                        .await?;
                    self.local(1, size_of::<Option<CallableTypes<'db>>>(), || ())
                        .await?;
                    let mut callables = Some(callables);
                    step = self
                        .local(2, size_of::<ConversionStep<'db>>(), || {
                            ConversionStep::Complete(callables.take())
                        })
                        .await?;
                    continue;
                }
                ConversionStep::CallMember(pending) => {
                    let Some(guard) = guard else {
                        return self
                            .unavailable(SourceOperation::CallableConversion(
                                CallableConversionOperation::RecursionGuard,
                            ))
                            .await;
                    };
                    step = call_member_with(
                        pending,
                        CallMemberFacts,
                        &SourceCallMemberEffects {
                            source: self,
                            guard,
                        },
                    )
                    .await?;
                    continue;
                }
                ConversionStep::Convert(_) => CallableConversionOperation::Continuation,
                ConversionStep::RuntimeUnion(_) => CallableConversionOperation::RuntimeUnion,
                ConversionStep::Constructor { .. } => CallableConversionOperation::Constructor,
                ConversionStep::CachedBoundMethod(_) => CallableConversionOperation::BoundMethod,
            };
            return self
                .unavailable(SourceOperation::CallableConversion(operation))
                .await;
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassCallableEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn top_parameters(&self) -> RunResult<Parameters<'db>> {
        let quote = parameters_storage_quote(2)
            .ok_or(RunError::Contract("top parameter quotation overflow"))?;
        let bytes = Self::checked(quote.bytes.checked_add(size_of::<Parameters<'db>>()))?;
        let work = Self::checked(quote.work.checked_add(2))?;
        self.local(1, size_of::<Option<Parameters<'db>>>(), || ())
            .await?;
        let mut parameters = None;
        self.local(work, bytes, || parameters = Some(Parameters::top()))
            .await?;
        parameters.ok_or(RunError::Contract("top parameters were not constructed"))
    }

    async fn subclass_instance(&self, subclass: SubclassOfType<'db>) -> RunResult<Type<'db>> {
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::callable_guard::subclass_instance_boundary(
            self.db(),
            self.access.endpoint(),
        )?;
        self.subclass_instance_value(subclass).await
    }

    async fn function_like(
        &self,
        parameters: Parameters<'db>,
        return_type: Type<'db>,
    ) -> RunResult<CallableTypes<'db>> {
        self.local(
            2,
            size_of::<Option<Parameters<'db>>>() + size_of::<Option<CallableSignature<'db>>>(),
            || (),
        )
        .await?;
        let mut parameters = Some(parameters);
        let mut signatures = None;
        self.local(
            8,
            size_of::<Signature<'db>>() + size_of::<CallableSignature<'db>>(),
            || {
                let parameters = parameters
                    .take()
                    .ok_or(RunError::Contract("top parameters already consumed"))?;
                signatures = Some(CallableSignature::single(Signature::new(
                    parameters,
                    return_type,
                )));
                Ok(())
            },
        )
        .await??;
        let signatures = signatures.ok_or(RunError::Contract(
            "subclass callable signature was not constructed",
        ))?;
        let callable = self
            .access
            .owned_callable_type(signatures, CallableTypeKind::FunctionLike, None)
            .await?;
        self.initialize_value(|| CallableTypes::one(callable)).await
    }
}

struct SourceCallMemberEffects<'effects, 'access, 'guard, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    guard: &'guard CallableRecursionGuard<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CallMemberEffects<'db>
    for SourceCallMemberEffects<'_, '_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source
            .work(size_of::<ConversionStep<'db>>() * 2 + 1)
            .await
    }

    async fn lookup(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.source.guarded_callable_member(ty, self.guard).await
    }

    async fn fallback_member(
        &self,
        result: MemberLookupResult<'db>,
    ) -> RunResult<ResolvedMember<'db>> {
        match result {
            Ok(member) => {
                self.source
                    .local(size_of::<ResolvedMember<'db>>() + 1, 0, || member)
                    .await
            }
            Err(error) => {
                self.source
                    .field(
                        error
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .fallback_member(),
                    )
                    .await
            }
        }
    }

    async fn place(&self, member: ResolvedMember<'db>) -> RunResult<Place<'db>> {
        let member = match member {
            ResolvedMember::Plain(member) => member,
            ResolvedMember::WithMetadata(metadata) => {
                self.source
                    .field(
                        metadata
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .member(),
                    )
                    .await?
            }
        };
        self.source
            .local(size_of::<Place<'db>>() + 1, 0, || member.place)
            .await
    }

    async fn origin(&self, member: ResolvedMember<'db>) -> RunResult<DescriptorOrigin<'db>> {
        match member {
            ResolvedMember::Plain(_) => {
                self.source
                    .initialize_value(DescriptorOrigin::default)
                    .await
            }
            ResolvedMember::WithMetadata(metadata) => {
                self.source
                    .field(
                        metadata
                            .field_requests(self.source.access.endpoint().field_request_context())
                            .descriptor(),
                    )
                    .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionConversionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn local<T>(&self, work: Option<usize>, action: impl FnOnce() -> T) -> RunResult<T> {
        SourceEffects::local(self, Self::checked(work)?, 0, action).await
    }

    async fn signature(
        &self,
        _db: &'db dyn Db,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.function_signature(function).await
    }

    async fn callable(
        &self,
        _db: &'db dyn Db,
        signatures: &'db CallableSignature<'db>,
        kind: CallableTypeKind,
    ) -> RunResult<CallableType<'db>> {
        self.access.callable_type(signatures, kind).await
    }
}

enum ConversionFailure {
    Runtime(RunError),
    Unavailable(CallableConversionOperation),
}

impl From<RunError> for ConversionFailure {
    fn from(error: RunError) -> Self {
        Self::Runtime(error)
    }
}

struct SourceConversionAdmission<'access, 'run, 'db> {
    endpoint: &'access TaskEndpoint<'run, 'db>,
}

impl ConversionAdmission for SourceConversionAdmission<'_, '_, '_> {
    type Error = ConversionFailure;

    fn checkpoint(&self) -> Result<(), Self::Error> {
        self.endpoint.admit_work(8)?;
        self.endpoint.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<ConversionStep<'_>>() * 2,
        })?;
        self.endpoint.check_completion()?;
        Ok(())
    }

    fn dependency(&self, operation: CallableConversionOperation) -> Result<(), Self::Error> {
        Err(ConversionFailure::Unavailable(operation))
    }
}
