use super::callback::{self, CallbackOwner};
use super::frame_free::EntryScope;
use super::passive_memos::{
    PassiveMemoInspection, PassiveMemoSchema, PassiveRetirement, SchemaOps,
};
use super::read_run;
use super::registration::{InternedValues, QueryKeys, TaskEndpoint};
use super::{ExecutionWork, RunError};
use crate::database::AsDynDatabase;
use crate::function::memo::KeyRetirementError;
use crate::function::{Configuration, InternedQueryConfiguration, QueryKeyProfile};
use crate::id::FromId;
use crate::interned::{FiniteInternedConfiguration, InternCommit, PreparedIntern, ReuseInspection};
use crate::quote::{QuoteError, QuoteFuel};
use crate::zalsa::ZalsaDatabase;
use crate::{DatabaseKeyIndex, Event, EventKind, Id};

/// Counts semantic requests, including hits; it is not a hash-probe or allocation bound.
const QUERY_KEY_REQUEST_UNITS: usize = 1;

struct KeyRequest<'db, C: InternedQueryConfiguration> {
    input: Option<<C as Configuration>::Input<'db>>,
    commit: Option<PreparedPassiveIntern<'db, C>>,
}

impl<'db, C: InternedQueryConfiguration> KeyRequest<'db, C> {
    fn preparation_bytes() -> Option<usize> {
        size_of::<<C as Configuration>::Input<'db>>()
            .checked_add(size_of::<
                Result<
                    PreparedIntern<'db, C, <C as Configuration>::Input<'db>>,
                    (PreparationError, <C as Configuration>::Input<'db>),
                >,
            >())?
            .checked_add(size_of::<
                PreparedIntern<'db, C, <C as Configuration>::Input<'db>>,
            >())?
            .checked_add(size_of::<Option<<C as Configuration>::Input<'db>>>())?
            .checked_add(size_of::<Option<PreparedPassiveIntern<'db, C>>>())?
            .checked_add(size_of::<Option<PreparedPassiveIntern<'db, C>>>())?
            .checked_add(size_of::<PreparedPassiveIntern<'db, C>>())?
            .checked_add(size_of::<Id>())
    }
}

struct ValueRequest<'db, I: FiniteInternedConfiguration, P = u8> {
    input: Option<I::Fields<'db>>,
    commit: Option<PreparedPassiveIntern<'db, I, P>>,
}

impl<'db, I: FiniteInternedConfiguration, P> ValueRequest<'db, I, P> {
    fn preparation_bytes() -> Option<usize> {
        size_of::<I::Fields<'db>>()
            .checked_add(size_of::<
                Result<PreparedIntern<'db, I, I::Fields<'db>>, (PreparationError, I::Fields<'db>)>,
            >())?
            .checked_add(size_of::<PreparedIntern<'db, I, I::Fields<'db>>>())?
            .checked_add(size_of::<Option<I::Fields<'db>>>())?
            .checked_add(size_of::<Option<PreparedPassiveIntern<'db, I, P>>>())?
            .checked_add(size_of::<Option<PreparedPassiveIntern<'db, I, P>>>())?
            .checked_add(size_of::<PreparedPassiveIntern<'db, I, P>>())?
            .checked_add(size_of::<I::Struct<'db>>())
    }
}

struct PreparedPassiveIntern<'db, I: crate::interned::Configuration, P = u8> {
    commit: InternCommit<'db, I>,
    retirement: PassiveRetirement<P>,
}

#[derive(Clone, Copy)]
struct InternQuote {
    candidate: Option<Id>,
    input_work: usize,
    retirement_work: usize,
}

enum PreparationError {
    Changed,
    Quote(KeyRetirementError),
}

impl From<QuoteError> for PreparationError {
    fn from(error: QuoteError) -> Self {
        Self::Quote(error.into())
    }
}

impl From<KeyRetirementError> for PreparationError {
    fn from(error: KeyRetirementError) -> Self {
        Self::Quote(error)
    }
}

fn grow_quantum(quantum: usize) -> Result<usize, RunError> {
    quantum
        .checked_mul(2)
        .ok_or(RunError::Contract("interned quotation quantum overflow"))
}

fn quotation_bytes<I: crate::interned::Configuration, P>(quantum: usize) -> Option<usize> {
    // Include the selected schema's temporary quote and the fixed returned ticket carriers.
    // At most one selection enum is transferred per paid visit, plus the selected value.
    // This is a conservative logical-copy bound, not a claim of repeated allocation.
    quantum
        .checked_add(1)?
        .checked_mul(size_of::<ReuseInspection<'_, '_, I>>())?
        .checked_add(size_of::<QuoteFuel>())?
        .checked_add(size_of::<InternQuote>())?
        .checked_add(size_of::<InternQuote>())?
        .checked_add(size_of::<Result<InternQuote, KeyRetirementError>>())?
        .checked_add(size_of::<Option<InternQuote>>())?
        .checked_add(size_of::<Result<Option<InternQuote>, RunError>>())?
        .checked_add(size_of::<PassiveRetirement<P>>())?
        .checked_add(size_of::<Result<PassiveRetirement<P>, KeyRetirementError>>())?
        .checked_add(size_of::<Result<usize, QuoteError>>())
}

fn admitted_preparation<M, F>(endpoint: &TaskEndpoint<'_, '_>, make: M) -> Result<F, RunError>
where
    M: FnOnce() -> F,
{
    endpoint.admit_work(1)?;
    endpoint.admit(ExecutionWork::Resource {
        requested_bytes: size_of::<M>()
            .checked_add(size_of::<F>())
            .and_then(|bytes| bytes.checked_add(size_of::<Result<F, RunError>>()))
            .ok_or(RunError::Contract(
                "interned preparation future size overflow",
            ))?,
    })?;
    endpoint.check_completion()?;
    Ok(make())
}

/// Quotes live storage, pays outside its shard lock, then rechecks the actual generation.
/// The request retains all input and any committed detached generation across callback acceptance.
async fn prepare<'call, 'run: 'call, 'db: 'run, I, P>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    db: &'db dyn crate::Database,
    ingredient: &'db crate::interned::IngredientImpl<I>,
    input: &mut Option<I::Fields<'db>>,
    commit: &mut Option<PreparedPassiveIntern<'db, I, P>>,
    check: impl Fn() -> Result<(), RunError>,
    field_work: impl Fn(&I::Fields<'db>, &mut QuoteFuel) -> Result<usize, QuoteError>,
    inspect: impl Fn(
        PassiveMemoInspection<'_>,
        &mut QuoteFuel,
    ) -> Result<PassiveRetirement<P>, KeyRetirementError>,
    empty: impl Fn() -> PassiveRetirement<P>,
    error_message: fn(KeyRetirementError) -> &'static str,
    preparation_bytes: usize,
) -> EntryScope<'call, 'db>
where
    I: crate::interned::Configuration,
{
    let mut quantum = 1;
    loop {
        let quote = endpoint
            .local_call(|| {
                check()?;
                let scope = EntryScope::capture(&endpoint.inner.context)?;
                endpoint.admit_work(1)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: quotation_bytes::<I, P>(quantum)
                        .ok_or(RunError::Contract("interned quotation size overflow"))?,
                })?;
                endpoint.admit_work(quantum)?;
                endpoint.check_completion()?;
                if !scope.is_current() {
                    return Err(RunError::Contract(
                        "execution entry changed its enclosing scope",
                    ));
                }
                let mut fuel = QuoteFuel::admitted(quantum);
                let input = input
                    .as_ref()
                    .ok_or(RunError::Contract("interned request lost its input"))?;
                let input_work = match field_work(input, &mut fuel) {
                    Ok(units) => units,
                    Err(QuoteError::Exhausted) => return Ok(None),
                    Err(error) => return Err(RunError::Contract(error_message(error.into()))),
                };
                if input_work != 0 {
                    endpoint.admit_work(input_work)?;
                }
                endpoint.check_completion()?;
                if !scope.is_current() {
                    return Err(RunError::Contract(
                        "execution entry changed its enclosing scope",
                    ));
                }
                let mut quote = InternQuote {
                    candidate: None,
                    input_work,
                    retirement_work: 0,
                };
                let result = (|| {
                    ingredient.inspect_intern(db.zalsa(), input, |step| {
                        match step {
                            ReuseInspection::Visit | ReuseInspection::Vacant => fuel.consume(1)?,
                            ReuseInspection::Selected { id, fields, memos } => {
                                let fields = field_work(fields, &mut fuel)?;
                                let memos = inspect(PassiveMemoInspection::new(memos), &mut fuel)?;
                                quote.candidate = Some(id);
                                quote.retirement_work = fields
                                    .checked_add(memos.output_units())
                                    .ok_or(QuoteError::Overflow)?;
                            }
                        }
                        Ok::<(), KeyRetirementError>(())
                    })?;
                    Ok::<_, KeyRetirementError>(quote)
                })();
                match result {
                    Ok(quote) => Ok(Some(quote)),
                    Err(KeyRetirementError::Quote(QuoteError::Exhausted)) => Ok(None),
                    Err(error) => Err(RunError::Contract(error_message(error))),
                }
            })
            .await;
        let Some(quote) = quote else {
            quantum = endpoint
                .local_call(|| {
                    endpoint.admit_work(1)?;
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: size_of::<usize>() + size_of::<Result<usize, RunError>>(),
                    })?;
                    grow_quantum(quantum)
                })
                .await;
            continue;
        };

        let outcome = endpoint
            .local_call(|| {
                check()?;
                let scope = EntryScope::capture(&endpoint.inner.context)?;
                // Logical transfers are cumulative byte requests even when storage is reused.
                endpoint.admit_work(1)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: preparation_bytes,
                })?;
                if quote.input_work != 0 {
                    endpoint.admit_work(quote.input_work)?;
                }
                if quote.retirement_work != 0 {
                    endpoint.admit_work(quote.retirement_work)?;
                }
                endpoint.admit_work(1)?;
                endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: quantum
                        .checked_add(1)
                        .and_then(|visits| {
                            visits.checked_mul(size_of::<ReuseInspection<'_, '_, I>>())
                        })
                        .and_then(|bytes| bytes.checked_add(size_of::<QuoteFuel>()))
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<
                                Result<Result<EntryScope<'call, 'db>, bool>, RunError>,
                            >())
                        })
                        .and_then(|bytes| bytes.checked_add(size_of::<PassiveRetirement<P>>()))
                        .and_then(|bytes| bytes.checked_add(size_of::<PassiveRetirement<P>>()))
                        .and_then(|bytes| {
                            bytes.checked_add(size_of::<
                                Result<PassiveRetirement<P>, KeyRetirementError>,
                            >())
                        })
                        .ok_or(RunError::Contract("interned reinspection size overflow"))?,
                })?;
                endpoint.admit_work(quantum)?;
                endpoint.check_completion()?;
                if !scope.is_current() {
                    return Err(RunError::Contract(
                        "execution entry changed its enclosing scope",
                    ));
                }
                let mut fuel = QuoteFuel::admitted(quantum);
                let submitted = input
                    .take()
                    .ok_or(RunError::Contract("interned request lost its input"))?;
                let mut retirement = empty();
                let prepared = ingredient.prepare_intern_checked(
                    db.zalsa(),
                    db.zalsa_local(),
                    submitted,
                    |_, input| input,
                    |step| {
                        match step {
                            ReuseInspection::Visit => fuel.consume(1)?,
                            ReuseInspection::Vacant => {
                                fuel.consume(1)?;
                                if quote.candidate.is_some() {
                                    return Err(PreparationError::Changed);
                                }
                            }
                            ReuseInspection::Selected { id, fields, memos } => {
                                if quote.candidate != Some(id) {
                                    return Err(PreparationError::Changed);
                                }
                                let fields = field_work(fields, &mut fuel)?;
                                let current =
                                    inspect(PassiveMemoInspection::new(memos), &mut fuel)?;
                                let work = fields
                                    .checked_add(current.output_units())
                                    .ok_or(QuoteError::Overflow)?;
                                if work > quote.retirement_work {
                                    return Err(PreparationError::Changed);
                                }
                                retirement = current;
                            }
                        }
                        Ok(())
                    },
                );
                match prepared {
                    Ok(prepared) => {
                        *input = prepared.unused_input;
                        *commit = Some(PreparedPassiveIntern {
                            commit: prepared.commit,
                            retirement,
                        });
                        Ok(Ok(scope))
                    }
                    Err((error, returned)) => {
                        *input = Some(returned);
                        match error {
                            PreparationError::Changed => Ok(Err(false)),
                            PreparationError::Quote(KeyRetirementError::Quote(
                                QuoteError::Exhausted,
                            )) => Ok(Err(true)),
                            PreparationError::Quote(error) => {
                                Err(RunError::Contract(error_message(error)))
                            }
                        }
                    }
                }
            })
            .await;
        match outcome {
            Ok(scope) => return scope,
            Err(grow) => {
                if grow {
                    quantum = endpoint
                        .local_call(|| {
                            endpoint.admit_work(1)?;
                            endpoint.admit(ExecutionWork::Resource {
                                requested_bytes: size_of::<usize>()
                                    + size_of::<Result<usize, RunError>>(),
                            })?;
                            grow_quantum(quantum)
                        })
                        .await;
                }
            }
        }
    }
}

