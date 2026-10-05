//! Collection from the shared lazy MRO cursor with admitted retained output.

use std::convert::Infallible;

use super::field_reads::MroFieldReads;
use super::iteration::{
    MroCursor, MroDirection, MroIterationEffects, SynchronousMroIterationEffects, mro_next_sync,
    mro_next_with,
};
use super::root::InlineMroRootEffects;
use crate::Db;
use crate::types::class_base::ClassBase;
use crate::types::{ClassLiteral, ClassType};

pub(in crate::types) mod base;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum MroCollectionWork {
    Begin,
    Classify,
    Append { len: usize, capacity: usize },
    BoxOutput { len: usize, capacity: usize },
    Publish,
}

pub(in crate::types) trait MroCollectionEffects<'db>:
    super::iteration::MroIterationEffects<'db>
{
    async fn collection_checkpoint(&self, work: MroCollectionWork) -> Result<(), Self::Error>;
}

pub(in crate::types) trait SynchronousMroCollectionEffects<'db>:
    SynchronousMroIterationEffects<'db>
{
    fn collection_checkpoint(&self, work: MroCollectionWork) -> Result<(), Self::Error>;
}

/// Supplies class-literal conversion and storage admission for forward MRO collection.
pub(in crate::types) trait ClassLiteralCollectionEffects<'db>: MroIterationEffects<'db> {
    async fn literal_collection_checkpoint(&self, work: MroCollectionWork) -> Result<(), Self::Error>;

    async fn class_literal(
        &self,
        fields: MroFieldReads<'db>,
        class: ClassType<'db>,
    ) -> Result<ClassLiteral<'db>, Self::Error>;
}

/// Supplies synchronous class-literal conversion and admission for forward MRO collection.
pub(in crate::types) trait SynchronousClassLiteralCollectionEffects<'db>:
    SynchronousMroCollectionEffects<'db>
{
    fn literal_collection_checkpoint(&self, work: MroCollectionWork) -> Result<(), Self::Error> {
        self.collection_checkpoint(work)
    }

    fn class_literal(
        &self,
        fields: MroFieldReads<'db>,
        class: ClassType<'db>,
    ) -> Result<ClassLiteral<'db>, Self::Error> {
        Ok(fields.class_literal(class))
    }
}

impl<'db, E: SynchronousMroCollectionEffects<'db>> SynchronousClassLiteralCollectionEffects<'db>
    for E
{
}

/// Collects each class entry's literal from the forward MRO, omitting non-class entries.
#[ty_mapping_probe_macros::dual_base_mro]
pub(in crate::types) async fn collect_class_literals_with<'db, E: ClassLiteralCollectionEffects<'db>>(
    fields: MroFieldReads<'db>,
    class: ClassLiteral<'db>,
    effects: &E,
) -> Result<Box<[ClassLiteral<'db>]>, E::Error> {
    effects
        .literal_collection_checkpoint(MroCollectionWork::Begin)
        .await?;
    let mut cursor = MroCursor::new(class, None);
    let mut output = Vec::new();
    while let Some(base) = mro_next_with(fields, &mut cursor, MroDirection::Forward, effects).await? {
        effects
            .literal_collection_checkpoint(MroCollectionWork::Classify)
            .await?;
        if let ClassBase::Class(class) = base {
            effects
                .literal_collection_checkpoint(MroCollectionWork::Append {
                    len: output.len(),
                    capacity: output.capacity(),
                })
                .await?;
            output.push(effects.class_literal(fields, class).await?);
        }
    }
    effects
        .literal_collection_checkpoint(MroCollectionWork::BoxOutput {
            len: output.len(),
            capacity: output.capacity(),
        })
        .await?;
    let output = output.into_boxed_slice();
    effects
        .literal_collection_checkpoint(MroCollectionWork::Publish)
        .await?;
    Ok(output)
}

impl<'db> SynchronousMroCollectionEffects<'db> for InlineMroRootEffects<'db> {
    #[inline]
    fn collection_checkpoint(&self, _: MroCollectionWork) -> Result<(), Infallible> {
        Ok(())
    }
}
