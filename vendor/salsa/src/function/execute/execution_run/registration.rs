//! Experimental provider registration on the runtime-owned task stack.
//!
//! Executable routes fetch and validate memos through the shared runtime states, including cold
//! selection and registered cycle callbacks. Proof-only routes dispatch callbacks without query
//! ownership. Generated query entries are not connected to this interface. Owning execution
//! requests remain private to the runtime.

use std::alloc::Layout;
use std::any::TypeId;
use std::cell::OnceCell;
use std::future::{Future, ready};
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll};

use super::admission::Admission;
pub use super::explicit_reads::{OrdinaryReadViolation, with_explicit_reads};
pub use super::field_run::{
    BorrowOrCopy, FieldReadProfile, FieldRequest, FieldRequestContext, FieldReturnMode,
    InputFieldRequest, InternedFieldRequest, TrackedFieldRequest,
};
use super::frame_free::EntryScope;
pub use super::native_source::{NativeCallbackEntry, NativeCallbackLimits, with_native_callback};
pub use super::native_values::{NativeValueOperation, NativeValueQuote, RetainedInput};
use super::passive_memos::SchemaOps;
pub use super::passive_memos::{
    PassiveMemo, PassiveMemoGroup, PassiveMemoInspection, PassiveMemoSchema, PassiveRetirement,
};
pub use super::prepared_source_run::{PreparedSourceRead, PreparedSourceRequest};
pub use super::task::{task_layout, task_state_layout};
use super::{
    Driver, Endpoint, ExecutionProvider, Reply, RunContext, callback, explicit_reads, fetch_run,
    key_run, native_source, source_run, structural_dependencies, validation_run,
};
pub use super::{ExecutionAdmission, ExecutionWork, RunError, RunResult};
use crate::attempt_probe;
pub use crate::attempt_probe::{
    ExecutionBudget, ExecutionLimits, ExecutionReceipt, ExecutionUsage, try_with_execution_budget,
    try_with_metered_execution_budget,
};
pub use crate::function::maybe_changed_after::VerifyResult;
pub use crate::function::memo::{
    FinalSourceError, FinalSourceMemo, PreparedSourceError, PreparedSourceMemo,
};
use crate::function::{
    Configuration, CopyMemoProfile, FixedQueryFields, IngredientImpl, InternedQueryConfiguration,
};
pub use crate::function::{FixedQueryKeyProfile, PassiveMemoProfile, QueryKeyProfile};
use crate::ingredient::Ingredient;
use crate::interned::FiniteInternedConfiguration;
use crate::zalsa::{IngredientIndex, ZalsaDatabase};
use crate::{Cycle, Database, DatabaseKeyIndex, Id, Revision};
use rustc_hash::FxHashMap;

/// A typed route does not retain a provider or any query ownership guard.
pub struct Route<'db, C: Configuration> {
    identity: Rc<()>,
    slot: usize,
    db: &'db C::DbView,
    ingredient: &'db IngredientImpl<C>,
    policy: attempt_probe::OperationPolicy,
}

impl<C: Configuration> Clone for Route<'_, C> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            slot: self.slot,
            db: self.db,
            ingredient: self.ingredient,
            policy: self.policy,
        }
    }
}

impl<C: Configuration> Route<'_, C> {
    pub fn database_key(&self, id: Id) -> DatabaseKeyIndex {
        self.ingredient.database_key_index(id)
    }
}

/// A typed query capability whose binding does not own the target provider.
pub struct CallableRoute<'run, 'db: 'run, C: Configuration> {
    route: Route<'db, C>,
    state: Rc<CallableState<'run, 'db, C>>,
}

impl<C: Configuration> Clone for CallableRoute<'_, '_, C> {
    fn clone(&self) -> Self {
        Self {
            route: self.route.clone(),
            state: self.state.clone(),
        }
    }
}

impl<C: Configuration> CallableRoute<'_, '_, C> {
    pub fn database_key(&self, id: Id) -> DatabaseKeyIndex {
        self.route.database_key(id)
    }
}

struct CallableState<'run, 'db: 'run, C: Configuration> {
    factory: OnceCell<Weak<dyn CallableFactory<'run, 'db, C> + 'run>>,
}

/// The real generated argument ingredient for one registered query.
pub struct QueryKeys<'db, C: InternedQueryConfiguration, P> {
    route: Route<'db, C>,
    pub(super) argument: &'db crate::interned::IngredientImpl<C>,
    pub(super) memos: (PassiveMemo<'db, C, C, P>,),
}

/// The fixed-input, Copy-output compatibility token.
pub type FixedQueryKeys<'db, C> = QueryKeys<'db, C, FixedQueryKeyProfile>;

impl<'db, C: InternedQueryConfiguration, P> QueryKeys<'db, C, P> {
    pub(super) fn db(&self) -> &'db C::DbView {
        self.route.db
    }
}

/// A canonical interner with finite fields and its complete checked passive memo schema.
pub struct InternedValues<'db, I: FiniteInternedConfiguration, S> {
    identity: Rc<()>,
    db: &'db dyn Database,
    pub(super) ingredient: &'db crate::interned::IngredientImpl<I>,
    pub(super) memos: S,
}

/// The single Copy-output memo compatibility token.
pub type FiniteInternedValues<'db, I, M> =
    InternedValues<'db, I, (PassiveMemo<'db, I, M, CopyMemoProfile>,)>;

impl<'db, I: FiniteInternedConfiguration, S> InternedValues<'db, I, S> {
    pub(super) fn db(&self) -> &'db dyn Database {
        self.db
    }
}

/// Exact prepared source keys, with no executable query provider.
pub struct FinalSourceRoute<'db, C: Configuration> {
    identity: Rc<()>,
    slot: usize,
    pub(super) sources: Rc<FinalSources<'db, C>>,
}

impl<C: Configuration> Clone for FinalSourceRoute<'_, C> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            slot: self.slot,
            sources: self.sources.clone(),
        }
    }
}

impl<C: Configuration> FinalSourceRoute<'_, C> {
    #[cfg(test)]
    pub(super) fn prepared_capacity(&self) -> usize {
        self.sources.prepared.capacity()
    }
}

pub(super) struct FinalSources<'db, C: Configuration> {
    pub(super) db: &'db C::DbView,
    pub(super) ingredient: &'db IngredientImpl<C>,
    pub(super) prepared: Vec<FinalSourceMemo<'db, C>>,
}

/// Runs an audited source query through its canonical native fetch and validation paths.
/// ReturnOnly permits refusal; the consumer must separately ensure these are source keys.
pub struct NativeSourceRoute<'db, C: Configuration> {
    identity: Rc<()>,
    slot: usize,
    pub(super) db: &'db C::DbView,
    pub(super) ingredient: &'db IngredientImpl<C>,
}

impl<C: Configuration> Clone for NativeSourceRoute<'_, C> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            slot: self.slot,
            db: self.db,
            ingredient: self.ingredient,
        }
    }
}

/// The runtime assigns identity independently of the provider's address or Rust type.
pub struct ProviderBinding<'run, P> {
    identity: Rc<()>,
    ordinal: usize,
    provider: &'run P,
}

impl<P> Clone for ProviderBinding<'_, P> {
    fn clone(&self) -> Self {
        Self {
            identity: self.identity.clone(),
            ordinal: self.ordinal,
            provider: self.provider,
        }
    }
}

/// Callback registration is separate from a query's attempt policy.
/// Neither callback may be used as a substitute for the missing fetch/validation protocol.
pub trait RouteProvider<'run, 'db: 'run, C: Configuration>: Sized + 'run {
    fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db C::DbView,
        input: C::Input<'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'run;

    fn verify(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db C::DbView,
        id: Id,
        revision: Revision,
    ) -> impl Future<Output = RunResult<VerifyResult>> + 'run;
}