#[cfg(test)]
pub(super) fn key_preparation_layout_for_tests<'db, C: InternedQueryConfiguration>()
-> [(&'static str, usize); 7] {
    [
        ("Input", size_of::<<C as Configuration>::Input<'db>>()),
        (
            "Result",
            size_of::<
                Result<
                    PreparedIntern<'db, C, <C as Configuration>::Input<'db>>,
                    (PreparationError, <C as Configuration>::Input<'db>),
                >,
            >(),
        ),
        (
            "PreparedIntern",
            size_of::<PreparedIntern<'db, C, <C as Configuration>::Input<'db>>>(),
        ),
        (
            "Option<Input>",
            size_of::<Option<<C as Configuration>::Input<'db>>>(),
        ),
        (
            "Option<PreparedPassiveIntern>",
            size_of::<Option<PreparedPassiveIntern<'db, C>>>(),
        ),
        (
            "PreparedPassiveIntern",
            size_of::<PreparedPassiveIntern<'db, C>>(),
        ),
        ("Id", size_of::<Id>()),
    ]
}

#[cfg(test)]
pub(super) fn key_layout_for_tests<'db, C, P>() -> [(&'static str, usize, usize); 7]
where
    C: InternedQueryConfiguration,
{
    [
        (
            "QueryKeys",
            size_of::<QueryKeys<'db, C, P>>(),
            align_of::<QueryKeys<'db, C, P>>(),
        ),
        (
            "InternCommit",
            size_of::<InternCommit<'db, C>>(),
            align_of::<InternCommit<'db, C>>(),
        ),
        (
            "PassiveRetirement",
            size_of::<PassiveRetirement>(),
            align_of::<PassiveRetirement>(),
        ),
        (
            "PreparedPassiveIntern",
            size_of::<PreparedPassiveIntern<'db, C>>(),
            align_of::<PreparedPassiveIntern<'db, C>>(),
        ),
        (
            "Option<InternCommit>",
            size_of::<Option<InternCommit<'db, C>>>(),
            align_of::<Option<InternCommit<'db, C>>>(),
        ),
        (
            "Option<PreparedPassiveIntern>",
            size_of::<Option<PreparedPassiveIntern<'db, C>>>(),
            align_of::<Option<PreparedPassiveIntern<'db, C>>>(),
        ),
        (
            "KeyRequest",
            size_of::<KeyRequest<'db, C>>(),
            align_of::<KeyRequest<'db, C>>(),
        ),
    ]
}

#[cfg(test)]
pub(super) fn value_layout_for_tests<'db, I, S>() -> [(&'static str, usize, usize); 8]
where
    I: FiniteInternedConfiguration,
    S: PassiveMemoSchema<'db, I>,
{
    [
        ("Schema", size_of::<S>(), align_of::<S>()),
        (
            "Presence",
            size_of::<S::Presence>(),
            align_of::<S::Presence>(),
        ),
        (
            "InternedValues",
            size_of::<InternedValues<'db, I, S>>(),
            align_of::<InternedValues<'db, I, S>>(),
        ),
        (
            "PassiveRetirement",
            size_of::<PassiveRetirement<S::Presence>>(),
            align_of::<PassiveRetirement<S::Presence>>(),
        ),
        (
            "InternCommit",
            size_of::<InternCommit<'db, I>>(),
            align_of::<InternCommit<'db, I>>(),
        ),
        (
            "PreparedPassiveIntern",
            size_of::<PreparedPassiveIntern<'db, I, S::Presence>>(),
            align_of::<PreparedPassiveIntern<'db, I, S::Presence>>(),
        ),
        (
            "Option<PreparedPassiveIntern>",
            size_of::<Option<PreparedPassiveIntern<'db, I, S::Presence>>>(),
            align_of::<Option<PreparedPassiveIntern<'db, I, S::Presence>>>(),
        ),
        (
            "ValueRequest",
            size_of::<ValueRequest<'db, I, S::Presence>>(),
            align_of::<ValueRequest<'db, I, S::Presence>>(),
        ),
    ]
}

