//! Admitted source execution of the canonical last-definition signature selection.

use std::future::Future;
use std::pin::Pin;

use salsa::execution_probe::{FieldRequest, RunError, RunResult, TaskEndpoint};

use super::local_transfer::local_quoted_with_fixed_transfers_at;
use super::{SourceAccess, SourceEffects};
use crate::Db;
use crate::types::function::last_signature::FunctionLastSignatureEffects;
use crate::types::function::{FunctionLiteral, FunctionType};
use crate::types::signatures::Signature;

/// Funds a local action's fixed factory and result transfers in addition to its supplied payload.
/// Owned captures stay outside the rejecting callback until queued children have drained.
pub(in crate::types::infer) async fn last_signature_local<T, F: FnOnce() -> T>(
    endpoint: &TaskEndpoint<'_, '_>,
    work: Option<usize>,
    bytes: Option<usize>,
    action: F,
) -> RunResult<T> {
    let quote = work.zip(bytes).ok_or(RunError::Contract(
        "last signature local quotation overflow",
    ));
    local_quoted_with_fixed_transfers_at(endpoint, quote, action).await
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits a boxed signature continuation and its factory without transferring owned captures
    /// into a callback that can reject admission before pending children drain.
    pub(in crate::types::infer) async fn last_signature_future<F: Future, M: FnOnce() -> F>(
        &self,
        make: M,
    ) -> RunResult<Pin<Box<F>>> {
        let bytes = size_of::<F::Output>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<F>()));
        last_signature_local(self.access.endpoint(), Some(1), bytes, || Box::pin(make())).await
    }

    /// Fetches the canonical effective last-definition signature after checking its program.
    pub(in crate::types::infer::builder) async fn function_last_definition_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<&'db Signature<'db>> {
        self.check_file_program(self.function_file(function).await?)
            .await?;
        self.last_signature_future(|| self.access.function_last_definition_signature(function))
            .await?
            .await
    }

    /// Executes the shared last-definition query body after checking the function's program.
    pub(in crate::types::infer) async fn infer_function_last_definition_signature(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<Signature<'db>> {
        self.check_file_program(self.function_file(function).await?)
            .await?;
        self.last_signature_future(|| function.last_definition_signature_with(self.db(), self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> FunctionLastSignatureEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn field<R: FieldRequest<'db>>(&self, request: R) -> RunResult<R::Output> {
        self.field(request).await
    }

    async fn local<T>(&self, work: usize, action: impl FnOnce() -> T) -> RunResult<T> {
        last_signature_local(self.access.endpoint(), Some(work), Some(0), action).await
    }

    async fn has_separate_implementation(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> RunResult<bool> {
        self.last_signature_future(|| literal.has_separate_implementation_with(db, self))
            .await?
            .await
    }

    async fn clone_signature(&self, signature: &Signature<'db>) -> RunResult<Signature<'db>> {
        // These quotations inspect fixed metadata and lengths, including the retained constraint
        // arenas. Their work counts initialized entries, not the widths of interned Type handles.
        let (work, bytes) = last_signature_local(self.access.endpoint(), Some(64), Some(0), || {
            (
                signature.retirement_work(),
                signature.clone_requested_bytes(),
            )
        })
        .await?;
        last_signature_local(
            self.access.endpoint(),
            work.and_then(|work| work.checked_add(8)),
            bytes,
            || signature.clone(),
        )
        .await
    }

    async fn definition_signature(
        &self,
        db: &'db dyn Db,
        literal: FunctionLiteral<'db>,
    ) -> RunResult<Signature<'db>> {
        self.last_signature_future(|| literal.last_definition.signature_with(db, self))
            .await?
            .await
    }
}