/// Required callbacks for runtime-owned validation and reexecution. There is no validity callback
/// or synchronous default: the runtime verifies dependencies and compares the completed memo.
///
/// Callbacks retain owned inputs and provisional values outside `local_call` and `child_call`
/// actions. Perform fallible work inside those protected boundaries so children queued by an
/// observer drain before an error or cancellation destroys callback-local owners. Construct
/// callback futures passively, transferring those owners into the future without fallible work.
pub trait ExecutableRouteProvider<'run, 'db: 'run, C: Configuration>: Sized + 'run {
    /// Quotes a native conversion or comparison without performing it.
    ///
    /// Constructing the returned future must be passive and allocation-free: its inline storage
    /// is included in the admitted execution task. When polled, admit every traversal step and
    /// dynamic cursor allocation through the endpoint before doing that work. Bounds must cover
    /// actual retained payloads, including values produced by ordinary queries. Refuse unsupported
    /// shapes; `Copy`, inline size and heap-size telemetry do not certify native Clone or Eq.
    /// Quotation inspects retained data without executing semantic queries or user Clone/Eq code.
    /// Retain owned quote cursors outside `local_call` or `child_call` actions and perform fallible
    /// admission inside those boundaries: returning an error directly can destroy a cursor before
    /// children queued by an observer have drained. Admit cursor cleanup before acquiring it.
    fn native_value<'call>(
        &'run self,
        _context: ProviderContext<'run, 'db, Self>,
        _db: &'db C::DbView,
        _operation: NativeValueOperation<'call, 'db, C>,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract(
            "native value operation has no profile",
        )))
    }

    fn body(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db C::DbView,
        input: C::Input<'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'run;

    fn initial(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db C::DbView,
        id: Id,
        input: C::Input<'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'run;

    fn recover<'call>(
        &'run self,
        context: ProviderContext<'run, 'db, Self>,
        db: &'db C::DbView,
        cycle: &'call Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call;
}

/// An owned provider whose callbacks borrow it only while their futures run.
///
/// Providers may hold callable routes to one another. Keep endpoints callback-local and provider
/// destruction passive: binding failure and registry teardown use ordinary Rust drop semantics.
/// Owned callback inputs and provisional values follow the protected-action retention contract
/// of [`ExecutableRouteProvider`].
pub trait CallableRouteProvider<'run, 'db: 'run, C: Configuration>: Sized + 'run {
    /// Quotes a borrowed native operation under the same contract as
    /// [`ExecutableRouteProvider::native_value`]. The runtime retains its operands until the
    /// quotation, admission and canonical callback have each completed successfully.
    fn native_value<'call>(
        &'call self,
        _endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db C::DbView,
        _operation: NativeValueOperation<'call, 'db, C>,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call
    where
        'run: 'call,
    {
        ready(Err(RunError::Contract(
            "native value operation has no profile",
        )))
    }

    fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db C::DbView,
        input: C::Input<'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call;

    fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db C::DbView,
        id: Id,
        input: C::Input<'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call;

    fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db C::DbView,
        cycle: &'call Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call;
}

struct RouteKey {
    ingredient: IngredientIndex,
    configuration: TypeId,
}

enum RouteEntry<'run, 'db: 'run> {
    Reserved(RouteKey),
    CallableReserved(RouteKey),
    Bound {
        key: RouteKey,
        provider: usize,
        factory: Box<dyn ValidationFactory<'run, 'db> + 'run>,
    },
    Executable {
        key: RouteKey,
        provider: usize,
        factory: Box<dyn ValidationFactory<'run, 'db> + 'run>,
    },
    Callable {
        key: RouteKey,
        factory: Box<dyn ValidationFactory<'run, 'db> + 'run>,
    },
    FinalSource {
        key: RouteKey,
        factory: Box<dyn ValidationFactory<'run, 'db> + 'run>,
    },
    NativeSource {
        key: RouteKey,
        factory: Box<dyn ValidationFactory<'run, 'db> + 'run>,
    },
}

trait ValidationFactory<'run, 'db: 'run> {
    fn dispatch(
        &self,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>>;
}

struct BoundFactory<'run, 'db: 'run, C: Configuration, P> {
    route: Route<'db, C>,
    binding: ProviderBinding<'run, P>,
}

struct ExecutableFactory<'run, 'db: 'run, C: Configuration, P> {
    route: Route<'db, C>,
    binding: ProviderBinding<'run, P>,
}

trait CallableFactory<'run, 'db: 'run, C: Configuration> {
    fn fetch(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
    ) -> RunResult<Demand<&'db C::Output<'db>>>;

    fn validate(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>>;
}

struct OwnedCallableFactory<'db, C: Configuration, P> {
    route: Route<'db, C>,
    provider: P,
}

struct CallableValidationFactory<'run, 'db: 'run, C: Configuration> {
    factory: Rc<dyn CallableFactory<'run, 'db, C> + 'run>,
}

impl<'run, 'db: 'run, C: Configuration> ValidationFactory<'run, 'db>
    for CallableValidationFactory<'run, 'db, C>
{
    fn dispatch(
        &self,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        self.factory.clone().validate(endpoint, id, revision)
    }
}

impl<'run, 'db: 'run, C, P> CallableFactory<'run, 'db, C> for OwnedCallableFactory<'db, C, P>
where
    C: Configuration,
    P: CallableRouteProvider<'run, 'db, C>,
{
    fn fetch(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
    ) -> RunResult<Demand<&'db C::Output<'db>>> {
        let ingredient = self.route.ingredient;
        let db = self.route.db;
        let callbacks = CallableCallbacks {
            endpoint: endpoint.clone(),
            factory: self,
        };
        let child_endpoint = endpoint.clone();
        endpoint.demand(move || fetch_run::fetch(child_endpoint, ingredient, db, id, callbacks))
    }

    fn validate(
        self: Rc<Self>,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        let ingredient = self.route.ingredient;
        let db = self.route.db;
        let callbacks = CallableCallbacks {
            endpoint: endpoint.clone(),
            factory: self,
        };
        let child_endpoint = endpoint.clone();
        endpoint.demand(move || {
            validation_run::validate(child_endpoint, ingredient, db, id, revision, callbacks)
        })
    }
}

struct CallableCallbacks<'run, 'db: 'run, C: Configuration, P> {
    endpoint: TaskEndpoint<'run, 'db>,
    factory: Rc<OwnedCallableFactory<'db, C, P>>,
}

impl<'run, 'db: 'run, C, P> ExecutionProvider<'run, 'db, C> for CallableCallbacks<'run, 'db, C, P>
where
    C: Configuration,
    P: CallableRouteProvider<'run, 'db, C>,
{
    fn operation_policy(&self) -> attempt_probe::OperationPolicy {
        self.factory.route.policy
    }

    fn native_value<'call>(
        &'call self,
        db: &'db C::DbView,
        operation: NativeValueOperation<'call, 'db, C>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call
    where
        'run: 'call,
    {
        self.factory
            .provider
            .native_value(self.endpoint.clone(), db, operation)
    }

    fn body<'call>(
        &'call self,
        db: &'db C::DbView,
        input: C::Input<'db>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.factory.provider.body(self.endpoint.clone(), db, input)
    }

    fn initial<'call>(
        &'call self,
        db: &'db C::DbView,
        id: Id,
        input: C::Input<'db>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.factory
            .provider
            .initial(self.endpoint.clone(), db, id, input)
    }

    fn recover<'call>(
        &'call self,
        db: &'db C::DbView,
        cycle: &'call Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.factory
            .provider
            .recover(self.endpoint.clone(), db, cycle, last, value, input)
    }
}

struct FinalSourceFactory<'db, C: Configuration> {
    sources: Rc<FinalSources<'db, C>>,
}

struct NativeSourceFactory<'db, C: Configuration> {
    route: NativeSourceRoute<'db, C>,
}

#[cfg(test)]
pub(super) fn source_layout<C: Configuration>(
    _ingredient: &IngredientImpl<C>,
) -> [(usize, usize); 3] {
    [
        (
            size_of::<RouteEntry<'_, '_>>(),
            align_of::<RouteEntry<'_, '_>>(),
        ),
        (
            size_of::<NativeSourceFactory<'_, C>>(),
            align_of::<NativeSourceFactory<'_, C>>(),
        ),
        (
            size_of::<NativeSourceRoute<'_, C>>(),
            align_of::<NativeSourceRoute<'_, C>>(),
        ),
    ]
}

impl<'run, 'db: 'run, C: Configuration> ValidationFactory<'run, 'db>
    for NativeSourceFactory<'db, C>
{
    fn dispatch(
        &self,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        let route = self.route.clone();
        let child = endpoint.clone();
        endpoint.demand(move || native_source::validate(child, route, id, revision))
    }
}

impl<'run, 'db: 'run, C: Configuration> ValidationFactory<'run, 'db>
    for FinalSourceFactory<'db, C>
{
    fn dispatch(
        &self,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        let sources = self.sources.clone();
        let child_endpoint = endpoint.clone();
        endpoint.demand(move || source_run::validate(child_endpoint, sources, id, revision))
    }
}

