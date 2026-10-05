//! Controlled occurrence collection reuses stored type-walk children and canonical semantic leaves.

use std::alloc::Layout;
use std::future::Future;
use std::pin::Pin;
use std::slice;

use salsa::execution_probe::{RunError, RunResult};

use super::class_selection::FixedFieldCopy;
use super::storage::{StorageQuote, ordered_merge, sequence_merge, slots, table_merge};
use super::{SourceAccess, SourceEffects};
use crate::FxOrderSet;
use crate::types::cyclic::TypeIdentity;
use crate::types::cyclic::entry::recursive_identity_with;
use crate::types::generics::return_locations::{
    LocationBoundary, LocationEffects, LocationFacts, LocationState, OccurrenceLocation,
    TypeVarLocations, collect_locations_with, walk_locations_with,
};
use crate::types::signatures::Parameters;
use crate::types::visitor::runtime::RuntimeTypeWalk;
use crate::types::visitor::{
    NonAtomicType, TypeWalkEffects, TypeWalkEvent, TypeWalkPolicy, WalkAction,
};
use crate::types::{
    BoundTypeVarInstance, CallableType, Parameter, RecursiveType, Type, TypeAliasType,
};

/// Quotes an insert-only hash table with finite handle keys, including replacement retirement.
fn hash_insert_quote<T>(len: usize, capacity: usize) -> Option<StorageQuote> {
    let (mut quote, replacement_slots) = table_merge::<T>(len, capacity, 1, 0)?;
    // A key is a small tuple of enum tags and interned IDs. Account for each probe separately
    // from its representation, and prepay the newly retained entry's eventual destruction.
    quote.work = quote.work.checked_mul(32)?.checked_add(8)?;
    quote.bytes = quote.bytes.checked_add(size_of::<T>().checked_mul(2)?)?;
    if len.checked_add(1)? > capacity {
        quote.work = quote
            .work
            .checked_add(slots(capacity)?)?
            .checked_add(replacement_slots)?;
        quote.bytes = quote.bytes.checked_add(len.checked_mul(size_of::<T>())?)?;
    }
    Layout::from_size_align(quote.bytes, align_of::<T>()).ok()?;
    Some(quote)
}

