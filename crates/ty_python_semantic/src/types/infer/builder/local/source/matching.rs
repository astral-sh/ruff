//! Source argument matching borrows the invocation's existing budget and binding owners.

use super::*;
use crate::types::call::bind::parameter_matching::{
    MatchingDependency, MatchingQuote, ParameterMatchingEffects,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ParameterMatchingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn matching_local<T>(
        &self,
        quote: Option<MatchingQuote>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = quote.ok_or(RunError::Contract("parameter matching quotation overflow"))?;
        self.local_with_fixed_transfers(quote.work, quote.requested_bytes, action).await
    }

    async fn dependency<T>(
        &self,
        dependency: MatchingDependency,
        _action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.unavailable(match dependency {
            MatchingDependency::GenericFreshening => SourceOperation::CallGenericFreshening,
            MatchingDependency::Variadic => SourceOperation::CallVariadicMatching,
            MatchingDependency::Keywords => SourceOperation::CallKeywordMatching,
            MatchingDependency::UnpackedVariadic => SourceOperation::CallUnpackedMatching,
        })
        .await
    }

    async fn bound_arguments(
        &self,
        arguments: &CallArguments<'_, 'db>,
        bound_type: Option<Type<'db>>,
        action: impl FnOnce(),
    ) -> RunResult<()> {
        let quote = self
            .local_quoted_with_fixed_transfers(
                arguments.len().checked_mul(12).and_then(|work| work.checked_add(64))
                    .map(|work| (work, 0))
                    .ok_or(RunError::Contract("bound argument scan quotation overflow")),
                || {
                arguments.with_self_storage_quote(bound_type)
            })
            .await?;
        let (work, requested_bytes) =
            quote.ok_or(RunError::Contract("bound arguments quotation overflow"))?;
        self.local_with_fixed_transfers(work, requested_bytes, action).await
    }
}