impl<'run, 'db: 'run, C, P> ValidationFactory<'run, 'db> for ExecutableFactory<'run, 'db, C, P>
where
    C: Configuration,
    P: ExecutableRouteProvider<'run, 'db, C>,
{
    fn dispatch(
        &self,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        let callbacks = RegisteredCallbacks {
            context: endpoint.provider(self.binding.clone())?,
        };
        let ingredient = self.route.ingredient;
        let db = self.route.db;
        let child_endpoint = endpoint.clone();
        endpoint.demand(move || {
            validation_run::validate(child_endpoint, ingredient, db, id, revision, callbacks)
        })
    }
}

struct RegisteredCallbacks<'run, 'db: 'run, P> {
    context: ProviderContext<'run, 'db, P>,
}

impl<'run, 'db: 'run, C, P> ExecutionProvider<'run, 'db, C> for RegisteredCallbacks<'run, 'db, P>
where
    C: Configuration,
    P: ExecutableRouteProvider<'run, 'db, C>,
{
    fn native_value<'call>(
        &'call self,
        db: &'db C::DbView,
        operation: NativeValueOperation<'call, 'db, C>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call
    where
        'run: 'call,
    {
        self.context
            .binding
            .provider
            .native_value(self.context.clone(), db, operation)
    }

    fn body<'call>(
        &'call self,
        db: &'db C::DbView,
        input: C::Input<'db>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.context
            .binding
            .provider
            .body(self.context.clone(), db, input)
    }
    fn initial<'call>(
        &'call self,
        db: &'db C::DbView,
        id: Id,
        input: C::Input<'db>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.context
            .binding
            .provider
            .initial(self.context.clone(), db, id, input)
    }
    fn recover<'call>(
        &'call self,
        db: &'db C::DbView,
        cycle: &'call Cycle<'call>,
        last: &'call C::Output<'db>,
        value: C::Output<'db>,
        input: C::Input<'db>,
        _endpoint: Endpoint<'run, 'db>,
    ) -> impl Future<Output = RunResult<C::Output<'db>>> + 'call
    where
        'run: 'call,
    {
        self.context
            .binding
            .provider
            .recover(self.context.clone(), db, cycle, last, value, input)
    }
}

impl<'run, 'db: 'run, C, P> ValidationFactory<'run, 'db> for BoundFactory<'run, 'db, C, P>
where
    C: Configuration,
    P: RouteProvider<'run, 'db, C>,
{
    fn dispatch(
        &self,
        endpoint: TaskEndpoint<'run, 'db>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        let context = endpoint.provider(self.binding.clone())?;
        let provider = self.binding.provider;
        let db = self.route.db;
        endpoint.demand(move || provider.verify(context, db, id, revision))
    }
}

/// Reserve routes, construct providers borrowing those tokens, then bind and seal them.
pub struct RegistryBuilder<'run, 'db: 'run> {
    db: &'db dyn Database,
    context: Rc<RunContext<'db>>,
    admission: Admission<'run>,
    identity: Rc<()>,
    entries: Vec<RouteEntry<'run, 'db>>,
    by_ingredient: FxHashMap<IngredientIndex, usize>,
    providers: usize,
    structural_dependencies: bool,
}

