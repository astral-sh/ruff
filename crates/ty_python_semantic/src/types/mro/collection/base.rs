//! Direct collection of fixed or lazy base MRO entries.

use std::collections::VecDeque;

use super::{MroCollectionEffects, MroCollectionWork};
use crate::Db;
use crate::types::ClassType;
use crate::types::class_base::ClassBase;
use crate::types::mro::Mro;
use crate::types::mro::base::BaseMroStart;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{
    MroCursor, MroDirection, MroIterationEffects, MroIterationWork, SynchronousMroIterationEffects,
    mro_next_sync, mro_next_with,
};

#[cfg(test)]
mod tests;

pub(in crate::types::mro) enum BaseCursor<'db> {
    Length2(std::array::IntoIter<ClassBase<'db>, 2>),
    Length3(std::array::IntoIter<ClassBase<'db>, 3>),
    Class(MroCursor<'db>),
}

impl<'db> BaseCursor<'db> {
    pub(in crate::types::mro) fn new(start: BaseMroStart<'db>) -> Self {
        match start {
            BaseMroStart::Length2(entries) => Self::Length2(entries.into_iter()),
            BaseMroStart::Length3(entries) => Self::Length3(entries.into_iter()),
            BaseMroStart::Class(start) => {
                Self::Class(MroCursor::new(start.class, start.specialization))
            }
        }
    }

    pub(in crate::types::mro) fn next<E: SynchronousMroIterationEffects<'db>>(
        &mut self,
        db: &'db dyn Db,
        effects: &E,
    ) -> Result<Option<ClassBase<'db>>, E::Error> {
        base_cursor_next_sync(db, self, effects)
    }
}

#[ty_mapping_probe_macros::dual_base_mro]
pub(in crate::types::mro) async fn base_cursor_next_with<'db, E: MroIterationEffects<'db>>(
    fields: MroFieldReads<'db>,
    cursor: &mut BaseCursor<'db>,
    effects: &E,
) -> Result<Option<ClassBase<'db>>, E::Error> {
    match cursor {
        BaseCursor::Length2(entries) => {
            effects
                .iteration_checkpoint(MroIterationWork::Advance)
                .await?;
            Ok(entries.next())
        }
        BaseCursor::Length3(entries) => {
            effects
                .iteration_checkpoint(MroIterationWork::Advance)
                .await?;
            Ok(entries.next())
        }
        BaseCursor::Class(cursor) => {
            mro_next_with(fields, cursor, MroDirection::Forward, effects).await
        }
    }
}

#[ty_mapping_probe_macros::dual_base_mro]
pub(in crate::types) async fn collect_start_with<'db, E: MroCollectionEffects<'db>>(
    fields: MroFieldReads<'db>,
    start: BaseMroStart<'db>,
    effects: &E,
) -> Result<VecDeque<ClassBase<'db>>, E::Error> {
    let mut cursor = BaseCursor::new(start);
    let mut output = VecDeque::new();
    while let Some(base) = base_cursor_next_with(fields, &mut cursor, effects).await? {
        effects
            .collection_checkpoint(MroCollectionWork::Append {
                len: output.len(),
                capacity: output.capacity(),
            })
            .await?;
        output.push_back(base);
    }
    effects
        .collection_checkpoint(MroCollectionWork::Publish)
        .await?;
    Ok(output)
}

#[ty_mapping_probe_macros::dual_base_mro]
pub(in crate::types) async fn collect_start_with_root_with<'db, E: MroCollectionEffects<'db>>(
    fields: MroFieldReads<'db>,
    root: ClassType<'db>,
    start: BaseMroStart<'db>,
    effects: &E,
) -> Result<Mro<'db>, E::Error> {
    let mut output = Vec::new();
    effects
        .collection_checkpoint(MroCollectionWork::Append {
            len: output.len(),
            capacity: output.capacity(),
        })
        .await?;
    output.push(ClassBase::Class(root));

    let mut cursor = BaseCursor::new(start);
    while let Some(base) = base_cursor_next_with(fields, &mut cursor, effects).await? {
        effects
            .collection_checkpoint(MroCollectionWork::Append {
                len: output.len(),
                capacity: output.capacity(),
            })
            .await?;
        output.push(base);
    }
    effects
        .collection_checkpoint(MroCollectionWork::BoxOutput {
            len: output.len(),
            capacity: output.capacity(),
        })
        .await?;
    let output = Mro::from(output);
    effects
        .collection_checkpoint(MroCollectionWork::Publish)
        .await?;
    Ok(output)
}