pub(super) async fn intern_value<'call, 'run: 'call, 'db: 'run, I, S>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    values: &'call InternedValues<'db, I, S>,
    input: I::Fields<'db>,
) -> I::Struct<'db>
where
    I: FiniteInternedConfiguration,
    S: PassiveMemoSchema<'db, I> + 'call,
{
    endpoint
        .local_call(|| {
            endpoint.check_finite_interned_values(values)?;
            endpoint.admit_work(QUERY_KEY_REQUEST_UNITS)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: size_of::<ValueRequest<'db, I, S::Presence>>(),
            })?;
            endpoint.check_completion()
        })
        .await;
    let mut request = ValueRequest::<I, S::Presence> {
        input: Some(input),
        commit: None,
    };
    let preparation_bytes = endpoint
        .local_call(|| {
            endpoint.admit_work(1)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: size_of::<usize>() + size_of::<Result<usize, RunError>>(),
            })?;
            ValueRequest::<'db, I, S::Presence>::preparation_bytes().ok_or(RunError::Contract(
                "finite interned value preparation size overflow",
            ))
        })
        .await;
    let scope = endpoint
        .local_call(|| {
            admitted_preparation(endpoint, || {
                prepare(
                    endpoint,
                    values.db(),
                    values.ingredient,
                    &mut request.input,
                    &mut request.commit,
                    || endpoint.check_finite_interned_values(values),
                    I::field_work_bounded,
                    |table, fuel| values.memos.inspect(table, fuel),
                    S::empty_retirement,
                    KeyRetirementError::value_message,
                    preparation_bytes,
                )
            })
        })
        .await
        .await;

    let Some(prepared) = request.commit.take() else {
        match callback::reject(
            &endpoint.inner,
            RunError::Contract("finite interned value completed without a commit"),
            request,
        )
        .await {}
    };
    let commit = &prepared.commit;
    read_run::dependency(
        endpoint,
        &scope,
        values.db().zalsa_local(),
        &commit.dependency_read(),
    )
    .await;

    if let Some(old_id) = commit.retired_id() {
        for slot in 0..S::LEN {
            if let Some(function) = values.memos.discard_function_at(slot, &prepared.retirement) {
                let key = DatabaseKeyIndex::new(function, old_id);
                endpoint
                    .local_call(|| {
                        values
                            .db()
                            .zalsa()
                            .event(&|| Event::new(EventKind::DidDiscard { key }));
                        Ok(())
                    })
                    .await;
            }
        }
    }
    if commit.has_event() {
        endpoint
            .local_call(|| {
                commit.emit_event(values.db().zalsa());
                Ok(())
            })
            .await;
    }
    let value = <I::Struct<'db> as FromId>::from_id(commit.id());
    // Only passive field and checked memo destruction remains after final acceptance.
    drop(prepared);
    drop(request);
    value
}

