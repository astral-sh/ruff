//! Admitted function reconstruction after inheriting a constructor's class context.

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::FixedFieldBorrow;
use super::{FixedFieldCopy, SourceAccess, SourceEffects};
use crate::types::callable::{CallableType, CallableTypeKind};
#[cfg(test)]
use crate::types::function::inherited_context::observations::{self, Stage};
use crate::types::function::inherited_context::{
    FunctionInheritedContextEffects, ImplementationInput, stored_implementations, updated_request,
    updated_signatures, with_inherited_generic_context_with,
};
use crate::types::function::{FunctionLiteral, FunctionType, UpdatedFunctionSignatures};
use crate::types::generics::GenericContext;
use crate::types::local_transfer::generated_field_quote;
use crate::types::mapping::source::MappingSourceEffects;
use crate::types::signatures::CallableSignature;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Inherits the class context into the function's effective public and implementation signatures.
    pub(super) async fn inherit_function_generic_context(
        &self,
        function: FunctionType<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<FunctionType<'db>> {
        #[cfg(test)]
        let _observation = observations::OperationLifetime::new(self.db(), function);
        self.type_parameter_future(|| with_inherited_generic_context_with(function, context, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionInheritedContextEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        let child = MappingSourceEffects::function_signature(self, function);
        #[cfg(test)]
        let child = observations::observe_child(self.db(), child);
        child.await
    }

    async fn inherit(
        &self,
        signature: &CallableSignature<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<CallableSignature<'db>> {
        self.type_parameter_future(|| self.inherit_callable_generic_context(signature, context))
            .await?
            .await
    }

    async fn literal(&self, function: FunctionType<'db>) -> RunResult<FunctionLiteral<'db>> {
        let quote = generated_field_quote(
            |function: FunctionType<'db>, fields| function.field_requests(fields),
            |function: FunctionType<'db>, fields| function.field_requests(fields).literal(),
        );
        let endpoint = self.access.endpoint();
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                let request = function
                    .field_requests(endpoint.field_request_context())
                    .literal();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        Ok(read.await)
    }

    async fn has_separate_implementation(&self, literal: FunctionLiteral<'db>) -> RunResult<bool> {
        let child = MappingSourceEffects::function_has_separate_implementation(self, literal);
        #[cfg(test)]
        let child = observations::observe_child(self.db(), child);
        child.await
    }

    async fn stored_implementations(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<Option<ImplementationInput<'db>>> {
        let quote = generated_field_quote(
            |function: FunctionType<'db>, fields| function.field_requests(fields),
            |function: FunctionType<'db>, fields| updated_request(function, fields),
        );
        let endpoint = self.access.endpoint();
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                let request = updated_request(function, endpoint.field_request_context());
                endpoint.read_field(request, &FixedFieldBorrow)
            })
            .await?;
        let updated = read.await;
        self.local_with_fixed_transfers(10, 4 * size_of::<Option<&[CallableType<'db>]>>(), || {
            stored_implementations(updated)
        })
        .await
    }

    async fn fallback_implementation(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<ImplementationInput<'db>> {
        let child = MappingSourceEffects::function_implementation_callable(self, function);
        #[cfg(test)]
        let child = observations::observe_child(self.db(), child);
        let callable = child.await?;
        self.local_with_fixed_transfers(1, 0, || ImplementationInput::Fallback(callable))
            .await
    }

    async fn new_output(
        &self,
        input: ImplementationInput<'db>,
    ) -> RunResult<Vec<CallableType<'db>>> {
        let (count, quote) = self
            .local_with_fixed_transfers(
                32,
                32 * (size_of::<usize>() + size_of::<Option<usize>>()),
                || {
                    let count = input.len();
                    let layout = Layout::array::<CallableType<'db>>(count).map_err(|_| {
                        RunError::Contract("inherited callable list layout overflow")
                    })?;
                    let work = count
                        .checked_mul(2)
                        .and_then(|work| work.checked_add(8))
                        .ok_or(RunError::Contract("inherited callable list work overflow"))?;
                    let bytes = layout
                        .size()
                        .checked_add(2 * size_of::<Vec<CallableType<'db>>>())
                        .ok_or(RunError::Contract("inherited callable list bytes overflow"))?;
                    Ok((count, (work, bytes)))
                },
            )
            .await??;
        self.local_quoted_with_fixed_transfers(Ok(quote), || Vec::with_capacity(count))
            .await
    }

    async fn next_callable(
        &self,
        input: ImplementationInput<'db>,
        cursor: &mut usize,
    ) -> RunResult<Option<CallableType<'db>>> {
        self.local_with_fixed_transfers(
            12,
            2 * size_of::<Option<CallableType<'db>>>() + 4 * size_of::<usize>(),
            || input.next(cursor),
        )
        .await
    }

    async fn callable_signatures(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        let quote = generated_field_quote(
            |callable: CallableType<'db>, fields| callable.field_requests(fields),
            |callable: CallableType<'db>, fields| callable.field_requests(fields).signatures(),
        );
        let endpoint = self.access.endpoint();
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                let request = callable
                    .field_requests(endpoint.field_request_context())
                    .signatures();
                endpoint.read_field(request, &FixedFieldBorrow)
            })
            .await?;
        Ok(read.await)
    }

    async fn replace_callable(
        &self,
        callable: CallableType<'db>,
        signatures: CallableSignature<'db>,
    ) -> RunResult<CallableType<'db>> {
        let quote = generated_field_quote(
            |callable: CallableType<'db>, fields| callable.field_requests(fields),
            |callable: CallableType<'db>, fields| callable.field_requests(fields).kind(),
        );
        let endpoint = self.access.endpoint();
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                let request = callable
                    .field_requests(endpoint.field_request_context())
                    .kind();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        let kind = read.await;
        let quote = generated_field_quote(
            |callable: CallableType<'db>, fields| callable.field_requests(fields),
            |callable: CallableType<'db>, fields| callable.field_requests(fields).deprecated(),
        );
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                let request = callable
                    .field_requests(endpoint.field_request_context())
                    .deprecated();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        let deprecated = read.await;
        MappingSourceEffects::owned_mapped_callable(self, signatures, kind, deprecated).await
    }

    async fn push(
        &self,
        output: &mut Vec<CallableType<'db>>,
        callable: CallableType<'db>,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(6, 0, || {
            if output.len() == output.capacity() {
                return Err(RunError::Contract("inherited callable capacity exhausted"));
            }
            output.push(callable);
            Ok(())
        })
        .await?
    }

    async fn finish_output(
        &self,
        output: Vec<CallableType<'db>>,
    ) -> RunResult<Box<[CallableType<'db>]>> {
        let quote = self
            .local_with_fixed_transfers(
                32,
                32 * (size_of::<usize>() + size_of::<Option<usize>>()),
                || {
                    let len = output.len();
                    let capacity = output.capacity();
                    let (work, bytes) = if len == capacity {
                        (8, 0)
                    } else {
                        let layout = Layout::array::<CallableType<'db>>(len).map_err(|_| {
                            RunError::Contract("inherited callable box layout overflow")
                        })?;
                        let work = len
                            .checked_mul(2)
                            .and_then(|work| work.checked_add(capacity))
                            .and_then(|work| work.checked_add(12))
                            .ok_or(RunError::Contract("inherited callable box work overflow"))?;
                        (work, layout.size())
                    };
                    let bytes = bytes
                        .checked_add(
                            2 * size_of::<Vec<CallableType<'db>>>()
                                + 2 * size_of::<Box<[CallableType<'db>]>>(),
                        )
                        .ok_or(RunError::Contract("inherited callable box bytes overflow"))?;
                    Ok((work, bytes))
                },
            )
            .await?;
        self.local_quoted_with_fixed_transfers(quote, || output.into_boxed_slice())
            .await
    }

    async fn updated(
        &self,
        signature: CallableSignature<'db>,
        implementations: Option<Box<[CallableType<'db>]>>,
    ) -> RunResult<Option<Box<UpdatedFunctionSignatures<'db>>>> {
        // Signature and list producers have prepaid their descendants' retirement. This admission
        // covers the new aggregate and box; the helper retains both inputs through refusal.
        let bytes = size_of::<UpdatedFunctionSignatures<'db>>()
            + 2 * size_of::<Option<CallableSignature<'db>>>()
            + 2 * size_of::<Option<Box<[CallableType<'db>]>>>();
        #[cfg(test)]
        observations::record(self.db(), Stage::BeforeUpdated);
        self.local_with_fixed_transfers(16, bytes, || {
            let updated = updated_signatures(signature, implementations);
            #[cfg(test)]
            observations::record(self.db(), Stage::AfterUpdated);
            updated
        })
        .await
    }

    async fn descriptor_kind(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<Option<CallableTypeKind>> {
        let quote = generated_field_quote(
            |function: FunctionType<'db>, fields| function.field_requests(fields),
            |function: FunctionType<'db>, fields| function.field_requests(fields).descriptor_kind(),
        );
        let endpoint = self.access.endpoint();
        let read = self
            .boxed_future_with_fixed_transfers(quote, || {
                let request = function
                    .field_requests(endpoint.field_request_context())
                    .descriptor_kind();
                endpoint.read_field(request, &FixedFieldCopy)
            })
            .await?;
        Ok(read.await)
    }

    async fn intern(
        &self,
        literal: FunctionLiteral<'db>,
        updated: Option<Box<UpdatedFunctionSignatures<'db>>>,
        descriptor: Option<CallableTypeKind>,
    ) -> RunResult<FunctionType<'db>> {
        #[cfg(test)]
        {
            observations::record(self.db(), Stage::BeforeIntern);
            let result =
                MappingSourceEffects::intern_mapped_function(self, literal, updated, descriptor)
                    .await?;
            observations::record(self.db(), Stage::AfterIntern);
            Ok(result)
        }
        #[cfg(not(test))]
        MappingSourceEffects::intern_mapped_function(self, literal, updated, descriptor).await
    }
}
