//! Admits descriptor provenance through the current invocation and canonical interners.

use salsa::execution_probe::{FieldRequest, RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation, storage};
use crate::types::call::bind::origin::OriginEffects;
use crate::types::call::{Bindings, CallableBinding};
use crate::types::descriptor::effects::DescriptorOperation;
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::{
    DescriptorArgumentComparison, DescriptorDispatch, DescriptorDispatches, DescriptorOrigin, Type,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> OriginEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn origin_local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = Self::checked(work)
            .and_then(|work| Self::checked(bytes).map(|bytes| (work, bytes)));
        self.local_quoted(quote, action).await
    }

    async fn origin_field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        self.field(request).await
    }

    async fn clone_signature(&self, signature: &Signature<'db>) -> RunResult<Signature<'db>> {
        let (work, bytes) = self
            .local(16, 0, || {
                (
                    signature
                        .retirement_work()
                        .and_then(|work| work.checked_add(8)),
                    signature
                        .clone_requested_bytes()
                        .and_then(|bytes| bytes.checked_add(size_of::<Signature<'db>>())),
                )
            })
            .await?;
        self.local(Self::checked(work)?, Self::checked(bytes)?, || {
            signature.clone()
        })
        .await
    }

    async fn dispatch(
        &self,
        _db: &'db dyn Db,
        signatures: CallableSignature<'db>,
        arguments: Box<[Type<'db>]>,
        comparisons: Box<[Box<[DescriptorArgumentComparison<'db>]>]>,
        selected: Box<[usize]>,
        failed: bool,
    ) -> RunResult<DescriptorDispatch<'db>> {
        self.access
            .descriptor_dispatch(signatures, arguments, comparisons, selected, failed)
            .await
    }

    async fn callable_origin(
        &self,
        db: &'db dyn Db,
        callable: &CallableBinding<'db>,
        arguments: &[Type<'db>],
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.allocate_future(|| callable.descriptor_origin_with(db, arguments, self))
            .await?
            .await
    }

    async fn merge_origin(
        &self,
        db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.allocate_future(|| left.merge_with(db, right, self))
            .await?
            .await
    }

    async fn dispatches(
        &self,
        _db: &'db dyn Db,
        elements: Box<[DescriptorDispatch<'db>]>,
    ) -> RunResult<DescriptorDispatches<'db>> {
        self.access.descriptor_dispatches(elements).await
    }

    async fn insert_dispatch(
        &self,
        elements: &mut FxOrderSet<DescriptorDispatch<'db>>,
        dispatch: DescriptorDispatch<'db>,
    ) -> RunResult<()> {
        let (len, capacity) = self
            .local(2, 0, || (elements.len(), elements.capacity()))
            .await?;
        let mut quote = storage::ordered_merge::<DescriptorDispatch<'db>>(len, capacity, 1).ok_or(
            RunError::Contract("descriptor origin set quotation overflow"),
        )?;
        quote.work = Self::checked(
            quote
                .work
                .checked_add(Self::checked(storage::slots(capacity))?),
        )?;
        self.local(quote.work, quote.bytes, || {
            if len == capacity {
                elements.reserve_exact(capacity.max(1));
            }
            elements.insert(dispatch);
        })
        .await
    }

    async fn finish_dispatches(
        &self,
        elements: FxOrderSet<DescriptorDispatch<'db>>,
    ) -> RunResult<Box<[DescriptorDispatch<'db>]>> {
        let (len, capacity) = self
            .local(2, 0, || (elements.len(), elements.capacity()))
            .await?;
        let quote = storage::dense_finish::<DescriptorDispatch<'db>>(len, capacity)
            .map(|quote| (quote.work, quote.bytes))
            .ok_or(RunError::Contract(
                "descriptor origin result quotation overflow",
            ));
        self.local_quoted(quote, || elements.into_iter().collect())
            .await
    }

    async fn downstream_origin(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: &Bindings<'db>,
        _arguments: &[Type<'db>],
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::BindingsOrigin,
        ))
        .await
    }

    async fn origin_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| bindings.return_type_with(db, env, self))
            .await?
            .await
    }

    async fn restrict_origin(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _origin: DescriptorOrigin<'db>,
        _return_type: Type<'db>,
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::BindingsOrigin,
        ))
        .await
    }
}
