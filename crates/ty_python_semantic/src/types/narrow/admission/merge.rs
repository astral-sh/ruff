//! Complete structural bounds for the ordinary ordered DNF merges.
//!
//! Measurement yields between bounded chunks. The admitted merge is atomic: its quote includes
//! candidates rejected by deduplication, all allocation requests, and eventual payload disposal.

use std::alloc::Layout;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};
use smallvec::SmallVec;

use super::{
    CombinedNarrowingConstraint, Conjunctions, NarrowingConstraint, NarrowingConstraintKind,
    NarrowingOperation, admit, checked,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Default)]
struct DisjunctMetadata {
    len: usize,
    capacity: usize,
    max_conjuncts: usize,
    max_inline_bytes: usize,
}

#[derive(Default)]
struct Metadata {
    intersections: DisjunctMetadata,
    replacements: DisjunctMetadata,
    combined: bool,
    cleanup: usize,
}

#[derive(Clone, Copy, Default)]
struct MergeQuote {
    work: usize,
    bytes: usize,
}

impl MergeQuote {
    fn work(&mut self, work: usize) -> RunResult<()> {
        self.work = checked(self.work.checked_add(work))?;
        Ok(())
    }

    fn combine(&mut self, other: Self, count: usize) -> RunResult<()> {
        self.work(checked(other.work.checked_mul(count))?)?;
        self.bytes = checked(
            other
                .bytes
                .checked_mul(count)
                .and_then(|bytes| self.bytes.checked_add(bytes)),
        )?;
        Ok(())
    }

    fn allocation<T>(&mut self, capacity: usize, requests: usize) -> RunResult<()> {
        let bytes = Layout::array::<T>(capacity)
            .map_err(|_| RunError::Contract("narrowing merge allocation overflow"))?
            .size();
        let bytes = checked(bytes.checked_mul(requests))?;
        self.bytes = checked(self.bytes.checked_add(bytes))?;
        // Each requested backing pays for initialization, relocation and eventual disposal.
        self.work(checked(bytes.checked_mul(3))?)
    }

    fn clone_conjuncts(&mut self, len: usize) -> RunResult<()> {
        if len > 2 {
            self.allocation::<NarrowingOperation<'_>>(
                checked(len.checked_next_power_of_two())?,
                1,
            )?;
        }
        Ok(())
    }

    fn pushed<T>(&mut self, len: usize, inline: usize) -> RunResult<()> {
        if len > inline {
            // SmallVec 1.15.2 rounds each push growth to a power of two. The sum of all
            // requested capacities is less than twice the final capacity, even when the
            // first allocation belongs to a clone that was quoted separately.
            self.allocation::<T>(checked(len.checked_next_power_of_two())?, 2)?;
        }
        self.work(checked(
            len.checked_mul(size_of::<T>())
                .and_then(|n| n.checked_mul(4)),
        )?)
    }

    fn extended<T>(&mut self, len: usize, capacity: usize, incoming: usize) -> RunResult<()> {
        let required = checked(len.checked_add(incoming))?;
        if required > capacity {
            // These ordinary extensions consume an exact-size SmallVec iterator. Its lower
            // size hint reserves the whole addition before any element is transferred.
            self.allocation::<T>(checked(required.checked_next_power_of_two())?, 1)?;
        }
        self.work(checked(
            incoming
                .checked_mul(size_of::<T>())
                .and_then(|n| n.checked_mul(4)),
        )?)
    }
}

