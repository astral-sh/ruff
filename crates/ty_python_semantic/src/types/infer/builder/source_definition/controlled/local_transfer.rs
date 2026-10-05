//! Admission for fixed local callback and result transfers, in addition to an operation's payload.

use std::future::Future;
use std::pin::Pin;

use salsa::execution_probe::RunResult;

use super::{SourceAccess, SourceEffects};
pub(in crate::types) use crate::types::local_transfer::{
    boxed_future_with_fixed_transfers_at, local_quoted_with_fixed_transfers_at,
    local_with_fixed_transfers_at,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Constructs and returns a boxed child future after admitting its fixed storage and transfers.
    /// See [`boxed_future_with_fixed_transfers_at`] for the quotation and capture-retention contract.
    pub(in crate::types::infer) async fn boxed_future_with_fixed_transfers<
        F: Future,
        M: FnOnce() -> F,
    >(
        &self,
        quote: RunResult<(usize, usize)>,
        make: M,
    ) -> RunResult<Pin<Box<F>>> {
        boxed_future_with_fixed_transfers_at(self.access.endpoint(), quote, make).await
    }

    pub(in crate::types::infer) async fn local_with_fixed_transfers<
        T,
        F: FnOnce() -> T,
    >(
        &self,
        work: usize,
        requested_bytes: usize,
        action: F,
    ) -> RunResult<T> {
        local_with_fixed_transfers_at(self.access.endpoint(), work, requested_bytes, action).await
    }

    pub(in crate::types::infer) async fn local_quoted_with_fixed_transfers<
        T,
        F: FnOnce() -> T,
    >(
        &self,
        quote: RunResult<(usize, usize)>,
        action: F,
    ) -> RunResult<T> {
        local_quoted_with_fixed_transfers_at(self.access.endpoint(), quote, action).await
    }
}
