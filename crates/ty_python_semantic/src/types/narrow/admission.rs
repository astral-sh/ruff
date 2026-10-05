//! Native storage and comparison admission for complete narrowing-constraint payloads.

use std::alloc::Layout;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use ty_python_core::place::ScopedPlaceId;

use super::{
    CombinedNarrowingConstraint, Conjunctions, ExpressionNarrowingConstraints,
    FrozenNarrowingConstraints, NarrowingConstraint, NarrowingConstraintKind, NarrowingConstraints,
    NarrowingOperation,
};

pub(in crate::types) mod merge;

#[cfg(test)]
mod tests;

fn checked<T>(value: Option<T>) -> RunResult<T> {
    value.ok_or(RunError::Contract("narrowing payload quotation overflow"))
}

fn admit(endpoint: &TaskEndpoint<'_, '_>, work: usize, bytes: usize) -> RunResult<()> {
    endpoint.admit_work(work)?;
    if bytes != 0 {
        endpoint.admit(ExecutionWork::Resource {
            requested_bytes: bytes,
        })?;
    }
    endpoint.check_completion()
}

#[derive(Default)]
struct ConstraintQuote {
    comparison_work: usize,
    clone_work: usize,
    clone_bytes: usize,
    retained_cleanup: usize,
}

impl ConstraintQuote {
    fn structure<T>(&mut self, count: usize) -> RunResult<()> {
        let bytes = checked(count.checked_mul(size_of::<T>()))?;
        self.comparison_work = checked(self.comparison_work.checked_add(bytes))?;
        self.clone_work = checked(
            bytes
                .checked_mul(3)
                .and_then(|work| self.clone_work.checked_add(work)),
        )?;
        self.retained_cleanup = checked(self.retained_cleanup.checked_add(bytes))?;
        Ok(())
    }

    fn allocation<T>(&mut self, cloned: usize, retained: usize) -> RunResult<()> {
        let bytes = Layout::array::<T>(cloned)
            .map_err(|_| RunError::Contract("narrowing payload allocation overflow"))?
            .size();
        self.clone_bytes = checked(self.clone_bytes.checked_add(bytes))?;
        // Allocation ownership includes disposal after refusal or cancellation of a parent.
        self.clone_work = checked(
            bytes
                .checked_mul(2)
                .and_then(|work| self.clone_work.checked_add(work)),
        )?;
        self.retained_cleanup = checked(
            retained
                .checked_mul(size_of::<T>())
                .and_then(|work| self.retained_cleanup.checked_add(work)),
        )?;
        Ok(())
    }

    fn vector<T>(&mut self, len: usize, inline: usize, retained_capacity: usize) -> RunResult<()> {
        // SmallVec's slice clone reserves its exact-size iterator's lower bound, rounding a
        // spill up to a power of two. It does not preserve the source's spare capacity.
        let cloned = if len > inline {
            checked(len.checked_next_power_of_two())?
        } else {
            0
        };
        self.allocation::<T>(cloned, retained_capacity)
    }

    fn operation(&mut self, operation: NarrowingOperation<'_>) -> RunResult<()> {
        self.structure::<NarrowingOperation<'_>>(1)?;
        self.comparison_work = checked(
            self.comparison_work
                .checked_add(operation.ty().inline_payload_bytes()),
        )?;
        Ok(())
    }
}

