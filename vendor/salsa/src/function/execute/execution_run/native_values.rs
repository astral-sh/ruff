use std::future::ready;

use super::callback::{CallbackKind, CallbackOwner};
use super::{Endpoint, ExecutionProvider, ExecutionWork, RunError, callback};
use crate::Id;
use crate::function::Configuration;
pub use crate::function::RetainedInput;
use crate::zalsa::ZalsaDatabase;

/// A canonical native operation whose operands remain owned by the runtime.
///
/// Comparison operands have the same order as the ensuing `Configuration::values_equal` call.
/// Providers quote that call; they cannot replace its answer or either operand.
pub enum NativeValueOperation<'call, 'db, C: Configuration> {
    InputConversion(RetainedInput<'call, 'db, C>),
    Comparison {
        left: &'call C::Output<'db>,
        right: &'call C::Output<'db>,
    },
}

/// Bounds for one native operation and the cleanup of ownership it creates.
///
/// `work` bounds the conversion or comparison itself. `requested_bytes` includes every allocation
/// the operation can request. `cleanup_work` prepays destruction of newly owned inputs, including
/// their nested payloads, so later refusal does not need another allowance. Comparison borrows
/// both values; production and cleanup of its candidate must already have been admitted by the
/// producer. Old memo retirement remains owned by the ordinary revision lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeValueQuote {
    pub work: usize,
    pub requested_bytes: usize,
    pub cleanup_work: usize,
}

pub(super) async fn admit<'call, 'run: 'call, 'db: 'run, C, P, O>(
    endpoint: &'call Endpoint<'run, 'db>,
    owner: &'call O,
    provider: &'call P,
    db: &'db C::DbView,
    operation: NativeValueOperation<'call, 'db, C>,
) where
    C: Configuration,
    P: ExecutionProvider<'run, 'db, C>,
    O: CallbackOwner,
{
    let quote = callback::complete(endpoint, owner, CallbackKind::NativeQuotation, || {
        provider.native_value(db, operation, endpoint.clone())
    })
    .await;
    admit_quote(endpoint, owner, quote).await;
}

pub(super) async fn admit_quote(
    endpoint: &Endpoint<'_, '_>,
    owner: &impl CallbackOwner,
    quote: NativeValueQuote,
) {
    callback::complete(endpoint, owner, CallbackKind::NativeAdmission, || {
        ready(
            quote
                .work
                .checked_add(quote.cleanup_work)
                .ok_or(RunError::Contract("native value work overflow"))
                .and_then(|units| endpoint.admit_work(units))
                .and_then(|()| {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: quote.requested_bytes,
                    })
                }),
        )
    })
    .await;
}

pub(super) async fn input<'call, 'run: 'call, 'db: 'run, C, P, O>(
    endpoint: &'call Endpoint<'run, 'db>,
    owner: &'call O,
    provider: &'call P,
    db: &'db C::DbView,
    id: Id,
) -> C::Input<'db>
where
    C: Configuration,
    P: ExecutionProvider<'run, 'db, C>,
    O: CallbackOwner,
{
    let input = callback::complete(endpoint, owner, CallbackKind::NativeQuotation, || {
        ready(C::retained_input(db.zalsa(), id).ok_or(RunError::Contract(
            "input conversion has no retained input accessor",
        )))
    })
    .await;
    admit::<C, _, _>(
        endpoint,
        owner,
        provider,
        db,
        NativeValueOperation::InputConversion(input),
    )
    .await;
    callback::complete(endpoint, owner, CallbackKind::NativeCall, || {
        ready(Ok(C::id_to_input(db.zalsa(), id)))
    })
    .await
}