pub(super) async fn intern<'call, 'run: 'call, 'db: 'run, C, P>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    keys: &'call QueryKeys<'db, C, P>,
    input: <C as Configuration>::Input<'db>,
) -> Id
where
    C: InternedQueryConfiguration,
    P: QueryKeyProfile<C> + 'call,
{
    endpoint
        .local_call(|| {
            endpoint.check_query_keys(keys)?;
            endpoint.admit_work(QUERY_KEY_REQUEST_UNITS)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: size_of::<KeyRequest<'db, C>>(),
            })?;
            endpoint.check_completion()
        })
        .await;
    // The request lives outside every callback. A failed post-commit acceptance therefore
    // retains the detached generation until the driver's queued children have retired.
    let mut request = KeyRequest {
        input: Some(input),
        commit: None,
    };
    let preparation_bytes = endpoint
        .local_call(|| {
            endpoint.admit_work(1)?;
            endpoint.admit(ExecutionWork::Resource {
                requested_bytes: size_of::<usize>() + size_of::<Result<usize, RunError>>(),
            })?;
            KeyRequest::<'db, C>::preparation_bytes()
                .ok_or(RunError::Contract("query key preparation size overflow"))
        })
        .await;
    let scope = endpoint
        .local_call(|| {
            admitted_preparation(endpoint, || {
                prepare(
                    endpoint,
                    keys.db().as_dyn_database(),
                    keys.argument,
                    &mut request.input,
                    &mut request.commit,
                    || endpoint.check_query_keys(keys),
                    P::input_work_bounded,
                    |table, fuel| keys.memos.0.inspect_singleton(table, fuel),
                    PassiveRetirement::empty,
                    KeyRetirementError::message,
                    preparation_bytes,
                )
            })
        })
        .await
        .await;

    let Some(prepared) = request.commit.take() else {
        match callback::reject(
            &endpoint.inner,
            RunError::Contract("fixed query key completed without a commit"),
            request,
        )
        .await {}
    };
    let commit = &prepared.commit;
    read_run::dependency(
        endpoint,
        &scope,
        keys.db().zalsa_local(),
        &commit.dependency_read(),
    )
    .await;

    if let Some(old_id) = commit.retired_id()
        && let Some(function) = keys.memos.discard_function_at(0, &prepared.retirement)
    {
        let key = DatabaseKeyIndex::new(function, old_id);
        endpoint
            .local_call(|| {
                keys.db()
                    .zalsa()
                    .event(&|| Event::new(EventKind::DidDiscard { key }));
                Ok(())
            })
            .await;
    }
    if commit.has_event() {
        endpoint
            .local_call(|| {
                commit.emit_event(keys.db().zalsa());
                Ok(())
            })
            .await;
    }
    let id = commit.id();
    // The profile certifies passive field and output cleanup. No callback follows this
    // passive retirement inside the key request, and no live-slot memo table is touched.
    drop(prepared);
    drop(request);
    id
}