impl<'run, 'db: 'run> RegistryBuilder<'run, 'db> {
    pub fn new(db: &'db dyn Database, admission: &'run dyn ExecutionAdmission) -> RunResult<Self> {
        let admission = Admission::legacy(db, admission)?;
        let context = RunContext::new(db)?;
        Self::from_context(db, admission, context)
    }

    /// Creates a registry borrowing this worker's installed execution budget.
    pub fn with_budget(
        db: &'db dyn Database,
        budget: &'run ExecutionBudget<'_>,
    ) -> RunResult<Self> {
        let admission = Admission::budget(db, budget)?;
        let context = RunContext::new(db)?;
        Self::from_context(db, admission, context)
    }

    /// The registry's borrow cannot outlive the lexical native callback entry.
    ///
    /// ```compile_fail
    /// use salsa::execution_probe::{ExecutionAdmission, NativeCallbackLimits, RegisteredRun,
    ///     RegistryBuilder, RunResult, with_native_callback};
    /// use std::num::NonZeroUsize;
    /// fn escape<'db>(db: &'db dyn salsa::Database, admission: &'db dyn ExecutionAdmission)
    ///     -> RunResult<RegisteredRun<'db, 'db>>
    /// {
    ///     with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
    ///         RegistryBuilder::for_native_callback(db, &entry, admission)?.seal()
    ///     })
    /// }
    /// ```
    ///
    /// ```compile_fail
    /// use salsa::execution_probe::{NativeCallbackEntry, NativeCallbackLimits, RunResult,
    ///     with_native_callback};
    /// use std::num::NonZeroUsize;
    /// fn escape(db: &dyn salsa::Database) -> RunResult<NativeCallbackEntry<'static>> {
    ///     with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), Ok)
    /// }
    /// ```
    ///
    /// ```compile_fail
    /// use salsa::execution_probe::{ExecutionAdmission, NativeCallbackLimits, RegistryBuilder,
    ///     RunResult, TaskEndpoint, with_native_callback};
    /// use std::num::NonZeroUsize;
    /// fn escape<'db>(db: &'db dyn salsa::Database, admission: &'db dyn ExecutionAdmission)
    ///     -> RunResult<TaskEndpoint<'db, 'db>>
    /// {
    ///     with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
    ///         RegistryBuilder::for_native_callback(db, &entry, admission)?.seal()?
    ///             .run(|endpoint| async move { Ok(endpoint) })
    ///     })
    /// }
    /// ```
    pub fn for_native_callback(
        db: &'db dyn Database,
        entry: &'run NativeCallbackEntry<'_>,
        admission: &'run dyn ExecutionAdmission,
    ) -> RunResult<Self> {
        let admission = Admission::legacy(db, admission)?;
        let context = RunContext::for_native_callback(db, entry.receipt)?;
        Self::from_context(db, admission, context)
    }

    /// Reuses the sealed root through an existing lexical native callback entry.
    ///
    /// ```compile_fail
    /// use salsa::execution_probe::{NativeCallbackLimits, RegisteredRun, RegistryBuilder,
    ///     RunResult, with_native_callback};
    /// use std::num::NonZeroUsize;
    /// fn escape<'db>(db: &'db dyn salsa::Database) -> RunResult<RegisteredRun<'db, 'db>> {
    ///     with_native_callback(db, NativeCallbackLimits::new(NonZeroUsize::MIN), |entry| {
    ///         RegistryBuilder::for_native_callback_with_budget(db, &entry)?.seal()
    ///     })
    /// }
    /// ```
    pub fn for_native_callback_with_budget(
        db: &'db dyn Database,
        entry: &'run NativeCallbackEntry<'_>,
    ) -> RunResult<Self> {
        entry.receipt.check(db)?;
        let admission = Admission::native_budget(db, &entry.receipt.support)?;
        let context = RunContext::for_native_callback(db, entry.receipt)?;
        Self::from_context(db, admission, context)
    }

    fn from_context(
        db: &'db dyn Database,
        admission: Admission<'run>,
        context: RunContext<'db>,
    ) -> RunResult<Self> {
        context.check_baseline()?;
        let admitted = admission.admit(
            &context,
            ExecutionWork::Resource {
                requested_bytes: size_of::<RunContext<'db>>()
                    .saturating_add(size_of::<()>())
                    .saturating_add(4 * size_of::<usize>()),
            },
        );
        if admitted.is_ok() {
            context.check_baseline()?;
        }
        context.observe(admitted)?;
        context.check_baseline()?;
        Ok(Self {
            db,
            context: Rc::new(context),
            admission,
            identity: Rc::new(()),
            entries: Vec::new(),
            by_ingredient: FxHashMap::default(),
            providers: 0,
            structural_dependencies: false,
        })
    }

    fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        self.context.check_start()?;
        let admitted = self.admission.admit(&self.context, work);
        if admitted.is_ok() {
            self.context.check_baseline()?;
        }
        self.context.observe(admitted)?;
        self.context.check_baseline()
    }

    /// Admits one shared allocation of fixed registration metadata before constructing it.
    ///
    /// `lifecycle_work` must be positive and bound construction and complete destruction of the
    /// concrete metadata, including nested handles and reference-count housekeeping. Payload size
    /// does not bound an arbitrary destructor. Construction and destruction must not allocate
    /// additional storage or execute semantic queries. The factory's borrowed or previously admitted
    /// captures retain their existing cleanup obligations if admission refuses before construction.
    ///
    /// The returned owner is retained by its callers, including registered providers. Its cleanup
    /// is prepaid; later refusal does not require another allowance or refund accepted work.
    /// Later handle clones and their retirement require their own admission.
    pub fn allocate_shared_metadata<T>(
        &self,
        lifecycle_work: usize,
        make: impl FnOnce() -> T,
    ) -> RunResult<Rc<T>> {
        self.context.check_start()?;
        if lifecycle_work == 0 {
            return Err(RunError::Contract(
                "shared metadata requires a positive lifecycle work bound",
            ));
        }
        // This mirrors std's repr(C, align(2)) RcInner header. Keep the quotation in sync with
        // the standard library: both padding before T and trailing allocation padding count.
        let header =
            Layout::from_size_align(size_of::<[usize; 2]>(), align_of::<usize>().max(2))
                .map_err(|_| RunError::Contract("shared metadata allocation layout overflow"))?;
        let (layout, _) = header
            .extend(Layout::new::<T>())
            .map_err(|_| RunError::Contract("shared metadata allocation layout overflow"))?;
        let requested_bytes = layout.pad_to_align().size();
        self.context
            .observe(attempt_probe::charge(self.db, lifecycle_work).map_err(RunError::Refused))?;
        self.admit(ExecutionWork::Work {
            units: lifecycle_work,
        })?;
        self.admit(ExecutionWork::Resource { requested_bytes })?;
        let metadata = make();
        self.context.check_start()?;
        let shared = Rc::new(metadata);
        self.context.check_baseline()?;
        Ok(shared)
    }

    /// Enables validation-only proofs and retained preparation for complete-only dependencies.
    /// Explicit query routes take precedence over this service.
    pub fn enable_structural_dependency_validation(&mut self) -> RunResult<()> {
        self.context.check_start()?;
        if self.structural_dependencies {
            return Err(RunError::Contract(
                "structural dependency validation already enabled",
            ));
        }
        self.structural_dependencies = true;
        Ok(())
    }

    pub fn register_native_source<C: Configuration>(
        &mut self,
        db: &'db C::DbView,
        ingredient: &'db IngredientImpl<C>,
    ) -> RunResult<NativeSourceRoute<'db, C>> {
        // Reserve through the same policy, database, ingredient and duplicate checks.
        let route = self.reserve_route(db, ingredient, false)?;
        self.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<NativeSourceFactory<'db, C>>(),
        })?;
        let source = NativeSourceRoute {
            identity: route.identity,
            slot: route.slot,
            db,
            ingredient,
        };
        self.entries[source.slot] = RouteEntry::NativeSource {
            key: RouteKey {
                ingredient: ingredient.index,
                configuration: TypeId::of::<C>(),
            },
            factory: Box::new(NativeSourceFactory {
                route: source.clone(),
            }),
        };
        Ok(source)
    }

    /// Certifies the fixed-field argument ingredient without constructing any query keys.
    pub fn fixed_query_keys<C>(
        &mut self,
        route: &Route<'db, C>,
    ) -> RunResult<FixedQueryKeys<'db, C>>
    where
        C: InternedQueryConfiguration,
        for<'key> <C as crate::interned::Configuration>::Fields<'key>: FixedQueryFields,
        for<'value> <C as Configuration>::Output<'value>: Copy,
    {
        self.query_keys_for_route(route)
    }

    /// Certifies the generated argument ingredient for a callable route.
    pub fn fixed_callable_query_keys<C>(
        &mut self,
        route: &CallableRoute<'run, 'db, C>,
    ) -> RunResult<FixedQueryKeys<'db, C>>
    where
        C: InternedQueryConfiguration,
        for<'key> <C as crate::interned::Configuration>::Fields<'key>: FixedQueryFields,
        for<'value> <C as Configuration>::Output<'value>: Copy,
    {
        self.query_keys_for_route(&route.route)
    }

    /// Certifies finite argument fields and passive memo retirement for a query route.
    pub fn query_keys<C, P>(&mut self, route: &Route<'db, C>) -> RunResult<QueryKeys<'db, C, P>>
    where
        C: InternedQueryConfiguration,
        P: QueryKeyProfile<C>,
    {
        self.query_keys_for_route(route)
    }

    /// Certifies finite argument fields and passive memo retirement for a callable route.
    pub fn callable_query_keys<C, P>(
        &mut self,
        route: &CallableRoute<'run, 'db, C>,
    ) -> RunResult<QueryKeys<'db, C, P>>
    where
        C: InternedQueryConfiguration,
        P: QueryKeyProfile<C>,
    {
        self.query_keys_for_route(&route.route)
    }

    fn query_keys_for_route<C, P>(
        &mut self,
        route: &Route<'db, C>,
    ) -> RunResult<QueryKeys<'db, C, P>>
    where
        C: InternedQueryConfiguration,
        P: QueryKeyProfile<C>,
    {
        self.context.check_start()?;
        if !Rc::ptr_eq(&self.identity, &route.identity)
            || self.by_ingredient.get(&route.ingredient.index) != Some(&route.slot)
        {
            return Err(RunError::Contract("fixed query key route is foreign"));
        }
        let key = match self.entries.get(route.slot) {
            Some(
                RouteEntry::Reserved(key)
                | RouteEntry::CallableReserved(key)
                | RouteEntry::Executable { key, .. }
                | RouteEntry::Callable { key, .. },
            ) => key,
            _ => return Err(RunError::Contract("fixed query key route is foreign")),
        };
        if key.configuration != TypeId::of::<C>() || key.ingredient != route.ingredient.index {
            return Err(RunError::Contract("fixed query key route is foreign"));
        }
        let argument = C::argument_ingredient(route.db.zalsa());
        let memo = PassiveMemo::<C, C, P>::new(route.db.zalsa(), argument, route.ingredient)
            .map_err(|error| RunError::Contract(error.message()))?;
        memo.validate_singleton(route.db.zalsa(), argument)
            .map_err(|error| RunError::Contract(error.message()))?;
        Ok(QueryKeys {
            route: route.clone(),
            argument,
            memos: (memo,),
        })
    }

    /// Certifies actual interned fields and their memo storage without constructing a value.
    pub fn finite_interned_values<I, M>(
        &mut self,
        db: &'db M::DbView,
        ingredient: &'db crate::interned::IngredientImpl<I>,
        memo: &'db IngredientImpl<M>,
    ) -> RunResult<FiniteInternedValues<'db, I, M>>
    where
        I: FiniteInternedConfiguration,
        M: for<'a> Configuration<SalsaStruct<'a> = I::Struct<'a>>,
        for<'a> M::Output<'a>: Copy,
    {
        self.context.check_start()?;
        if !std::ptr::eq(db.zalsa(), self.db.zalsa())
            || !std::ptr::eq(db.zalsa_local(), self.db.zalsa_local())
            || !db
                .zalsa()
                .ingredients()
                .nth(ingredient.ingredient_index().as_u32() as usize)
                .is_some_and(|actual| std::ptr::addr_eq(actual, ingredient as &dyn Ingredient))
            || !db
                .zalsa()
                .ingredients()
                .nth(memo.index.as_u32() as usize)
                .is_some_and(|actual| std::ptr::addr_eq(actual, memo as &dyn Ingredient))
        {
            return Err(RunError::Contract(
                "finite interned value ingredient is foreign",
            ));
        }
        let memo = PassiveMemo::<I, M, CopyMemoProfile>::new(db.zalsa(), ingredient, memo)
            .map_err(|error| RunError::Contract(error.value_message()))?;
        let memos = (memo,);
        memos.validate(db.zalsa(), ingredient)?;
        Ok(InternedValues {
            identity: self.identity.clone(),
            db: self.db,
            ingredient,
            memos,
        })
    }

    /// Describes one actual memo with an explicit passive output profile.
    pub fn passive_memo<I, C, P>(
        &mut self,
        owner: &'db crate::interned::IngredientImpl<I>,
        memo: &'db IngredientImpl<C>,
    ) -> RunResult<PassiveMemo<'db, I, C, P>>
    where
        I: crate::interned::Configuration,
        C: Configuration,
        P: PassiveMemoProfile<C>,
    {
        self.context.check_start()?;
        PassiveMemo::new(self.db.zalsa(), owner, memo)
            .map_err(|error| RunError::Contract(error.value_message()))
    }

    /// Checks that the supplied typed schema covers every memo slot on this interner.
    pub fn finite_interned_values_with_memos<I, S>(
        &mut self,
        ingredient: &'db crate::interned::IngredientImpl<I>,
        memos: S,
    ) -> RunResult<InternedValues<'db, I, S>>
    where
        I: FiniteInternedConfiguration,
        S: PassiveMemoSchema<'db, I>,
    {
        self.context.check_start()?;
        memos.validate(self.db.zalsa(), ingredient)?;
        Ok(InternedValues {
            identity: self.identity.clone(),
            db: self.db,
            ingredient,
            memos,
        })
    }

    /// Registers exact finalized inputs; it never supplies a source query body.
    /// The certificates must be nonempty and ordered by their unique query key.
    pub fn register_final_source<C: Configuration>(
        &mut self,
        db: &'db C::DbView,
        ingredient: &'db IngredientImpl<C>,
        prepared: &[FinalSourceMemo<'db, C>],
    ) -> RunResult<FinalSourceRoute<'db, C>> {
        self.context.check_start()?;
        if !std::ptr::eq(db.zalsa(), self.db.zalsa())
            || !std::ptr::eq(db.zalsa_local(), self.db.zalsa_local())
        {
            return Err(RunError::Contract(
                FinalSourceError::ForeignIngredient.message(),
            ));
        }
        if !matches!(
            C::ATTEMPT_POLICY,
            crate::attempt_probe::QueryPolicy::CompleteOnly
                | crate::attempt_probe::QueryPolicy::ReturnOnly
        ) {
            return Err(RunError::Contract(
                FinalSourceError::UnsupportedPolicy.message(),
            ));
        }
        if prepared.is_empty() {
            return Err(RunError::Contract("final source registration has no keys"));
        }
        if self.by_ingredient.contains_key(&ingredient.index) {
            return Err(RunError::Contract("query route already reserved"));
        }
        let mut previous = None;
        for certificate in prepared {
            self.admit(ExecutionWork::Work {
                units: source_run::SourceWork::RegistrationKey.units(prepared.len())?,
            })?;
            if !certificate.belongs_to(db, ingredient) {
                return Err(RunError::Contract(
                    FinalSourceError::ForeignIngredient.message(),
                ));
            }
            if previous.is_some_and(|previous| previous >= certificate.id()) {
                return Err(RunError::Contract(
                    "final source keys are not sorted and unique",
                ));
            }
            certificate
                .check_current()
                .map_err(|error| RunError::Contract(error.message()))?;
            previous = Some(certificate.id());
        }
        let key_bytes = prepared
            .len()
            .checked_mul(size_of::<FinalSourceMemo<'db, C>>())
            .ok_or(RunError::Contract(
                "final source registration size overflow",
            ))?;
        let sources_bytes = size_of::<FinalSources<'db, C>>()
            .checked_add(2 * size_of::<usize>())
            .ok_or(RunError::Contract(
                "final source registration size overflow",
            ))?;
        for requested_bytes in [
            key_bytes,
            sources_bytes,
            size_of::<FinalSourceFactory<'db, C>>(),
            size_of::<RouteEntry<'run, 'db>>()
                .saturating_mul(4)
                .saturating_add(size_of::<(IngredientIndex, usize)>().saturating_mul(8)),
        ] {
            self.admit(ExecutionWork::Resource { requested_bytes })?;
        }
        // Registration admissions can replace an earlier key. Recheck every retained selection
        // after the last callback and before publishing this immutable source entry.
        for certificate in prepared {
            certificate
                .check_current()
                .map_err(|error| RunError::Contract(error.message()))?;
        }
        let mut certificates = Vec::with_capacity(prepared.len());
        certificates.extend(prepared.iter().map(FinalSourceMemo::copy_handle));
        let sources = Rc::new(FinalSources {
            db,
            ingredient,
            prepared: certificates,
        });
        let slot = self.entries.len();
        self.entries.push(RouteEntry::FinalSource {
            key: RouteKey {
                ingredient: ingredient.index,
                configuration: TypeId::of::<C>(),
            },
            factory: Box::new(FinalSourceFactory {
                sources: sources.clone(),
            }),
        });
        self.by_ingredient.insert(ingredient.index, slot);
        Ok(FinalSourceRoute {
            identity: self.identity.clone(),
            slot,
            sources,
        })
    }

    pub fn reserve<C: Configuration>(
        &mut self,
        db: &'db C::DbView,
        ingredient: &'db IngredientImpl<C>,
    ) -> RunResult<Route<'db, C>> {
        self.reserve_route(db, ingredient, false)
    }

    /// Reserves a typed callable before any of its mutually dependent providers are bound.
    /// Its own controlled execution may refuse and cannot create tracked outputs or accumulate
    /// values, including when declared CompleteOnly. Ordinary CompleteOnly dependencies retain
    /// their completion and output behavior. The declaration still restricts query dependencies;
    /// cycle strategy is unchanged.
    pub fn reserve_callable<C: Configuration>(
        &mut self,
        db: &'db C::DbView,
        ingredient: &'db IngredientImpl<C>,
    ) -> RunResult<CallableRoute<'run, 'db, C>> {
        let route = self.reserve_route(db, ingredient, true)?;
        Ok(CallableRoute {
            route,
            state: Rc::new(CallableState {
                factory: OnceCell::new(),
            }),
        })
    }

    fn reserve_route<C: Configuration>(
        &mut self,
        db: &'db C::DbView,
        ingredient: &'db IngredientImpl<C>,
        callable: bool,
    ) -> RunResult<Route<'db, C>> {
        self.context.check_start()?;
        if C::ATTEMPT_POLICY != crate::attempt_probe::QueryPolicy::ReturnOnly
            && !(callable && C::ATTEMPT_POLICY == crate::attempt_probe::QueryPolicy::CompleteOnly)
        {
            return Err(RunError::Contract(
                "only return-only routes can be registered",
            ));
        }
        if !std::ptr::eq(db.zalsa(), self.db.zalsa())
            || !std::ptr::eq(db.zalsa_local(), self.db.zalsa_local())
            || !self
                .db
                .zalsa()
                .ingredients()
                .nth(ingredient.index.as_u32() as usize)
                .is_some_and(|known| std::ptr::addr_eq(known, ingredient as &dyn Ingredient))
        {
            return Err(RunError::Contract(
                "route has a foreign database or ingredient",
            ));
        }
        if self.by_ingredient.contains_key(&ingredient.index) {
            return Err(RunError::Contract("query route already reserved"));
        }
        self.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<RouteEntry<'run, 'db>>()
                .saturating_mul(4)
                .saturating_add(size_of::<(IngredientIndex, usize)>().saturating_mul(8))
                .saturating_add(if callable {
                    size_of::<CallableState<'run, 'db, C>>().saturating_add(2 * size_of::<usize>())
                } else {
                    0
                }),
        })?;
        let slot = self.entries.len();
        let key = RouteKey {
            ingredient: ingredient.index,
            configuration: TypeId::of::<C>(),
        };
        self.entries.push(if callable {
            RouteEntry::CallableReserved(key)
        } else {
            RouteEntry::Reserved(key)
        });
        self.by_ingredient.insert(ingredient.index, slot);
        Ok(Route {
            identity: self.identity.clone(),
            slot,
            db,
            ingredient,
            policy: if callable {
                attempt_probe::OperationPolicy::admitted(C::ATTEMPT_POLICY)
            } else {
                attempt_probe::OperationPolicy::ordinary(C::ATTEMPT_POLICY)
            },
        })
    }

    pub fn provider<P>(&mut self, provider: &'run P) -> RunResult<ProviderBinding<'run, P>> {
        self.context.check_start()?;
        let ordinal = self.providers;
        self.providers = ordinal
            .checked_add(1)
            .ok_or(RunError::Contract("provider identity exhausted"))?;
        Ok(ProviderBinding {
            identity: self.identity.clone(),
            ordinal,
            provider,
        })
    }

    pub fn bind<C, P>(
        &mut self,
        route: &Route<'db, C>,
        provider: &ProviderBinding<'run, P>,
    ) -> RunResult<()>
    where
        C: Configuration,
        P: RouteProvider<'run, 'db, C>,
    {
        self.check_unbound(route, provider)?;
        self.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<BoundFactory<'run, 'db, C, P>>(),
        })?;
        self.entries[route.slot] = RouteEntry::Bound {
            key: RouteKey {
                ingredient: route.ingredient.index,
                configuration: TypeId::of::<C>(),
            },
            provider: provider.ordinal,
            factory: Box::new(BoundFactory {
                route: route.clone(),
                binding: provider.clone(),
            }),
        };
        Ok(())
    }

    pub fn bind_executable<C, P>(
        &mut self,
        route: &Route<'db, C>,
        provider: &ProviderBinding<'run, P>,
    ) -> RunResult<()>
    where
        C: Configuration,
        P: ExecutableRouteProvider<'run, 'db, C>,
    {
        self.check_unbound(route, provider)?;
        self.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<ExecutableFactory<'run, 'db, C, P>>(),
        })?;
        self.entries[route.slot] = RouteEntry::Executable {
            key: RouteKey {
                ingredient: route.ingredient.index,
                configuration: TypeId::of::<C>(),
            },
            provider: provider.ordinal,
            factory: Box::new(ExecutableFactory {
                route: route.clone(),
                binding: provider.clone(),
            }),
        };
        Ok(())
    }

    /// Moves a provider into the registration; callable handles retain only weak references to it.
    pub fn bind_callable<C, P>(
        &mut self,
        route: &CallableRoute<'run, 'db, C>,
        provider: P,
    ) -> RunResult<()>
    where
        C: Configuration,
        P: CallableRouteProvider<'run, 'db, C>,
    {
        self.context.check_start()?;
        if !Rc::ptr_eq(&self.identity, &route.route.identity) {
            return Err(RunError::Contract("foreign registration token"));
        }
        let Some(RouteEntry::CallableReserved(key)) = self.entries.get(route.route.slot) else {
            return Err(RunError::Contract("query route already bound or missing"));
        };
        if key.configuration != TypeId::of::<C>() || key.ingredient != route.route.ingredient.index
        {
            return Err(RunError::Contract("query route type mismatch"));
        }
        if route.state.factory.get().is_some() {
            return Err(RunError::Contract("callable query route already bound"));
        }
        self.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<OwnedCallableFactory<'db, C, P>>()
                .saturating_add(2 * size_of::<usize>())
                .saturating_add(size_of::<CallableValidationFactory<'run, 'db, C>>()),
        })?;
        let factory: Rc<dyn CallableFactory<'run, 'db, C> + 'run> = Rc::new(OwnedCallableFactory {
            route: route.route.clone(),
            provider,
        });
        let weak = Rc::downgrade(&factory);
        let factory = Box::new(CallableValidationFactory { factory });
        // All callbacks precede publication. The sealed registry owns the strong reference;
        // provider-to-provider routes retain only weak references, including recursive routes.
        if route.state.factory.set(weak).is_err() {
            return Err(RunError::Contract("callable query route already bound"));
        }
        self.entries[route.route.slot] = RouteEntry::Callable {
            key: RouteKey {
                ingredient: route.route.ingredient.index,
                configuration: TypeId::of::<C>(),
            },
            factory,
        };
        Ok(())
    }

    fn check_unbound<C: Configuration, P>(
        &self,
        route: &Route<'db, C>,
        provider: &ProviderBinding<'run, P>,
    ) -> RunResult<()> {
        self.context.check_start()?;
        if !Rc::ptr_eq(&self.identity, &route.identity)
            || !Rc::ptr_eq(&self.identity, &provider.identity)
        {
            return Err(RunError::Contract("foreign registration token"));
        }
        let Some(RouteEntry::Reserved(key)) = self.entries.get(route.slot) else {
            return Err(RunError::Contract("query route already bound or missing"));
        };
        if key.configuration != TypeId::of::<C>() || key.ingredient != route.ingredient.index {
            return Err(RunError::Contract("query route type mismatch"));
        }
        Ok(())
    }

    pub fn seal(self) -> RunResult<RegisteredRun<'run, 'db>> {
        self.context.check_start()?;
        if self.entries.iter().any(|entry| {
            matches!(
                entry,
                RouteEntry::Reserved(_) | RouteEntry::CallableReserved(_)
            )
        }) {
            return Err(RunError::Contract("query route remains unbound"));
        }
        self.admit(ExecutionWork::Resource {
            requested_bytes: size_of::<Registry<'run, 'db>>()
                .saturating_add(2 * size_of::<usize>()),
        })?;
        Ok(RegisteredRun {
            registry: Rc::new(Registry {
                db: self.db,
                context: self.context,
                admission: self.admission,
                identity: self.identity,
                entries: self.entries,
                by_ingredient: self.by_ingredient,
                providers: self.providers,
                structural_dependencies: self.structural_dependencies,
            }),
        })
    }
}

