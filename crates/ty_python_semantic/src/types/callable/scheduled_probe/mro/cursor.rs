//! Lazy prepared MRO iteration shared by member lookup and static MRO collection.

use super::{PreparedMroRootEffects, PreparedMroWork, StaticMroResultId};
use crate::Db;
use crate::types::callable::scheduled_probe::Boundary;
use crate::types::callable::scheduled_probe::member_lookup::{LookupFailure, LookupOperation};
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::mro::base::BaseMroStart;
use crate::types::mro::root::{MroTailRequest, mro_first_with, mro_tail_request_with};
use crate::types::{ClassLiteral, StaticClassLiteral};

#[derive(Clone, Copy)]
enum PreparedMroPhase {
    First,
    NeedProper,
    PreparedTail {
        index: usize,
    },
    ComputedTail {
        result: StaticMroResultId,
        index: usize,
    },
    Done,
}

pub(in crate::types::callable::scheduled_probe) struct PreparedMroCursor<'db> {
    class: ClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
    phase: PreparedMroPhase,
}

impl<'db> PreparedMroCursor<'db> {
    pub(in crate::types::callable::scheduled_probe) fn new(
        class: ClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Self {
        Self {
            class,
            specialization,
            phase: PreparedMroPhase::First,
        }
    }

    pub(in crate::types::callable::scheduled_probe) async fn advance(
        &mut self,
        db: &'db dyn Db,
        work: &PreparedMroWork<'_, 'db, '_>,
    ) -> Result<Option<ClassBase<'db>>, LookupFailure<'db>> {
        work.checkpoint(8).await?;
        let effects = PreparedMroRootEffects::new(db, work);
        match self.phase {
            PreparedMroPhase::First => {
                let first = mro_first_with(
                    crate::types::mro::field_reads::MroFieldReads::new(db),
                    self.class,
                    self.specialization,
                    &effects,
                )
                .await?;
                self.phase = PreparedMroPhase::NeedProper;
                Ok(Some(first))
            }
            PreparedMroPhase::NeedProper => {
                match mro_tail_request_with(
                    crate::types::mro::field_reads::MroFieldReads::new(db),
                    self.class,
                    self.specialization,
                    &effects,
                )
                .await?
                {
                    MroTailRequest::Static(class, None) => {
                        self.advance_prepared_tail(work, class, 0)
                    }
                    MroTailRequest::Static(class, specialization @ Some(_)) => {
                        let result = work
                            .router()
                            .demand_static_mro(db, work, class, specialization)
                            .await?;
                        // The separately yielded first entry retains its original specialization.
                        self.advance_computed_tail(work, result, 1).await
                    }
                    MroTailRequest::Dynamic(_) => Err(LookupFailure::Unsupported(
                        LookupOperation::DynamicProperMro,
                    )),
                    MroTailRequest::DynamicNamedTuple(_) => Err(LookupFailure::Unsupported(
                        LookupOperation::DynamicNamedTupleProperMro,
                    )),
                    MroTailRequest::DynamicTypedDict(_) => Err(LookupFailure::Unsupported(
                        LookupOperation::DynamicTypedDictProperMro,
                    )),
                    MroTailRequest::DynamicEnum(_) => Err(LookupFailure::Unsupported(
                        LookupOperation::DynamicEnumProperMro,
                    )),
                }
            }
            PreparedMroPhase::PreparedTail { index } => {
                let class = self.class.as_static().ok_or(Boundary::SourceSupport)?;
                self.advance_prepared_tail(work, class, index)
            }
            PreparedMroPhase::ComputedTail { result, index } => {
                self.advance_computed_tail(work, result, index).await
            }
            PreparedMroPhase::Done => Ok(None),
        }
    }

    fn advance_prepared_tail(
        &mut self,
        work: &PreparedMroWork<'_, 'db, '_>,
        class: StaticClassLiteral<'db>,
        index: usize,
    ) -> Result<Option<ClassBase<'db>>, LookupFailure<'db>> {
        let next = work
            .router()
            .declarations
            .as_deref()
            .ok_or(Boundary::SourcePreparation)?
            .proper_mro(class)?
            .get(index)
            .copied();
        self.phase = if next.is_some() {
            PreparedMroPhase::PreparedTail {
                index: index.checked_add(1).ok_or(Boundary::CostOverflow)?,
            }
        } else {
            PreparedMroPhase::Done
        };
        Ok(next)
    }

    async fn advance_computed_tail(
        &mut self,
        work: &PreparedMroWork<'_, 'db, '_>,
        result: StaticMroResultId,
        index: usize,
    ) -> Result<Option<ClassBase<'db>>, LookupFailure<'db>> {
        let next = work.router().static_mro_entry(work, result, index).await?;
        self.phase = if next.is_some() {
            PreparedMroPhase::ComputedTail {
                result,
                index: index.checked_add(1).ok_or(Boundary::CostOverflow)?,
            }
        } else {
            PreparedMroPhase::Done
        };
        Ok(next)
    }
}

pub(super) enum PreparedBaseMroCursor<'db> {
    Length2(std::array::IntoIter<ClassBase<'db>, 2>),
    Length3(std::array::IntoIter<ClassBase<'db>, 3>),
    Class(PreparedMroCursor<'db>),
}

impl<'db> PreparedBaseMroCursor<'db> {
    pub(super) fn new(start: BaseMroStart<'db>) -> Self {
        match start {
            BaseMroStart::Length2(entries) => Self::Length2(entries.into_iter()),
            BaseMroStart::Length3(entries) => Self::Length3(entries.into_iter()),
            BaseMroStart::Class(start) => {
                Self::Class(PreparedMroCursor::new(start.class, start.specialization))
            }
        }
    }

    pub(super) async fn advance(
        &mut self,
        db: &'db dyn Db,
        work: &PreparedMroWork<'_, 'db, '_>,
    ) -> Result<Option<ClassBase<'db>>, LookupFailure<'db>> {
        match self {
            Self::Length2(entries) => {
                work.checkpoint(8).await?;
                Ok(entries.next())
            }
            Self::Length3(entries) => {
                work.checkpoint(8).await?;
                Ok(entries.next())
            }
            Self::Class(cursor) => cursor.advance(db, work).await,
        }
    }
}