/// Quotes ordered occurrence insertion, including the index table and cached-hash entries.
pub(super) fn ordered_insert_quote(len: usize, capacity: usize) -> Option<StorageQuote> {
    type Entry<'db> = (usize, BoundTypeVarInstance<'db>);
    let mut quote = ordered_merge::<BoundTypeVarInstance<'_>>(len, capacity, 1)?;
    quote.work = quote.work.checked_mul(16)?.checked_add(8)?;
    quote.bytes = quote
        .bytes
        .checked_add(size_of::<Entry<'_>>().checked_mul(2)?)?;
    if len.checked_add(1)? > capacity {
        let (_, replacement_slots) = table_merge::<usize>(len, capacity, 1, 0)?;
        quote.work = quote
            .work
            .checked_add(slots(capacity)?)?
            .checked_add(replacement_slots)?;
        quote.bytes = quote
            .bytes
            .checked_add(len.checked_mul(size_of::<Entry<'_>>())?)?;
    }
    Layout::from_size_align(quote.bytes, align_of::<Entry<'_>>()).ok()?;
    Some(quote)
}

/// Quotes a flat boundary push and prepays its pop or abandoned-stack retirement.
fn boundary_quote(len: usize, capacity: usize) -> Option<StorageQuote> {
    let mut quote = sequence_merge::<LocationBoundary<'_>>(len, capacity, 1)?;
    quote.work = quote.work.checked_add(8)?;
    quote.bytes = quote
        .bytes
        .checked_add(size_of::<LocationBoundary<'_>>().checked_mul(2)?)?;
    if len.checked_add(1)? > capacity {
        let replacement = capacity.checked_mul(2)?.max(len.checked_add(1)?).max(4);
        quote.work = quote.work.checked_add(capacity)?.checked_add(replacement)?;
        quote.bytes = quote
            .bytes
            .checked_add(len.checked_mul(size_of::<LocationBoundary<'_>>())?)?;
    }
    Layout::from_size_align(quote.bytes, align_of::<LocationBoundary<'_>>()).ok()?;
    Some(quote)
}

/// Supplements the existing pending-frame reservation for the collector's handle-only frames.
/// The walker pays allocation; this pays transfers, abandonment and backing retirement.
fn pending_push_quote(state: &LocationState<'_>) -> Option<StorageQuote> {
    let pending = &state.cursor.pending;
    let mut quote = StorageQuote {
        work: 8,
        bytes: size_of::<WalkAction<'_>>().checked_mul(2)?,
    };
    let required = pending.len().checked_add(1)?;
    if required > pending.capacity() {
        let replacement = pending.capacity().checked_mul(2)?.max(required).max(4);
        quote.work = quote
            .work
            .checked_add(pending.capacity())?
            .checked_add(replacement)?;
        quote.bytes = quote
            .bytes
            .checked_add(pending.len().checked_mul(size_of::<WalkAction<'_>>())?)?;
    }
    Some(quote)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Admits the continuation and its result carriers before constructing a boxed child future.
    async fn return_location_future<F: Future, M: FnOnce() -> F>(
        &self,
        make: M,
    ) -> RunResult<Pin<Box<F>>> {
        let quote = size_of::<F::Output>()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(size_of::<F>().checked_mul(2)?))
            .map(|bytes| (1, bytes))
            .ok_or(RunError::Contract(
                "return-location future quotation overflow",
            ));
        self.local_quoted_with_fixed_transfers(quote, || Box::pin(make()))
            .await
    }

    /// Supplies the existing type-walk field and lazy-child providers on the caller's endpoint.
    fn return_location_fields(&self) -> RuntimeTypeWalk<'_, 'run, 'db, (), &Self> {
        RuntimeTypeWalk {
            db: self.db(),
            endpoint: self.access.endpoint(),
            query: (),
            unavailable: self,
        }
    }

    /// Collects complete occurrence sets before the caller filters or renames any variables.
    pub(super) async fn return_typevar_locations(
        &self,
        parameters: &Parameters<'db>,
        return_type: Type<'db>,
    ) -> RunResult<TypeVarLocations<'db>> {
        self.return_location_future(|| collect_locations_with(parameters, return_type, self))
            .await?
            .await
    }

    /// Admits a collection quotation while its state remains borrowed by the caller.
    async fn location_storage<T>(
        &self,
        quote: Option<StorageQuote>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        self.local_quoted_with_fixed_transfers(
            quote
                .map(|quote| (quote.work, quote.bytes))
                .ok_or(RunError::Contract(
                    "return-location storage quotation overflow",
                )),
            action,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LocationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn new_state(&self) -> RunResult<LocationState<'db>> {
        self.local_with_fixed_transfers(8, 0, LocationState::default)
            .await
    }

    async fn parameters<'a>(
        &self,
        parameters: &'a Parameters<'db>,
    ) -> RunResult<slice::Iter<'a, Parameter<'db>>> {
        self.local_with_fixed_transfers(2, 0, || parameters.iter())
            .await
    }

    async fn next_parameter(
        &self,
        parameters: &mut slice::Iter<'_, Parameter<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local_with_fixed_transfers(3, 0, || parameters.next().map(Parameter::annotated_type))
            .await
    }

    async fn walk(
        &self,
        state: &mut LocationState<'db>,
        ty: Type<'db>,
        location: OccurrenceLocation<'db>,
    ) -> RunResult<()> {
        self.return_location_future(|| {
            walk_locations_with(state, ty, location, LocationFacts, self)
        })
        .await?
        .await
    }

    async fn start(
        &self,
        state: &mut LocationState<'db>,
        location: OccurrenceLocation<'db>,
    ) -> RunResult<()> {
        self.local_with_fixed_transfers(1, 0, || state.location = location)
            .await
    }

    async fn push(&self, state: &mut LocationState<'db>, action: WalkAction<'db>) -> RunResult<()> {
        let quote = self
            .local_with_fixed_transfers(32, 0, || pending_push_quote(state))
            .await?;
        self.location_storage(quote, || ()).await?;
        self.return_location_future(|| async {
            TypeWalkEffects::push_action(
                &mut self.return_location_fields(),
                &mut state.cursor,
                action,
            )
            .await
        })
        .await?
        .await
    }

    async fn next(&self, state: &mut LocationState<'db>) -> RunResult<Option<TypeWalkEvent<'db>>> {
        self.return_location_future(|| async {
            TypeWalkEffects::next_event(
                &mut self.return_location_fields(),
                &mut state.cursor,
                TypeWalkPolicy::locations(),
            )
            .await
        })
        .await?
        .await
    }

    async fn remember(&self, state: &mut LocationState<'db>, ty: Type<'db>) -> RunResult<bool> {
        let quote = self
            .local_with_fixed_transfers(64, 0, || {
                hash_insert_quote::<(Type<'db>, OccurrenceLocation<'db>)>(
                    state.seen.len(),
                    state.seen.capacity(),
                )
            })
            .await?;
        self.location_storage(quote, || state.seen.insert((ty, state.location)))
            .await
    }

    async fn normalize(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarInstance<'db>> {
        let request = self.local_with_fixed_transfers(4, 0, || {
            variable.identity_request(self.access.endpoint().field_request_context())
        }).await?;
        let mut identity = self.return_location_future(|| self.field_with_profile(request, &FixedFieldCopy)).await?.await?;
        let request = self.local_with_fixed_transfers(4, 0, || {
            identity.identity.field_requests(self.access.endpoint().field_request_context()).kind()
        }).await?;
        let kind = self.return_location_future(|| self.field_with_profile(request, &FixedFieldCopy)).await?.await?;
        if !kind.is_paramspec() {
            return Ok(variable);
        }
        let request = self.local_with_fixed_transfers(4, 0, || {
            variable.field_requests(self.access.endpoint().field_request_context()).typevar()
        }).await?;
        let typevar = self.return_location_future(|| self.field_with_profile(request, &FixedFieldCopy)).await?.await?;
        let request = self.local_with_fixed_transfers(4, 0, || {
            typevar.field_requests(self.access.endpoint().field_request_context()).explicit_variance()
        }).await?;
        let variance = self.return_location_future(|| self.field_with_profile(request, &FixedFieldCopy)).await?.await?;
        self.local_with_fixed_transfers(1, 0, || identity.paramspec_attr = None)
            .await?;
        let typevar = self
            .return_location_future(|| {
                self.access
                    .intern_typevar_instance(identity.identity, None, variance, None)
            })
            .await?
            .await?;
        self.return_location_future(|| self.access.intern_bound_typevar(typevar, identity))
            .await?
            .await
    }

    async fn record(
        &self,
        state: &mut LocationState<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        match state.location {
            OccurrenceLocation::Parameter | OccurrenceLocation::Return => {
                let quote = self
                    .local_with_fixed_transfers(64, 0, || {
                        let outside = &state.locations.found_outside_callable_return;
                        hash_insert_quote::<BoundTypeVarInstance<'db>>(
                            outside.len(),
                            outside.capacity(),
                        )
                    })
                    .await?;
                self.location_storage(quote, || {
                    state
                        .locations
                        .found_outside_callable_return
                        .insert(variable);
                })
                .await
            }
            OccurrenceLocation::Callable(callable) => {
                let quote = self
                    .local_with_fixed_transfers(64, 0, || {
                        let inside = &state.locations.found_inside_callable_return;
                        hash_insert_quote::<(
                            CallableType<'db>,
                            FxOrderSet<BoundTypeVarInstance<'db>>,
                        )>(inside.len(), inside.capacity())
                    })
                    .await?;
                let variables = self
                    .location_storage(quote, move || {
                        state
                            .locations
                            .found_inside_callable_return
                            .entry(callable)
                            .or_default()
                    })
                    .await?;
                let quote = self
                    .local_with_fixed_transfers(64, 0, || {
                        ordered_insert_quote(variables.len(), variables.capacity())
                    })
                    .await?;
                self.location_storage(quote, || {
                    variables.insert(variable);
                })
                .await
            }
        }
    }

    async fn enter_callable(
        &self,
        state: &mut LocationState<'db>,
        callable: CallableType<'db>,
    ) -> RunResult<bool> {
        let enters = self
            .local_with_fixed_transfers(2, 0, || state.location == OccurrenceLocation::Return)
            .await?;
        if !enters {
            return Ok(false);
        }
        let quote = self
            .local_with_fixed_transfers(32, 0, || {
                boundary_quote(state.boundaries.len(), state.boundaries.capacity())
            })
            .await?;
        self.location_storage(quote, || {
            state.boundaries.push(LocationBoundary {
                previous: state.location,
                active_alias: None,
            });
            state.location = OccurrenceLocation::Callable(callable);
            true
        })
        .await
    }

    async fn identity(&self, ty: Type<'db>) -> RunResult<TypeIdentity<'db>> {
        let identity = self
            .return_location_future(|| recursive_identity_with(ty, self))
            .await?
            .await?;
        self.local_with_fixed_transfers(1, 0, || identity.unwrap_or(TypeIdentity::Other(ty)))
            .await
    }

    async fn enter_alias(
        &self,
        state: &mut LocationState<'db>,
        identity: TypeIdentity<'db>,
    ) -> RunResult<bool> {
        let quote = self
            .local_with_fixed_transfers(8, 0, || {
                state
                    .boundaries
                    .len()
                    .checked_mul(8)
                    .and_then(|work| work.checked_add(2))
            })
            .await?;
        let active = self
            .local_quoted_with_fixed_transfers(
                quote.map(|work| (work, 0)).ok_or(RunError::Contract(
                    "return-location active scan quotation overflow",
                )),
                || {
                    state
                        .boundaries
                        .iter()
                        .any(|boundary| boundary.active_alias == Some(identity))
                },
            )
            .await?;
        if active {
            return Ok(false);
        }
        let quote = self
            .local_with_fixed_transfers(32, 0, || {
                boundary_quote(state.boundaries.len(), state.boundaries.capacity())
            })
            .await?;
        self.location_storage(quote, || {
            state.boundaries.push(LocationBoundary {
                previous: state.location,
                active_alias: Some(identity),
            });
            true
        })
        .await
    }

    async fn leave(&self, state: &mut LocationState<'db>) -> RunResult<()> {
        self.local_with_fixed_transfers(3, 0, || {
            let boundary = state
                .boundaries
                .pop()
                .ok_or(RunError::Contract("unbalanced return-location boundary"))?;
            state.location = boundary.previous;
            Ok(())
        })
        .await?
    }

    async fn alias_value(&self, alias: TypeAliasType<'db>) -> RunResult<Type<'db>> {
        self.return_location_future(|| async {
            TypeWalkEffects::alias_value(&mut self.return_location_fields(), alias).await
        })
        .await?
        .await
    }

    async fn recursive_unfold(&self, recursive: RecursiveType<'db>) -> RunResult<Type<'db>> {
        self.return_location_future(|| async {
            TypeWalkEffects::recursive_unfold(&mut self.return_location_fields(), recursive).await
        })
        .await?
        .await
    }

    async fn expand(
        &self,
        state: &mut LocationState<'db>,
        kind: NonAtomicType<'db>,
    ) -> RunResult<()> {
        self.return_location_future(|| async {
            TypeWalkEffects::expand_children(
                &mut self.return_location_fields(),
                &mut state.cursor,
                kind,
                TypeWalkPolicy::locations(),
            )
            .await
        })
        .await?
        .await
    }

    async fn finish(&self, state: &mut LocationState<'db>) -> RunResult<TypeVarLocations<'db>> {
        self.local_with_fixed_transfers(4, 0, || std::mem::take(&mut state.locations))
            .await
    }
}