struct Registry<'run, 'db: 'run> {
    db: &'db dyn Database,
    context: Rc<RunContext<'db>>,
    admission: Admission<'run>,
    identity: Rc<()>,
    entries: Vec<RouteEntry<'run, 'db>>,
    by_ingredient: FxHashMap<IngredientIndex, usize>,
    providers: usize,
    structural_dependencies: bool,
}

pub struct RegisteredRun<'run, 'db: 'run> {
    registry: Rc<Registry<'run, 'db>>,
}

impl<'run, 'db: 'run> RegisteredRun<'run, 'db> {
    #[cfg(test)]
    pub(super) fn registration_capacities(&self) -> (usize, usize) {
        (
            self.registry.entries.capacity(),
            self.registry.by_ingredient.capacity(),
        )
    }

    pub fn run<T: 'run, F: Future<Output = RunResult<T>> + 'run>(
        self,
        make: impl FnOnce(TaskEndpoint<'run, 'db>) -> F + 'run,
    ) -> RunResult<T> {
        let registry = self.registry;
        registry.context.check_start()?;
        let db = registry.db;
        crate::attach(db, || {
            let root_registry = registry.clone();
            let result = Driver::run_with_context(
                registry.context.clone(),
                registry.admission,
                move |inner| {
                    make(TaskEndpoint {
                        inner,
                        registry: root_registry,
                    })
                },
            );
            // A plain child need not capture a TaskEndpoint. Keep registered providers alive
            // until the driver has retired every queued and active task, including on unwind.
            drop(registry);
            result
        })
    }
}

