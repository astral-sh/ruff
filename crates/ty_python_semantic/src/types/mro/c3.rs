//! Shared C3 merging with admitted scans, mutations, and result construction.

use crate::types::mro::field_reads::MroFieldReads;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;

use crate::Db;
#[cfg(debug_assertions)]
use crate::types::DynamicType;
use crate::types::Type;
use crate::types::class_base::ClassBase;

use super::Mro;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::types) enum C3Work {
    OutputCapacity { entries: usize },
    RetainSequences { len: usize },
    CandidateAdvance,
    TailSequenceAdvance,
    TailEntryAdvance,
    IdentityComparison { todo_bytes: usize },
    OutputAppend { prefix_len: usize, capacity: usize },
    SelectedIdentity,
    RemovalSequenceAdvance,
    RemoveHead { todo_bytes: usize },
    BoxOutput { len: usize, capacity: usize },
    Publish,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(super) type C3SelectionCoordinates = Vec<(usize, usize)>;

/// A merge's positions move with its sequences, so equal payloads retain distinct origins.
pub(in crate::types) struct C3Occurrences {
    positions: Vec<(usize, usize)>,
    selected: Vec<(usize, usize)>,
}

impl C3Occurrences {
    fn new(sequence_count: usize) -> Self {
        Self {
            positions: (0..sequence_count).map(|index| (index, 0)).collect(),
            selected: Vec::new(),
        }
    }

    fn retain(&mut self, sequences: &[VecDeque<ClassBase<'_>>]) {
        let mut index = 0;
        self.positions.retain(|_| {
            let keep = !sequences[index].is_empty();
            index += 1;
            keep
        });
    }

    fn selected(&mut self, index: usize) {
        self.selected.push(self.positions[index]);
    }

    fn popped(&mut self, index: usize) {
        self.positions[index].1 += 1;
    }
}

pub(in crate::types) trait C3Effects<'db>: sealed::Sealed {
    type Error;

    async fn checkpoint(&self, work: C3Work) -> Result<(), Self::Error>;

    async fn mro_identity(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    #[inline]
    async fn start_occurrences(
        &self,
        _sequence_count: usize,
    ) -> Result<Option<C3Occurrences>, Self::Error> {
        Ok(None)
    }

    #[inline]
    async fn publish_occurrences(
        &self,
        _occurrences: Option<C3Occurrences>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Ordinary merging invokes the shared loop without constructing futures.
pub(in crate::types) trait SynchronousC3Effects: sealed::Sealed {
    type Error;

    fn checkpoint(&self, work: C3Work) -> Result<(), Self::Error>;

    fn mro_identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    #[inline]
    fn start_occurrences(
        &self,
        _sequence_count: usize,
    ) -> Result<Option<C3Occurrences>, Self::Error> {
        Ok(None)
    }

    #[inline]
    fn publish_occurrences(&self, _occurrences: Option<C3Occurrences>) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Input sequence ownership and its cancellation cleanup are admitted by the caller.
#[ty_mapping_probe_macros::dual_c3_merge]
#[inline]
pub(in crate::types) async fn c3_merge_with<'db, E: C3Effects<'db>>(
    fields: MroFieldReads<'db>,
    mut sequences: Vec<VecDeque<ClassBase<'db>>>,
    effects: &E,
) -> Result<Option<Mro<'db>>, E::Error> {
    // Most MROs aren't that long...
    effects
        .checkpoint(C3Work::OutputCapacity { entries: 8 })
        .await?;
    let mut occurrences = effects.start_occurrences(sequences.len()).await?;
    let mut mro = Vec::with_capacity(8);

    loop {
        effects
            .checkpoint(C3Work::RetainSequences {
                len: sequences.len(),
            })
            .await?;
        if let Some(occurrences) = &mut occurrences {
            occurrences.retain(&sequences);
        }
        sequences.retain(|sequence| !sequence.is_empty());

        if sequences.is_empty() {
            effects
                .checkpoint(C3Work::BoxOutput {
                    len: mro.len(),
                    capacity: mro.capacity(),
                })
                .await?;
            let mro = Mro::from(mro);
            effects.checkpoint(C3Work::Publish).await?;
            effects.publish_occurrences(occurrences).await?;
            return Ok(Some(mro));
        }

        // If the candidate exists "deeper down" in the inheritance hierarchy,
        // we should refrain from adding it to the MRO for now. Add the first candidate
        // for which this does not hold true. If this holds true for all candidates,
        // return `None`; it will be impossible to find a consistent MRO for the class
        // with the given bases.
        let mut candidates = sequences.iter();
        let (mro_entry, mro_entry_identity, selected_index) = 'candidate: loop {
            effects.checkpoint(C3Work::CandidateAdvance).await?;
            let Some(outer_sequence) = candidates.next() else {
                effects.checkpoint(C3Work::Publish).await?;
                effects.publish_occurrences(None).await?;
                return Ok(None);
            };
            let candidate = outer_sequence[0];
            let candidate_identity = effects.mro_identity(fields, candidate).await?;

            let mut tails = sequences.iter();
            loop {
                effects.checkpoint(C3Work::TailSequenceAdvance).await?;
                let Some(sequence) = tails.next() else {
                    break;
                };
                let mut tail = sequence.iter().skip(1);
                loop {
                    effects.checkpoint(C3Work::TailEntryAdvance).await?;
                    let Some(base) = tail.next() else {
                        break;
                    };
                    let base_identity = effects.mro_identity(fields, *base).await?;
                    effects
                        .checkpoint(C3Work::IdentityComparison {
                            todo_bytes: comparison_label_bytes(base_identity, candidate_identity),
                        })
                        .await?;
                    let not_head = base_identity != candidate_identity;
                    if !not_head {
                        continue 'candidate;
                    }
                }
            }

            break (
                candidate,
                candidate_identity,
                sequences.len() - candidates.len() - 1,
            );
        };

        effects
            .checkpoint(C3Work::OutputAppend {
                prefix_len: mro.len(),
                capacity: mro.capacity(),
            })
            .await?;
        if let Some(occurrences) = &mut occurrences {
            occurrences.selected(selected_index);
        }
        mro.push(mro_entry);

        // Make sure we don't try to add the candidate to the MRO twice:
        effects.checkpoint(C3Work::SelectedIdentity).await?;
        let sequence_count = sequences.len();
        let mut heads = sequences.iter_mut();
        loop {
            effects.checkpoint(C3Work::RemovalSequenceAdvance).await?;
            let Some(sequence) = heads.next() else {
                break;
            };
            let Some(base) = sequence.front().copied() else {
                continue;
            };
            let base_identity = effects.mro_identity(fields, base).await?;
            effects
                .checkpoint(C3Work::RemoveHead {
                    todo_bytes: comparison_label_bytes(base_identity, mro_entry_identity),
                })
                .await?;
            if base_identity == mro_entry_identity {
                sequence.pop_front();
                if let Some(occurrences) = &mut occurrences {
                    occurrences.popped(sequence_count - heads.len() - 1);
                }
            }
        }
    }
}

struct CapturingC3Effects<'a, E> {
    inner: &'a E,
    selected: RefCell<Vec<(usize, usize)>>,
}

impl<E> sealed::Sealed for CapturingC3Effects<'_, E> {}

impl<E: SynchronousC3Effects> SynchronousC3Effects for CapturingC3Effects<'_, E> {
    type Error = E::Error;

    fn checkpoint(&self, work: C3Work) -> Result<(), Self::Error> {
        self.inner.checkpoint(work)
    }

    fn mro_identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.inner.mro_identity(fields, base)
    }

    fn start_occurrences(
        &self,
        sequence_count: usize,
    ) -> Result<Option<C3Occurrences>, Self::Error> {
        Ok(Some(C3Occurrences::new(sequence_count)))
    }

    fn publish_occurrences(&self, occurrences: Option<C3Occurrences>) -> Result<(), Self::Error> {
        if let Some(occurrences) = occurrences {
            *self.selected.borrow_mut() = occurrences.selected;
        }
        Ok(())
    }
}