async fn constraint_quote<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    constraint: &NarrowingConstraint<'db>,
) -> RunResult<ConstraintQuote> {
    let mut quote = endpoint
        .local_call(|| {
            admit(endpoint, 4, 0)?;
            let mut quote = ConstraintQuote::default();
            quote.structure::<NarrowingConstraint<'db>>(1)?;
            Ok(quote)
        })
        .await;
    match &constraint.0 {
        NarrowingConstraintKind::Empty => {}
        NarrowingConstraintKind::Intersection(operation)
        | NarrowingConstraintKind::Replacement(operation) => {
            endpoint
                .local_call(|| {
                    admit(endpoint, 2, 0)?;
                    quote.operation(*operation)
                })
                .await;
        }
        NarrowingConstraintKind::Combined(combined) => {
            endpoint
                .local_call(|| {
                    admit(endpoint, 4, 0)?;
                    quote.structure::<CombinedNarrowingConstraint<'db>>(1)?;
                    quote.allocation::<CombinedNarrowingConstraint<'db>>(1, 1)
                })
                .await;
            for disjuncts in [
                &combined.intersection_disjuncts,
                &combined.replacement_disjuncts,
            ] {
                endpoint
                    .local_call(|| {
                        admit(endpoint, 4, 0)?;
                        quote.vector::<Conjunctions<'db>>(
                            disjuncts.len(),
                            1,
                            if disjuncts.spilled() {
                                disjuncts.capacity()
                            } else {
                                0
                            },
                        )
                    })
                    .await;
                for conjunction in disjuncts {
                    endpoint
                        .local_call(|| {
                            admit(endpoint, 4, 0)?;
                            quote.structure::<Conjunctions<'db>>(1)?;
                            quote.vector::<NarrowingOperation<'db>>(
                                conjunction.conjuncts.len(),
                                2,
                                if conjunction.conjuncts.spilled() {
                                    conjunction.conjuncts.capacity()
                                } else {
                                    0
                                },
                            )
                        })
                        .await;
                    if conjunction.conjuncts.is_empty() {
                        endpoint.checkpoint()?.await?;
                    }
                    let mut operations = conjunction.conjuncts.iter();
                    while operations.len() != 0 {
                        let chunk = operations.len().min(64);
                        endpoint
                            .local_call(|| {
                                admit(endpoint, chunk * 2, 0)?;
                                for operation in operations.by_ref().take(chunk) {
                                    quote.operation(*operation)?;
                                }
                                Ok(())
                            })
                            .await;
                        endpoint.checkpoint()?.await?;
                    }
                }
            }
        }
    }
    Ok(quote)
}

pub(in crate::types) async fn clone_constraint<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    constraint: &NarrowingConstraint<'db>,
) -> RunResult<NarrowingConstraint<'db>> {
    let quote = constraint_quote(endpoint, constraint).await?;
    Ok(endpoint
        .local_call(|| {
            admit(endpoint, quote.clone_work, quote.clone_bytes)?;
            Ok(constraint.clone())
        })
        .await)
}

pub(in crate::types) async fn selected_constraint<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    constraints: &ExpressionNarrowingConstraints<'db>,
    place: ScopedPlaceId,
    is_positive: bool,
) -> RunResult<Option<NarrowingConstraint<'db>>> {
    let selected = endpoint
        .local_call(|| {
            admit(endpoint, 4, 0)?;
            let map = if is_positive {
                &constraints.positive
            } else {
                &constraints.negative
            };
            let Some(map) = map else {
                return Ok(None);
            };
            let work = map
                .iter()
                .len()
                .checked_ilog2()
                .map_or(1, |logarithm| logarithm as usize + 2);
            admit(endpoint, work, 0)?;
            Ok(map.get(&place))
        })
        .await;
    match selected {
        Some(constraint) => Ok(Some(clone_constraint(endpoint, constraint).await?)),
        None => Ok(None),
    }
}

pub(in crate::types) async fn comparison_work<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    left: &ExpressionNarrowingConstraints<'db>,
    right: &ExpressionNarrowingConstraints<'db>,
) -> RunResult<usize> {
    let mut work = 4usize;
    for value in [left, right] {
        for map in [&value.positive, &value.negative] {
            endpoint.local_call(|| admit(endpoint, 1, 0)).await;
            if let Some(map) = map {
                for (_, constraint) in map {
                    let quote = constraint_quote(endpoint, constraint).await?;
                    work = endpoint
                        .local_call(|| {
                            admit(endpoint, 2, 0)?;
                            checked(
                                work.checked_add(size_of::<ScopedPlaceId>())
                                    .and_then(|work| work.checked_add(quote.comparison_work)),
                            )
                        })
                        .await;
                    endpoint.checkpoint()?.await?;
                }
            }
        }
    }
    Ok(work)
}