/// Typed replies contain results, never independently owned query guards.
pub struct Demand<T>(Reply<T>);

impl<T> Future for Demand<T> {
    type Output = RunResult<T>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.get_mut().0).poll(cx)
    }
}

/// This facade uses the existing runtime queue and driver; it does not poll tasks itself.
pub struct TaskEndpoint<'run, 'db: 'run> {
    pub(super) inner: Endpoint<'run, 'db>,
    registry: Rc<Registry<'run, 'db>>,
}

impl Clone for TaskEndpoint<'_, '_> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            registry: self.registry.clone(),
        }
    }
}

impl<'run, 'db: 'run> TaskEndpoint<'run, 'db> {
    /// Identifies this endpoint's storage for constructing deferred field requests.
    pub fn field_request_context(&self) -> FieldRequestContext<'db> {
        FieldRequestContext::from(self.inner.context.db)
    }

    /// Reads an existing complete-only memo through the caller's canonical dependency transaction.
    pub fn read_prepared_source<'call, R>(
        &'call self,
        request: R,
    ) -> impl Future<Output = &'db R::Output> + 'call
    where
        'run: 'call,
        R: PreparedSourceRead<'db> + 'call,
    {
        super::prepared_source_run::read(self, request)
    }

    /// Admits a generated field read and its canonical getter conversion.
    pub fn read_field<'call, R, P>(
        &'call self,
        request: R,
        profile: &'call P,
    ) -> impl Future<Output = R::Output> + 'call
    where
        'run: 'call,
        R: FieldRequest<'db> + 'call,
        P: FieldReadProfile<R::Stored>,
    {
        super::field_run::read(self, request, profile)
    }

    /// Interns reviewed owned fields under the shared semantic-progress allowance.
    pub fn intern_value<'call, I, S>(
        &'call self,
        values: &'call InternedValues<'db, I, S>,
        fields: I::Fields<'db>,
    ) -> impl Future<Output = I::Struct<'db>> + 'call
    where
        'run: 'call,
        I: FiniteInternedConfiguration,
        S: PassiveMemoSchema<'db, I> + 'call,
    {
        key_run::intern_value(self, values, fields)
    }

    pub(super) fn check_finite_interned_values<I, S>(
        &self,
        values: &InternedValues<'db, I, S>,
    ) -> RunResult<()>
    where
        I: FiniteInternedConfiguration,
    {
        if !Rc::ptr_eq(&self.registry.identity, &values.identity)
            || !std::ptr::eq(values.db.zalsa(), self.inner.context.db.zalsa())
            || !std::ptr::eq(values.db.zalsa_local(), self.inner.context.db.zalsa_local())
        {
            return Err(RunError::Contract(
                "finite interned value capability is foreign",
            ));
        }
        Ok(())
    }

    /// Creates or finds the canonical argument key under the shared attempt allowance.
    /// The producer must admit the input's construction and cleanup before submitting it.
    pub fn intern_query_key<'call, C, P>(
        &'call self,
        keys: &'call QueryKeys<'db, C, P>,
        input: <C as Configuration>::Input<'db>,
    ) -> impl Future<Output = Id> + 'call
    where
        'run: 'call,
        C: InternedQueryConfiguration,
        P: QueryKeyProfile<C> + 'call,
    {
        key_run::intern(self, keys, input)
    }

    pub(super) fn check_query_keys<C: InternedQueryConfiguration, P>(
        &self,
        keys: &QueryKeys<'db, C, P>,
    ) -> RunResult<()> {
        let route = &keys.route;
        if !Rc::ptr_eq(&self.registry.identity, &route.identity) {
            return Err(RunError::Contract("fixed query key route is foreign"));
        }
        let Some(RouteEntry::Executable { key, .. } | RouteEntry::Callable { key, .. }) =
            self.registry.entries.get(route.slot)
        else {
            return Err(RunError::Contract("fixed query key route is foreign"));
        };
        if key.configuration != TypeId::of::<C>() || key.ingredient != route.ingredient.index {
            return Err(RunError::Contract("fixed query key route is foreign"));
        }
        Ok(())
    }

    /// Reads one registered final source memo while recording the caller's real dependency.
    /// Missing or invalidated certificates stop the attempt without executing the source query.
    pub fn read_final_source<'call, C: Configuration>(
        &'call self,
        route: &'call FinalSourceRoute<'db, C>,
        id: Id,
    ) -> impl Future<Output = &'db C::Output<'db>> + 'call
    where
        'run: 'call,
    {
        source_run::read(self, route, id)
    }

    /// Requests ordinary native execution of a registered source.
    /// An explicit-read boundary makes the enclosing run return [`RunError::RequiresFetch`]
    /// before entering the source query.
    pub fn read_native_source<'call, C: Configuration>(
        &'call self,
        route: &'call NativeSourceRoute<'db, C>,
        id: Id,
    ) -> impl Future<Output = &'db C::Output<'db>> + 'call
    where
        'run: 'call,
    {
        native_source::read(self, route, id)
    }

    pub(super) fn check_native_source<C: Configuration>(
        &self,
        route: &NativeSourceRoute<'db, C>,
    ) -> RunResult<()> {
        attempt_probe::check_query_dependency(C::ATTEMPT_POLICY).map_err(RunError::Contract)?;
        if !Rc::ptr_eq(&self.registry.identity, &route.identity) {
            return Err(RunError::Contract("foreign native source route"));
        }
        let Some(RouteEntry::NativeSource { key, .. }) = self.registry.entries.get(route.slot)
        else {
            return Err(RunError::Contract("native source route is not registered"));
        };
        if key.configuration != TypeId::of::<C>() || key.ingredient != route.ingredient.index {
            return Err(RunError::Contract("native source route type mismatch"));
        }
        if explicit_reads::strict_is_active() {
            return Err(RunError::RequiresFetch);
        }
        Ok(())
    }

    pub(super) fn check_final_source<C: Configuration>(
        &self,
        route: &FinalSourceRoute<'db, C>,
    ) -> RunResult<()> {
        attempt_probe::check_query_dependency(C::ATTEMPT_POLICY).map_err(RunError::Contract)?;
        if !Rc::ptr_eq(&self.registry.identity, &route.identity) {
            return Err(RunError::Contract("foreign final source route"));
        }
        let Some(RouteEntry::FinalSource { key, .. }) = self.registry.entries.get(route.slot)
        else {
            return Err(RunError::Contract("final source route is not registered"));
        };
        if key.configuration != TypeId::of::<C>()
            || key.ingredient != route.sources.ingredient.index
        {
            return Err(RunError::Contract("final source route type mismatch"));
        }
        Ok(())
    }

    /// Runs one synchronous admitted action while retaining its borrowed caller until acceptance.
    ///
    /// The action first runs when this future is polled. Success returns its value immediately;
    /// refusal or native panic suspends until the driver drains queued children, then reports the
    /// original failure. No task, checkpoint or work admission is added by this boundary.
    /// Keep semantic owners outside the action: values destroyed inside it before return cannot
    /// be retained. The action's work must still use the applicable admission controls.
    pub fn local_call<'call, T, M>(&'call self, make: M) -> impl Future<Output = T> + 'call
    where
        'run: 'call,
        M: FnOnce() -> RunResult<T> + 'call,
        T: 'call,
    {
        callback::local_call(&self.inner, make)
    }

    /// Prepares and certifies structural inputs while retaining the current semantic task.
    ///
    /// The action runs synchronously with an empty structural query stack. Only complete-only
    /// queries may execute, including during certification. The same semantic owners and remaining
    /// budget resume afterward; native cancellation and panics use the driver's ordered cleanup.
    /// Native callback drivers cannot enter this phase.
    pub fn prepare_structural<'call, T, M>(&'call self, make: M) -> impl Future<Output = T> + 'call
    where
        'run: 'call,
        M: FnOnce() -> RunResult<T> + 'call,
        T: 'call,
    {
        super::structural_preparation::prepare(&self.inner, make)
    }

    /// Retains the borrowed caller through request creation and its asynchronous reply.
    ///
    /// The factory first runs when this future is polled. Wrap both demand creation and its
    /// await in the supplied future: a creation error or native panic then retains the caller
    /// until the driver drains queued children. Success requires the original enclosing scope
    /// and no pending child; the request may leave a child queued while it awaits that reply.
    /// This boundary adds no task, admission or cancellation callback. Keep semantic owners
    /// outside the factory and request future; values destroyed inside them cannot be retained.
    pub fn child_call<'call, T, M, F>(&'call self, make: M) -> impl Future<Output = T> + 'call
    where
        'run: 'call,
        M: FnOnce() -> F + 'call,
        F: Future<Output = RunResult<T>> + 'call,
        T: 'call,
    {
        callback::child_call(&self.inner, make)
    }

    /// Yields local work while keeping its borrowed state inside the current task.
    ///
    /// Immediately await the returned future with `endpoint.checkpoint()?.await?`. Its first
    /// poll must occur in the same task poll that created it; awaiting other work first makes
    /// the checkpoint stale and interrupts the attempt with a contract error.
    pub fn checkpoint(&self) -> RunResult<impl Future<Output = RunResult<()>> + '_> {
        self.inner.checkpoint()
    }

    /// Checks that a local result can be returned without scheduling another operation.
    pub fn check_completion(&self) -> RunResult<()> {
        self.inner.check_completion()
    }

    pub fn demand<T: 'run, F: Future<Output = RunResult<T>> + 'run>(
        &self,
        make: impl FnOnce() -> F + 'run,
    ) -> RunResult<Demand<T>> {
        self.inner.demand(make).map(Demand)
    }

    /// Admits a quotation through the root's selected authority.
    ///
    /// Legacy roots invoke their admission observer. Budget roots debit requested bytes for
    /// `Resource` and `Task`; `Work` and `Poll` do not spend either counter. A raw `Work` report
    /// does not limit semantic progress. Use `admit_work` for work not already charged, and
    /// propagate refusal through the caller's protected boundary.
    pub fn admit(&self, work: ExecutionWork) -> RunResult<()> {
        let access = self.inner.queue.begin_access()?;
        self.inner.context.check_resume()?;
        access.check()?;
        self.inner.admit(work)?;
        access.check()
    }

    /// Debits shared semantic work, then dispatches a `Work` report through the selected authority.
    ///
    /// Legacy roots invoke their observer after the debit; budget roots do not debit again.
    /// Accepted debits are retained on later refusal. Keep semantic owners outside the protected
    /// action and propagate errors through that boundary; this method does not suspend by itself.
    /// Zero units do not limit repeated semantic operations.
    pub fn admit_work(&self, units: usize) -> RunResult<()> {
        if !self.inner.local_call_is_eligible() {
            return Err(RunError::Contract(
                "semantic work requires the current interruptible execution run",
            ));
        }
        let access = self.inner.queue.begin_access()?;
        attempt_probe::charge(self.inner.context.db, units).map_err(RunError::Refused)?;
        self.admit(ExecutionWork::Work { units })?;
        access.check()
    }

    pub fn provider<P>(
        &self,
        binding: ProviderBinding<'run, P>,
    ) -> RunResult<ProviderContext<'run, 'db, P>> {
        if !Rc::ptr_eq(&self.registry.identity, &binding.identity)
            || binding.ordinal >= self.registry.providers
        {
            return Err(RunError::Contract("foreign provider binding"));
        }
        Ok(ProviderContext {
            endpoint: self.clone(),
            binding,
        })
    }

    /// Fetches the canonical query through its registered provider, independently of the caller.
    pub fn fetch_ref<C: Configuration>(
        &self,
        route: &CallableRoute<'run, 'db, C>,
        id: Id,
    ) -> RunResult<Demand<&'db C::Output<'db>>> {
        self.callable_factory(route)?.fetch(self.clone(), id)
    }

    pub fn validate_callable<C: Configuration>(
        &self,
        route: &CallableRoute<'run, 'db, C>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        self.callable_factory(route)?
            .validate(self.clone(), id, revision)
    }

    fn callable_factory<C: Configuration>(
        &self,
        route: &CallableRoute<'run, 'db, C>,
    ) -> RunResult<Rc<dyn CallableFactory<'run, 'db, C> + 'run>> {
        attempt_probe::check_admitted_entry(C::ATTEMPT_POLICY).map_err(RunError::Contract)?;
        if !Rc::ptr_eq(&self.registry.identity, &route.route.identity) {
            return Err(RunError::Contract("foreign query route"));
        }
        let Some(RouteEntry::Callable { key, .. }) = self.registry.entries.get(route.route.slot)
        else {
            return Err(RunError::Contract("query route is not callable"));
        };
        if key.configuration != TypeId::of::<C>() || key.ingredient != route.route.ingredient.index
        {
            return Err(RunError::Contract("query route type mismatch"));
        }
        route
            .state
            .factory
            .get()
            .and_then(Weak::upgrade)
            .ok_or(RunError::Contract("callable query route is not bound"))
    }

    /// Dispatches a provider callback only; this does not validate a memo.
    pub fn verify_callback(
        &self,
        key: DatabaseKeyIndex,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        attempt_probe::check_query_dependency(attempt_probe::QueryPolicy::ReturnOnly)
            .map_err(RunError::Contract)?;
        let Some(slot) = self.registry.by_ingredient.get(&key.ingredient_index()) else {
            return Err(RunError::Contract("verification route is not registered"));
        };
        let Some(RouteEntry::Bound { factory, .. }) = self.registry.entries.get(*slot) else {
            return Err(RunError::Contract("verification route is not bound"));
        };
        factory.dispatch(self.clone(), key.key_index(), revision)
    }

    /// Validates a dependency using runtime-owned memo states or an explicitly supported leaf.
    /// Complete-only dependencies may also use the enabled structural preparation service.
    pub(in crate::function) fn validate(
        &self,
        key: DatabaseKeyIndex,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>> {
        let db = self.registry.db;
        let ingredient = db.zalsa().lookup_ingredient(key.ingredient_index());
        if let Some(function) = ingredient.as_function() {
            attempt_probe::check_query_dependency(function.attempt_policy()).map_err(RunError::Contract)?;
            let Some(slot) = self.registry.by_ingredient.get(&key.ingredient_index()) else {
                if function.attempt_policy() == crate::attempt_probe::QueryPolicy::CompleteOnly
                    && self.registry.structural_dependencies
                {
                    let endpoint = self.clone();
                    return self.demand(move || {
                        structural_dependencies::validate(endpoint, db, key, revision)
                    });
                }
                return Err(RunError::Contract("validation route is not registered"));
            };
            let Some(
                RouteEntry::Executable { factory, .. }
                | RouteEntry::Callable { factory, .. }
                | RouteEntry::FinalSource { factory, .. }
                | RouteEntry::NativeSource { factory, .. },
            ) = self.registry.entries.get(*slot)
            else {
                return Err(RunError::Contract("validation route is not executable"));
            };
            factory.dispatch(self.clone(), key.key_index(), revision)
        } else {
            let endpoint = self.clone();
            self.demand(move || async move {
                let scope = match EntryScope::capture(&endpoint.inner.context) {
                    Ok(scope) => scope,
                    Err(error) => match callback::reject(&endpoint.inner, error, ()).await {},
                };
                let result = callback::complete(
                    &endpoint.inner,
                    &scope,
                    callback::CallbackKind::Canonical,
                    || {
                        ready(
                            endpoint
                                .admit(ExecutionWork::Work { units: 1 })
                                .and_then(|()| {
                                    // The synchronous call releases any native shard lock before the
                                    // callback adapter can suspend and drain a queued child.
                                    // SAFETY: This ingredient belongs to the registered run's database.
                                    let result = unsafe {
                                        ingredient.maybe_changed_after_leaf(
                                            db.zalsa(),
                                            db.into(),
                                            key.key_index(),
                                            revision,
                                        )
                                    };
                                    result.ok_or(RunError::Contract(
                                        "ingredient has no local validation route",
                                    ))
                                }),
                        )
                    },
                )
                .await;
                Ok(result)
            })
        }
    }
}

