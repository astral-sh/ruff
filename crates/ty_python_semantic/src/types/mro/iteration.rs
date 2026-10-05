//! Lazy MRO iteration with separate first-entry and full-MRO dependencies.

use crate::types::mro::field_reads::MroFieldReads;

use std::convert::Infallible;

use super::Mro;
use super::root::{
    InlineMroRootEffects, MroRootEffects, MroTailRequest, SynchronousMroRootEffects,
    mro_first_sync, mro_first_with, mro_tail_request_sync, mro_tail_request_with,
};
use crate::Db;
use crate::types::ClassLiteral;
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::source_read::{SourceReadControl, UnrestrictedSourceRead, read_source};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum MroDirection {
    Forward,
    Reverse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum MroIterationWork {
    Advance,
    First,
    TailRequest,
    FullMro,
}

#[derive(Clone)]
pub(in crate::types) struct MroCursor<'db> {
    class: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    first_element_yielded: bool,
    // The full MRO stays unevaluated until iteration needs an entry after the first.
    pub(super) subsequent_elements: Option<std::slice::Iter<'db, ClassBase<'db>>>,
}

impl<'db> MroCursor<'db> {
    pub(in crate::types) fn new(
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        Self {
            class,
            specialization,
            first_element_yielded: false,
            subsequent_elements: None,
        }
    }
}

pub(in crate::types) trait MroIterationEffects<'db>: MroRootEffects<'db> {
    async fn iteration_checkpoint(&self, work: MroIterationWork) -> Result<(), Self::Error>;

    async fn full_mro(&self, request: MroTailRequest<'db>) -> Result<&'db Mro<'db>, Self::Error>;
}

pub(in crate::types) trait SynchronousMroIterationEffects<'db>:
    SynchronousMroRootEffects<'db>
{
    fn iteration_checkpoint(&self, work: MroIterationWork) -> Result<(), Self::Error>;

    fn full_mro(&self, request: MroTailRequest<'db>) -> Result<&'db Mro<'db>, Self::Error>;
}

#[ty_mapping_probe_macros::dual_mro_iteration]
#[inline]
pub(in crate::types) async fn mro_next_with<'db, E: MroIterationEffects<'db>>(
    fields: MroFieldReads<'db>,
    cursor: &mut MroCursor<'db>,
    direction: MroDirection,
    effects: &E,
) -> Result<Option<ClassBase<'db>>, E::Error> {
    effects
        .iteration_checkpoint(MroIterationWork::Advance)
        .await?;
    if direction == MroDirection::Forward && !cursor.first_element_yielded {
        effects
            .iteration_checkpoint(MroIterationWork::First)
            .await?;
        cursor.first_element_yielded = true;
        return Ok(Some(
            mro_first_with(fields, cursor.class, cursor.specialization, effects).await?,
        ));
    }

    let next = loop {
        if let Some(tail) = &mut cursor.subsequent_elements {
            break match direction {
                MroDirection::Forward => tail.next(),
                MroDirection::Reverse => tail.next_back(),
            }
            .copied();
        }

        effects
            .iteration_checkpoint(MroIterationWork::TailRequest)
            .await?;
        let request =
            mro_tail_request_with(fields, cursor.class, cursor.specialization, effects).await?;
        effects
            .iteration_checkpoint(MroIterationWork::FullMro)
            .await?;
        let mut full_mro = effects.full_mro(request).await?.iter();
        full_mro.next();
        cursor.subsequent_elements = Some(full_mro);
    };

    if next.is_some() || cursor.first_element_yielded {
        return Ok(next);
    }

    // Reverse iteration resolves the tail before requesting the separately specialized first
    // entry. Forward and reverse consumption share both pieces of exhaustion state.
    effects
        .iteration_checkpoint(MroIterationWork::First)
        .await?;
    cursor.first_element_yielded = true;
    Ok(Some(
        mro_first_with(fields, cursor.class, cursor.specialization, effects).await?,
    ))
}

pub(in crate::types) fn full_mro_with<'db, C: SourceReadControl>(
    db: &'db dyn Db,
    request: MroTailRequest<'db>,
    control: &C,
) -> Result<&'db Mro<'db>, C::Error> {
    Ok(match request {
        MroTailRequest::Static(literal, specialization) => {
            read_source(control, || literal.try_mro(db, specialization))?
                .unwrap_or_else(|error| error.fallback_mro())
        }
        MroTailRequest::Dynamic(literal) => read_source(control, || literal.try_mro(db))?
            .as_ref()
            .unwrap_or_else(|error| error.fallback_mro()),
        MroTailRequest::DynamicNamedTuple(literal) => read_source(control, || literal.mro(db))?,
        MroTailRequest::DynamicTypedDict(literal) => read_source(control, || literal.mro(db))?,
        MroTailRequest::DynamicEnum(literal) => read_source(control, || literal.try_mro(db))?
            .as_ref()
            .unwrap_or_else(|error| error.fallback_mro()),
    })
}

impl<'db> SynchronousMroIterationEffects<'db> for InlineMroRootEffects<'db> {
    #[inline]
    fn iteration_checkpoint(&self, _work: MroIterationWork) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn full_mro(&self, request: MroTailRequest<'db>) -> Result<&'db Mro<'db>, Infallible> {
        full_mro_with(self.db, request, &UnrestrictedSourceRead)
    }
}