async fn measure_disjuncts<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    disjuncts: &SmallVec<[Conjunctions<'db>; 1]>,
    cleanup: &mut usize,
) -> RunResult<DisjunctMetadata> {
    let mut metadata = endpoint
        .local_call(|| {
            admit(endpoint, size_of::<DisjunctMetadata>() * 2 + 32, 0)?;
            if disjuncts.spilled() {
                *cleanup = checked(
                    disjuncts
                        .capacity()
                        .checked_mul(size_of::<Conjunctions<'db>>())
                        .and_then(|bytes| cleanup.checked_add(bytes)),
                )?;
            }
            Ok(DisjunctMetadata {
                len: disjuncts.len(),
                capacity: disjuncts.capacity(),
                ..DisjunctMetadata::default()
            })
        })
        .await;
    for conjunction in disjuncts {
        endpoint
            .local_call(|| {
                admit(endpoint, 32, 0)?;
                let conjuncts = &conjunction.conjuncts;
                metadata.max_conjuncts = metadata.max_conjuncts.max(conjuncts.len());
                let backing = if conjuncts.spilled() {
                    conjuncts.capacity()
                } else {
                    0
                };
                *cleanup = checked(
                    conjuncts
                        .len()
                        .checked_add(backing)
                        .and_then(|len| len.checked_mul(size_of::<NarrowingOperation<'db>>()))
                        .and_then(|bytes| bytes.checked_add(size_of::<Conjunctions<'db>>()))
                        .and_then(|bytes| cleanup.checked_add(bytes)),
                )?;
                Ok(())
            })
            .await;
        let mut operations = conjunction.conjuncts.iter();
        if operations.len() == 0 {
            endpoint.checkpoint()?.await?;
        }
        while operations.len() != 0 {
            let chunk = operations.len().min(64);
            endpoint
                .local_call(|| {
                    admit(endpoint, chunk * 4, 0)?;
                    for operation in operations.by_ref().take(chunk) {
                        metadata.max_inline_bytes = metadata
                            .max_inline_bytes
                            .max(operation.ty().inline_payload_bytes());
                    }
                    Ok(())
                })
                .await;
            endpoint.checkpoint()?.await?;
        }
    }
    Ok(metadata)
}

async fn measure<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    constraint: &NarrowingConstraint<'db>,
) -> RunResult<Metadata> {
    let mut metadata = endpoint
        .local_call(|| {
            admit(endpoint, size_of::<Metadata>() * 2 + 16, 0)?;
            Ok(Metadata {
                cleanup: size_of::<NarrowingConstraint<'db>>() * 2,
                ..Metadata::default()
            })
        })
        .await;
    match &constraint.0 {
        NarrowingConstraintKind::Empty => {}
        NarrowingConstraintKind::Intersection(operation)
        | NarrowingConstraintKind::Replacement(operation) => {
            endpoint
                .local_call(|| {
                    admit(endpoint, 16, 0)?;
                    let disjunct = DisjunctMetadata {
                        len: 1,
                        capacity: 1,
                        max_conjuncts: 1,
                        max_inline_bytes: operation.ty().inline_payload_bytes(),
                    };
                    if matches!(&constraint.0, NarrowingConstraintKind::Intersection(_)) {
                        metadata.intersections = disjunct;
                    } else {
                        metadata.replacements = disjunct;
                    }
                    Ok(())
                })
                .await;
        }
        NarrowingConstraintKind::Combined(combined) => {
            endpoint
                .local_call(|| {
                    admit(endpoint, 16, 0)?;
                    metadata.combined = true;
                    metadata.cleanup = checked(
                        metadata
                            .cleanup
                            .checked_add(size_of::<CombinedNarrowingConstraint<'db>>() * 2),
                    )?;
                    Ok(())
                })
                .await;
            metadata.intersections = measure_disjuncts(
                endpoint,
                &combined.intersection_disjuncts,
                &mut metadata.cleanup,
            )
            .await?;
            metadata.replacements = measure_disjuncts(
                endpoint,
                &combined.replacement_disjuncts,
                &mut metadata.cleanup,
            )
            .await?;
        }
    }
    Ok(metadata)
}

fn product_quote(
    left: DisjunctMetadata,
    right: DisjunctMetadata,
) -> RunResult<(MergeQuote, usize)> {
    let count = checked(left.len.checked_mul(right.len))?;
    if count == 0 {
        return Ok((MergeQuote::default(), 0));
    }
    let len = checked(left.max_conjuncts.checked_add(right.max_conjuncts))?;
    let comparison = checked(
        left.max_inline_bytes
            .max(right.max_inline_bytes)
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<NarrowingOperation<'_>>() * 2 + 4)),
    )?;
    let mut candidate = MergeQuote {
        work: size_of::<Conjunctions<'_>>() * 6 + 32,
        bytes: 0,
    };
    candidate.clone_conjuncts(left.max_conjuncts)?;
    candidate.clone_conjuncts(right.max_conjuncts)?;
    candidate.pushed::<NarrowingOperation<'_>>(len, 2)?;
    // Every candidate clones both operands before scanning for Never. If neither scan wins,
    // each right operation searches the growing left vector, including rejected duplicates.
    let comparisons = checked(
        right
            .max_conjuncts
            .checked_mul(len)
            .and_then(|count| count.checked_add(len)),
    )?;
    candidate.work(checked(comparisons.checked_mul(comparison))?)?;
    let mut quote = MergeQuote::default();
    quote.combine(candidate, count)?;
    // A phase's nth candidate can compare with at most n previous results. Comparing ordered
    // conjunctions can visit every operation and its inline Type payload on both sides.
    let prior = count - 1;
    let pairs = if count.is_multiple_of(2) {
        checked((count / 2).checked_mul(prior))?
    } else {
        checked(count.checked_mul(prior / 2))?
    };
    let equality = checked(
        len.checked_mul(comparison)
            .and_then(|work| work.checked_add(4)),
    )?;
    quote.work(checked(pairs.checked_mul(equality))?)?;
    quote.pushed::<Conjunctions<'_>>(count, 1)?;
    Ok((quote, count))
}

fn and_quote(left: &Metadata, right: &Metadata) -> RunResult<MergeQuote> {
    let mut quote = MergeQuote {
        work: size_of::<NarrowingConstraint<'_>>() * 4 + 32,
        bytes: 0,
    };
    quote.work(right.cleanup)?;
    if right.intersections.len == 0 {
        return Ok(quote);
    }
    let (intersections, _) = product_quote(left.intersections, right.intersections)?;
    let (replacements, replacement_count) = product_quote(left.replacements, right.intersections)?;
    quote.combine(intersections, 1)?;
    quote.combine(replacements, 1)?;
    quote.extended::<Conjunctions<'_>>(
        right.replacements.len,
        right.replacements.capacity.max(1),
        replacement_count,
    )?;
    // Compression can avoid this allocation; the general result may retain both DNF vectors.
    quote.allocation::<CombinedNarrowingConstraint<'_>>(1, 1)?;
    Ok(quote)
}

fn or_quote(left: &Metadata, right: &Metadata) -> RunResult<MergeQuote> {
    let mut quote = MergeQuote {
        work: size_of::<NarrowingConstraint<'_>>() * 4 + 32,
        bytes: 0,
    };
    quote.work(checked(left.cleanup.checked_add(right.cleanup))?)?;
    quote.extended::<Conjunctions<'_>>(
        left.intersections.len,
        left.intersections.capacity.max(1),
        right.intersections.len,
    )?;
    quote.extended::<Conjunctions<'_>>(
        left.replacements.len,
        left.replacements.capacity.max(1),
        right.replacements.len,
    )?;
    if !left.combined {
        quote.allocation::<CombinedNarrowingConstraint<'_>>(1, 1)?;
    }
    Ok(quote)
}

pub(in crate::types) async fn merge_and<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    left: &NarrowingConstraint<'db>,
    mut right: NarrowingConstraint<'db>,
) -> RunResult<NarrowingConstraint<'db>> {
    let right_metadata = measure(endpoint, &right).await?;
    // The ordinary early return does not inspect the left payload.
    let left_metadata = if right_metadata.intersections.len == 0 {
        Metadata::default()
    } else {
        measure(endpoint, left).await?
    };
    Ok(endpoint
        .local_call(|| {
            admit(endpoint, 256, 0)?;
            let quote = and_quote(&left_metadata, &right_metadata)?;
            admit(endpoint, quote.work, quote.bytes)?;
            Ok(left.merge_constraint_and(std::mem::take(&mut right)))
        })
        .await)
}

pub(in crate::types) async fn merge_or<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    left: &mut NarrowingConstraint<'db>,
    mut right: NarrowingConstraint<'db>,
) -> RunResult<()> {
    let left_metadata = measure(endpoint, left).await?;
    let right_metadata = measure(endpoint, &right).await?;
    endpoint
        .local_call(|| {
            admit(endpoint, 256, 0)?;
            let quote = or_quote(&left_metadata, &right_metadata)?;
            admit(endpoint, quote.work, quote.bytes)?;
            left.merge_constraint_or(std::mem::take(&mut right));
            Ok(())
        })
        .await;
    Ok(())
}