pub struct ProviderContext<'run, 'db: 'run, P> {
    endpoint: TaskEndpoint<'run, 'db>,
    binding: ProviderBinding<'run, P>,
}

impl<P> Clone for ProviderContext<'_, '_, P> {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            binding: self.binding.clone(),
        }
    }
}

impl<'run, 'db: 'run, P: 'run> ProviderContext<'run, 'db, P> {
    pub fn endpoint(&self) -> &TaskEndpoint<'run, 'db> {
        &self.endpoint
    }

    /// Fetches the registered query and records the retained memo as a read of the caller.
    /// Any output copy or clone happens after this demand completes, in the caller's operation.
    pub fn fetch_ref<C: Configuration>(
        &self,
        route: &Route<'db, C>,
        id: Id,
    ) -> RunResult<Demand<&'db C::Output<'db>>>
    where
        P: ExecutableRouteProvider<'run, 'db, C>,
    {
        self.executable_route(route)?;
        let callbacks = RegisteredCallbacks {
            context: self.clone(),
        };
        let endpoint = self.endpoint.clone();
        let ingredient = route.ingredient;
        let db = route.db;
        self.endpoint
            .demand(move || fetch_run::fetch(endpoint, ingredient, db, id, callbacks))
    }

    pub fn validate<C: Configuration>(
        &self,
        route: &Route<'db, C>,
        id: Id,
        revision: Revision,
    ) -> RunResult<Demand<VerifyResult>>
    where
        P: ExecutableRouteProvider<'run, 'db, C>,
    {
        let factory = self.executable_route(route)?;
        factory.dispatch(self.endpoint.clone(), id, revision)
    }