/// Records selections for a declaration-owned merge. The caller owns the recording allocations.
/// Failed merges return no coordinates; interrupted merges cannot publish a partial recording.
pub(super) fn capture_c3_sync<'db, E: SynchronousC3Effects>(
    db: &'db dyn Db,
    sequences: Vec<VecDeque<ClassBase<'db>>>,
    effects: &E,
) -> Result<(Option<Mro<'db>>, C3SelectionCoordinates), E::Error> {
    let effects = CapturingC3Effects {
        inner: effects,
        selected: RefCell::default(),
    };
    let result = c3_merge_sync(db, sequences, &effects)?;
    Ok((result, effects.selected.into_inner()))
}

// Debug TODO identities compare their labels by content; release TODO identities have no label.
#[cfg(debug_assertions)]
#[inline]
fn comparison_label_bytes(left: Type<'_>, right: Type<'_>) -> usize {
    if let (Type::Dynamic(DynamicType::Todo(left)), Type::Dynamic(DynamicType::Todo(right))) =
        (left, right)
        && left.0.len() == right.0.len()
    {
        left.0.len()
    } else {
        0
    }
}

#[cfg(not(debug_assertions))]
#[inline]
fn comparison_label_bytes(_left: Type<'_>, _right: Type<'_>) -> usize {
    0
}

pub(in crate::types) struct InlineC3Effects;

impl sealed::Sealed for InlineC3Effects {}

impl SynchronousC3Effects for InlineC3Effects {
    type Error = Infallible;

    #[inline]
    fn checkpoint(&self, _work: C3Work) -> Result<(), Infallible> {
        Ok(())
    }

    #[inline]
    fn mro_identity<'db>(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(fields.mro_identity(base))
    }
}
