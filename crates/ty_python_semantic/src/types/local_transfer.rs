//! Admission for fixed local callback and result transfers, in addition to an operation's payload.

pub(in crate::types) mod collections;
pub(in crate::types) mod context_variables;
pub(in crate::types) mod hash_table;
pub(in crate::types) mod names;
pub(in crate::types) mod scopes;

use std::future::Future;
use std::pin::Pin;

use salsa::execution_probe::{
    ExecutionWork, FieldRequest, FieldRequestContext, RunError, RunResult, TaskEndpoint,
};

/// Quotes the audited generated request construction for an admitted native field-read future.
/// Returns `(logical work, requested bytes)` or an error if the quotation overflows.
/// The factories identify opaque accessor/request types and are never invoked. Callers use
/// noncapturing factories or borrowed forwarding adapters.
/// This bound covers the mapping fields for aliases and specializations, and bound type-variable
/// identity/kind, including their thin request wrappers, plus static-class body scope and the
/// tracked `ScopeId::file_scope_id` and `Definition::kind` requests without a wrapper.
/// The latter uses a present field index and borrows its payload. Generic-context variables
/// use one thin wrapper around an interned borrowed field; `GenericContext::program` is a direct
/// interned copied field. Unbound type-variable identity
/// and the underlying variable of a bound occurrence use interned copied fields.
/// Function literal/descriptor and retained updated-signature requests, and callable
/// signature/kind/deprecation requests, use the same interned shapes. The retained update request
/// has one thin wrapper; the other function/callable requests are direct.
/// `FileModule::search_path_request` uses an interned borrowed field and one forwarding
/// wrapper around the generated request. It borrows the stored `SearchPath` without cloning it.
/// The direct copied input requests for project and mdtest verbose settings use the generated
/// input accessor and its fixed field index; they do not read a prepared settings memo.
/// Exact-tuple payloads use a direct interned borrowed field. Static-class known metadata,
/// generic-alias origins and explicit-Any instance classes use direct interned copied fields.
/// `ScopeId::program_file` is a direct tracked copied field, and `ProgramFile::program`
/// is a direct interned copied field.
/// Other paths need their own audit.
///
/// Three accessor, eight context/request, and two input/output/result representations cover
/// construction and forwarding into the read future; four reference slots cover endpoint/profile
/// arguments. Work counts request construction (20) and forwarding/results (7), independently of
/// those widths. The type-width arithmetic is evaluated at compile time, so no runtime quotation
/// preparation is charged.
/// The tracked request constructs and installs an optional index instead of calling a thin wrapper;
/// its actual request width and the spare wrapper-result slot cover that additional carrier.
/// The caller uses `boxed_future_with_fixed_transfers_at` to admit and box the read future,
/// then awaits it with its original field profile, which separately admits selection/conversion.
pub fn generated_field_quote<'db, H, A, R: FieldRequest<'db>>(
    _accessor: impl FnOnce(H, FieldRequestContext<'db>) -> A,
    _request: impl FnOnce(H, FieldRequestContext<'db>) -> R,
) -> RunResult<(usize, usize)> {
    const {
        match collections::fixed_quote(
            27,
            [
                (3, size_of::<A>()),
                (8, size_of::<FieldRequestContext<'db>>()),
                (2, size_of::<H>()),
                (8, size_of::<R>()),
                (2, size_of::<R::Output>()),
                (2, size_of::<RunResult<R::Output>>()),
                (4, size_of::<&TaskEndpoint<'_, 'db>>()),
            ],
        ) {
            Some(quote) => Ok(quote),
            None => Err(RunError::Contract("generated field quotation overflow")),
        }
    }
}

/// Constructs and returns a boxed child future after admitting its storage and fixed transfers.
/// `quote` supplies `(logical work, requested bytes)` for the factory's operation; the child
/// funds variable-sized payloads and their destruction. The factory must keep owners needed
/// by queued children alive: [`TaskEndpoint::local_call`] cannot retain values the factory
/// destroys synchronously. The fixed bound allows up to three whole-future return transfers
/// inside the factory; additional forwarding belongs in `quote`. On quotation or admission
/// failure, captures remain in the enclosing future until the endpoint drains pending children.
pub async fn boxed_future_with_fixed_transfers_at<F: Future, M: FnOnce() -> F>(
    endpoint: &TaskEndpoint<'_, '_>,
    quote: RunResult<(usize, usize)>,
    make: M,
) -> RunResult<Pin<Box<F>>> {
    let quote = quote.and_then(|(work, bytes)| {
        // Eight future representations cover construction, up to three factory return
        // transfers, the by-value Box::pin/Box::new/write_via_move arguments, and heap storage.
        // Four output representations cover child return, await, adapter return, and shared
        // caller. The local-transfer helper charges the boxed result and retained factory.
        let bytes = size_of::<F>()
            .checked_mul(8)
            .and_then(|n| n.checked_add(size_of::<M>().checked_mul(2)?))
            .and_then(|n| n.checked_add(size_of::<F::Output>().checked_mul(4)?))
            .and_then(|n| n.checked_add(size_of::<Pin<Box<F>>>().checked_mul(2)?))
            .and_then(|n| n.checked_add(20 * size_of::<usize>()))
            .and_then(|n| n.checked_add(20 * size_of::<Option<usize>>()))
            .and_then(|n| n.checked_add(bytes))
            .ok_or(RunError::Contract("boxed future byte quotation overflow"))?;
        // Twenty operations bound checked quotation and result handling; twelve bound
        // construction, boxing, fixed transfers and retirement, independently of their widths.
        let work = work
            .checked_add(32)
            .ok_or(RunError::Contract("boxed future work quotation overflow"))?;
        Ok((work, bytes))
    });
    local_quoted_with_fixed_transfers_at(endpoint, quote, || Box::pin(make())).await
}

/// Executes a local action after admitting its operation and fixed callback/result transfers.
/// Payload allocation and cleanup remain the caller's responsibility. An enclosing admitted
/// future pays for its storage; this charge covers each initialization and transfer even when
/// the action reuses that storage in a loop.
pub async fn local_with_fixed_transfers_at<T, F: FnOnce() -> T>(
    endpoint: &TaskEndpoint<'_, '_>,
    work: usize,
    requested_bytes: usize,
    action: F,
) -> RunResult<T> {
    local_quoted_with_fixed_transfers_at(endpoint, Ok((work, requested_bytes)), action).await
}

/// Executes a quoted local action, retaining captured owners through failed admission.
/// The fixed quote includes the retained optional factory, callback captures, and nested results.
/// It does not traverse owned payloads; their allocation and cleanup remain the caller's responsibility.
pub(in crate::types) async fn local_quoted_with_fixed_transfers_at<T, F: FnOnce() -> T>(
    endpoint: &TaskEndpoint<'_, '_>,
    quote: RunResult<(usize, usize)>,
    action: F,
) -> RunResult<T> {
    let quote = quote.and_then(|(work, bytes)| {
        // The bound covers this helper and up to three forwarding adapters, including Some/take
        // of the factory and callback/result returns. The callback retains a quote and two
        // borrows; plain wrapper arguments use eight pointer-width scalar slots. These scalar
        // representations contribute bytes only; work counts the bounded operations separately.
        type Captures<'a, 'run, 'db, F> = (
            RunResult<(usize, usize)>,
            &'a mut Option<F>,
            &'a TaskEndpoint<'run, 'db>,
        );
        let bytes = size_of::<F>()
            .checked_mul(8)
            .and_then(|n| n.checked_add(size_of::<Option<F>>().checked_mul(2)?))
            .and_then(|n| n.checked_add(size_of::<T>().checked_mul(3)?))
            .and_then(|n| n.checked_add(size_of::<RunResult<T>>().checked_mul(8)?))
            .and_then(|n| n.checked_add(size_of::<Captures<'_, '_, '_, F>>().checked_mul(2)?))
            .and_then(|n| n.checked_add(size_of::<RunResult<(usize, usize)>>().checked_mul(4)?))
            .and_then(|n| n.checked_add(size_of::<usize>().checked_mul(8)?))
            .and_then(|n| n.checked_add(bytes))
            .ok_or(RunError::Contract("local transfer byte quotation overflow"))?;
        let work = work
            .checked_add(64)
            .ok_or(RunError::Contract("local transfer work quotation overflow"))?;
        Ok((work, bytes))
    });
    // Keep the captured owner in this future while the existing endpoint drains pending children.
    let mut action = Some(action);
    Ok(endpoint
        .local_call(|| {
            let (work, requested_bytes) = quote?;
            endpoint.admit_work(work)?;
            endpoint.admit(ExecutionWork::Resource { requested_bytes })?;
            endpoint.check_completion()?;
            let action = action
                .take()
                .ok_or(RunError::Contract("local factory was consumed"))?;
            Ok(action())
        })
        .await)
}
