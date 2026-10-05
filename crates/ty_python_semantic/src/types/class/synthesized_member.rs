//! Shared selection of canonical class-member synthesis operations.

use super::own_member::OwnMemberLookupRequest;
use super::{CodeGeneratorKind, FrozenDataclassMethod, StaticClassLiteral};
use crate::types::Type;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum SynthesizedMemberWork {
    Admission { name_bytes: usize },
    OrderingRequest,
    FrozenRequest,
    CodeGenerator,
    GeneratedRequest,
    Publish,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

/// Selecting a producer requests its complete canonical computation.
pub(in crate::types) trait SynthesizedMemberEffects<'db>: sealed::Sealed {
    async fn total_ordering(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    type Error;
    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;

    async fn checkpoint(&self, work: SynthesizedMemberWork) -> Result<(), Self::Error>;
    async fn total_ordering_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn frozen_subclass_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        method: FrozenDataclassMethod,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    async fn generated_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
}

pub(in crate::types) trait SynchronousSynthesizedMemberEffects<'db>:
    sealed::Sealed
{
    fn total_ordering(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error>;
    type Error;
    fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error>;

    fn checkpoint(&self, work: SynthesizedMemberWork) -> Result<(), Self::Error>;
    fn total_ordering_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    fn frozen_subclass_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        method: FrozenDataclassMethod,
    ) -> Result<Option<Type<'db>>, Self::Error>;
    fn generated_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
        field_policy: CodeGeneratorKind<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_synthesized_member]
#[inline]
pub(in crate::types) async fn own_synthesized_member_with<
    'a,
    'db,
    E: SynthesizedMemberEffects<'db>,
>(
    request: OwnMemberLookupRequest<'a, 'db>,
    effects: &E,
) -> Result<Option<Type<'db>>, E::Error> {
    effects
        .checkpoint(SynthesizedMemberWork::Admission {
            name_bytes: request.name.len(),
        })
        .await?;

    // Handle `@functools.total_ordering`: synthesize comparison methods
    // for classes that have `@total_ordering` and define at least one
    // ordering method. The decorator requires at least one of __lt__,
    // __le__, __gt__, or __ge__ to be defined (either in this class or
    // inherited from a superclass, excluding `object`).
    if effects.total_ordering(request.class).await?
        && matches!(request.name, "__lt__" | "__le__" | "__gt__" | "__ge__")
    {
        effects
            .checkpoint(SynthesizedMemberWork::OrderingRequest)
            .await?;
        if let Some(member) = effects.total_ordering_member(request).await? {
            effects.checkpoint(SynthesizedMemberWork::Publish).await?;
            return Ok(Some(member));
        }
    }

    // An ordinary subclass of a frozen dataclass is not itself dataclass-like, so the
    // `CodeGeneratorKind::from_class` check below would return `None` before dataclass-like
    // synthesis runs. Still, an instance of such a subclass inherits the frozen dataclass's
    // generated `__setattr__` and `__delattr__`, which reject assignments and deletions of
    // frozen base fields.
    if let Some(method) = FrozenDataclassMethod::from_name(request.name) {
        effects
            .checkpoint(SynthesizedMemberWork::FrozenRequest)
            .await?;
        if let Some(member) = effects.frozen_subclass_member(request, method).await? {
            effects.checkpoint(SynthesizedMemberWork::Publish).await?;
            return Ok(Some(member));
        }
    }

    effects
        .checkpoint(SynthesizedMemberWork::CodeGenerator)
        .await?;
    let Some(field_policy) = effects.code_generator(request.class).await? else {
        effects.checkpoint(SynthesizedMemberWork::Publish).await?;
        return Ok(None);
    };
    effects
        .checkpoint(SynthesizedMemberWork::GeneratedRequest)
        .await?;
    let member = effects.generated_member(request, field_policy).await?;
    effects.checkpoint(SynthesizedMemberWork::Publish).await?;
    Ok(member)
}