    fn executable_route<C: Configuration>(
        &self,
        route: &Route<'db, C>,
    ) -> RunResult<&dyn ValidationFactory<'run, 'db>> {
        attempt_probe::check_admitted_entry(C::ATTEMPT_POLICY).map_err(RunError::Contract)?;
        if !Rc::ptr_eq(&self.endpoint.registry.identity, &route.identity) {
            return Err(RunError::Contract("foreign query route"));
        }
        let Some(RouteEntry::Executable {
            key,
            provider,
            factory,
        }) = self.endpoint.registry.entries.get(route.slot)
        else {
            return Err(RunError::Contract("query route is not executable"));
        };
        if key.configuration != TypeId::of::<C>()
            || key.ingredient != route.ingredient.index
            || *provider != self.binding.ordinal
        {
            return Err(RunError::Contract(
                "query route has a different provider binding",
            ));
        }
        Ok(factory.as_ref())
    }

    /// Dispatches the registered body callback; no query frame, claim, or memo is created.
    pub fn body_callback<C: Configuration>(
        &self,
        route: &Route<'db, C>,
        input: C::Input<'db>,
    ) -> RunResult<Demand<C::Output<'db>>>
    where
        P: RouteProvider<'run, 'db, C>,
    {
        attempt_probe::check_query_dependency(C::ATTEMPT_POLICY).map_err(RunError::Contract)?;
        if !Rc::ptr_eq(&self.endpoint.registry.identity, &route.identity) {
            return Err(RunError::Contract("foreign query route"));
        }
        let Some(RouteEntry::Bound { key, provider, .. }) =
            self.endpoint.registry.entries.get(route.slot)
        else {
            return Err(RunError::Contract("query route is not bound"));
        };
        if key.configuration != TypeId::of::<C>()
            || key.ingredient != route.ingredient.index
            || *provider != self.binding.ordinal
        {
            return Err(RunError::Contract(
                "query route has a different provider binding",
            ));
        }
        let provider = self.binding.provider;
        let context = self.clone();
        let db = route.db;
        self.endpoint
            .demand(move || provider.body(context, db, input))
    }
}