fn table_slots(capacity: usize) -> Option<usize> {
    if capacity == 0 {
        Some(0)
    } else {
        capacity.checked_add(1)?.checked_mul(4)?.checked_add(32)
    }
}

pub(in crate::types) async fn singleton_constraint<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    place: ScopedPlaceId,
    mut constraint: NarrowingConstraint<'db>,
) -> RunResult<NarrowingConstraints<'db>> {
    let payload = constraint_quote(endpoint, &constraint).await?;
    Ok(endpoint
        .local_call(|| {
            admit(endpoint, 4, 0)?;
            let slots = checked(table_slots(1))?;
            let bytes = checked(
                slots.checked_mul(size_of::<(ScopedPlaceId, NarrowingConstraint<'db>)>() + 1),
            )?;
            let work = checked(
                bytes
                    .checked_mul(3)
                    .and_then(|work| work.checked_add(slots))
                    .and_then(|work| work.checked_add(payload.retained_cleanup)),
            )?;
            admit(endpoint, work, bytes)?;
            Ok(NarrowingConstraints::from_iter([(
                place,
                std::mem::take(&mut constraint),
            )]))
        })
        .await)
}

pub(in crate::types) async fn freeze_constraints<'run, 'db: 'run>(
    endpoint: &TaskEndpoint<'run, 'db>,
    mut constraints: Option<NarrowingConstraints<'db>>,
) -> RunResult<Option<FrozenNarrowingConstraints<'db>>> {
    // Controlled mutable maps are insert-only: merges and rebinding invalidation still refuse.
    // Enabling removal requires retaining the table's high-water backing before using this
    // quotation, since HashMap::capacity can then understate the backing visited by iteration.
    let (len, backing) = endpoint
        .local_call(|| {
            admit(endpoint, 4, 0)?;
            let (len, backing) = match &constraints {
                Some(map) => (map.len(), checked(table_slots(map.capacity()))?),
                None => (0, 0),
            };
            admit(endpoint, backing, 0)?;
            Ok((len, backing))
        })
        .await;
    let mut payload_cleanup = 0usize;
    if let Some(map) = &constraints {
        for constraint in map.values() {
            let quote = constraint_quote(endpoint, constraint).await?;
            payload_cleanup = endpoint
                .local_call(|| {
                    admit(endpoint, 1, 0)?;
                    checked(payload_cleanup.checked_add(quote.retained_cleanup))
                })
                .await;
            endpoint.checkpoint()?.await?;
        }
    }
    Ok(endpoint
        .local_call(|| {
            admit(endpoint, 8, 0)?;
            let entry_bytes = size_of::<(ScopedPlaceId, NarrowingConstraint<'db>)>();
            let logarithm = len.checked_ilog2().map_or(0, |value| value as usize);
            // FrozenMap consumes the hash table, collects into a Vec, sorts its fixed-size
            // keys, checks uniqueness in debug builds, and converts the entries to a Box.
            let sorting = checked(
                logarithm
                    .checked_mul(23)
                    .and_then(|work| work.checked_add(43))
                    .and_then(|work| work.checked_mul(len))
                    .and_then(|work| work.checked_mul(entry_bytes)),
            )?;
            let allocation = if len == 0 {
                0
            } else {
                checked(len.checked_mul(5).and_then(|len| len.checked_add(16)))?
            };
            let bytes = Layout::array::<(ScopedPlaceId, NarrowingConstraint<'db>)>(allocation)
                .map_err(|_| RunError::Contract("narrowing frozen allocation overflow"))?
                .size();
            let work = checked(
                bytes
                    .checked_mul(3)
                    .and_then(|work| work.checked_add(sorting))
                    .and_then(|work| work.checked_add(backing.checked_mul(entry_bytes + 1)?))
                    .and_then(|work| work.checked_add(payload_cleanup))
                    .and_then(|work| work.checked_add(4)),
            )?;
            admit(endpoint, work, bytes)?;
            Ok(constraints.take().map(FrozenNarrowingConstraints::from))
        })
        .await)
}
