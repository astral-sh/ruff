use std::convert::Infallible;
use std::future::{Future, pending, ready};
use std::ops::ControlFlow;
use std::task::Poll;

use super::callback::{self, CallbackKind};
use super::frame_free::EntryScope;
use super::native_values::{self, NativeValueQuote};
use super::registration::TaskEndpoint;
use super::{RunError, RunResult, read_run};
use crate::id::AsId;
use crate::zalsa::Zalsa;
use crate::zalsa_local::ZalsaLocal;
use crate::zalsa_local::dependency_read::DependencyRead;
use crate::{Database, DatabaseKeyIndex, Durability, Revision, input, interned, tracked_struct};

/// Identifies database storage for constructing deferred field requests.
///
/// This context does not admit or perform a read. `TaskEndpoint::read_field` checks its database
/// and worker-local identity before selecting a value. It does not provide ordinary getter access:
///
/// ```compile_fail
/// use salsa::execution_probe::FieldRequestContext;
///
/// fn ordinary_getter(db: &dyn salsa::Database) {}
/// fn deferred_reads(context: FieldRequestContext<'_>) {
///     ordinary_getter(&context);
/// }
/// ```
#[derive(Clone, Copy)]
pub struct FieldRequestContext<'db> {
    storage: (&'db Zalsa, &'db ZalsaLocal),
}

impl<'db, D: Database + ?Sized> From<&'db D> for FieldRequestContext<'db> {
    fn from(db: &'db D) -> Self {
        Self {
            storage: db.zalsas(),
        }
    }
}

/// The generated getter's canonical conversion of its stored field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FieldReturnMode {
    Ref,
    Copy,
    Clone,
    Deref,
    AsRef,
    AsDeref,
}

/// Quotes a getter conversion without performing it or replacing its result.
///
/// Variable-size quotation must itself admit traversal through `endpoint`. The quote includes
/// destruction of any ownership created by the conversion, using the native-value contract.
pub trait FieldReadProfile<T> {
    fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        stored: &'call T,
        mode: FieldReturnMode,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call;
}

/// Finite conversions generated for `returns(ref)` and `returns(copy)` fields.
/// Other modes can call user implementations and require their own profile.
pub struct BorrowOrCopy;

impl<T> FieldReadProfile<T> for BorrowOrCopy {
    fn quote<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        _endpoint: &'call TaskEndpoint<'run, 'db>,
        _stored: &'call T,
        mode: FieldReturnMode,
    ) -> impl Future<Output = RunResult<NativeValueQuote>> + 'call {
        ready(match mode {
            FieldReturnMode::Ref | FieldReturnMode::Copy => {
                let bytes = if mode == FieldReturnMode::Copy {
                    size_of::<T>()
                } else {
                    size_of::<&T>()
                };
                Ok(NativeValueQuote {
                    work: 1,
                    requested_bytes: bytes,
                    cleanup_work: 0,
                })
            }
            FieldReturnMode::Clone
            | FieldReturnMode::Deref
            | FieldReturnMode::AsRef
            | FieldReturnMode::AsDeref => Err(RunError::Contract(
                "field conversion requires an explicit profile",
            )),
        })
    }
}

mod sealed {
    pub trait Sealed {}
}

/// A generated field request retains its identity without reading or converting the field.
/// Use `TaskEndpoint::read_field` to admit its dependency and receive the ordinary getter result.
pub trait FieldRequest<'db>: sealed::Sealed {
    type Stored: 'db;
    type Output: 'db;
    #[doc(hidden)]
    type Dependency: FieldDependencyState;

    /// Reads and converts the field synchronously, recording its ordinary dependency first.
    /// This performs no execution admission; controlled callers use `TaskEndpoint::read_field`.
    fn read_ordinary(self) -> Self::Output
    where
        Self: Sized,
    {
        super::explicit_reads::assert_ordinary_execution_allowed(
            self.storage().into(),
            "ordinary field request",
        );
        let selected = self.select();
        selected.read.record_ordinary(self.storage().1);
        self.convert(selected.stored)
    }

    #[doc(hidden)]
    fn storage(&self) -> (&'db Zalsa, &'db ZalsaLocal);
    #[doc(hidden)]
    fn select(&self) -> FieldSelection<'db, Self::Stored, Self::Dependency>;
    #[doc(hidden)]
    fn convert(&self, stored: &'db Self::Stored) -> Self::Output;
    #[doc(hidden)]
    fn mode(&self) -> FieldReturnMode;
}

#[doc(hidden)]
pub struct FieldSelection<'db, T, D> {
    stored: &'db T,
    read: D,
}


/// Borrows the existing field-read owner while its dependency is recorded.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct FieldRecordingContext<'call, 'run, 'db> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
    owner: &'call EntryScope<'call, 'db>,
    local: &'call ZalsaLocal,
}

/// Keeps the request's dependency capability in its concrete recording future.
#[doc(hidden)]
pub trait FieldDependencyState: sealed::Sealed + Sized + 'static {
    const RECORDING_QUOTE: Option<(usize, usize)>;

    fn record_ordinary(self, local: &ZalsaLocal);

    fn record<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        context: FieldRecordingContext<'call, 'run, 'db>,
    ) -> impl Future<Output = ()> + 'call;
}

#[doc(hidden)]
pub struct NoFieldDependency;

impl sealed::Sealed for NoFieldDependency {}

impl FieldDependencyState for NoFieldDependency {
    const RECORDING_QUOTE: Option<(usize, usize)> = recording_quote::<Self, _, _, 8, 170>(
        &|state: &'static Self, context: FieldRecordingContext<'static, 'static, 'static>| {
            state.record(context)
        },
        true,
    );

    fn record_ordinary(self, _local: &ZalsaLocal) {}

    fn record<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        _context: FieldRecordingContext<'call, 'run, 'db>,
    ) -> impl Future<Output = ()> + 'call {
        ready(())
    }
}

#[doc(hidden)]
pub struct OptionalFieldDependency(Option<FieldDependency>);

impl sealed::Sealed for OptionalFieldDependency {}

impl FieldDependencyState for OptionalFieldDependency {
    const RECORDING_QUOTE: Option<(usize, usize)> = recording_quote::<Self, _, _, 7, 84>(
        &|state: &'static Self, context: FieldRecordingContext<'static, 'static, 'static>| {
            state.record(context)
        },
        false,
    );

    fn record_ordinary(self, local: &ZalsaLocal) {
        if let Some(read) = self.0 {
            local.report_tracked_read_simple(read.input, read.durability, read.changed_at);
        }
    }

    async fn record<'call, 'run: 'call, 'db: 'run>(
        &'call self,
        context: FieldRecordingContext<'call, 'run, 'db>,
    ) {
        if let Some(read) = &self.0 {
            read_run::dependency(
                context.endpoint,
                context.owner,
                context.local,
                &read.as_read(),
            )
            .await;
        }
    }
}

// The borrowed constructor identifies the future type without creating a context or a future.
// Its static lifetimes are a type witness; recording uses the actual field-read borrow lifetimes.
const fn recording_quote<D: 'static, F: Future<Output = ()>, M, const TRANSFERS: usize, const WORK: usize>(
    _constructor: &M,
    ready: bool,
) -> Option<(usize, usize)>
where
    M: FnOnce(&'static D, FieldRecordingContext<'static, 'static, 'static>) -> F,
{
    let groups = [
        (TRANSFERS, size_of::<F>()),
        (4, size_of::<FieldRecordingContext<'static, 'static, 'static>>()),
        (16, size_of::<&()>()),
        (2, size_of::<(&Zalsa, &ZalsaLocal)>()),
        (if ready { 3 } else { 0 }, size_of::<&str>()),
        (if ready { 12 } else { 0 }, size_of::<Option<()>>()),
        (8, size_of::<Poll<()>>()),
    ];
    let mut bytes = 0usize;
    let mut index = 0;
    while index < groups.len() {
        let (count, width) = groups[index];
        let Some(group) = count.checked_mul(width) else {
            return None;
        };
        let Some(total) = bytes.checked_add(group) else {
            return None;
        };
        bytes = total;
        index += 1;
    }
    Some((WORK, bytes))
}

const RECORDING_PREPARATION_WORK: usize = 3 * (3 * 5 + 4 * 4) + 10;
const RECORDING_PREPARATION_BYTES: usize = 3
    * (5 * size_of::<RunResult<()>>()
        + 4 * size_of::<ControlFlow<RunResult<Infallible>, ()>>()
        + 3 * size_of::<RunResult<Infallible>>()
        + 4 * size_of::<RunError>())
    + 3 * size_of::<Option<(usize, usize)>>()
    + 3 * size_of::<(usize, usize)>()
    + 8 * size_of::<usize>();

struct FieldDependency {
    input: DatabaseKeyIndex,
    durability: Durability,
    changed_at: Revision,
}

impl FieldDependency {
    fn as_read(&self) -> DependencyRead<'_> {
        DependencyRead::Simple {
            input: self.input,
            durability: self.durability,
            changed_at: self.changed_at,
        }
    }
}

#[doc(hidden)]
pub struct InputFieldRequest<'db, C: input::Configuration<Struct: Copy>, T, R> {
    context: FieldRequestContext<'db>,
    value: C::Struct,
    index: usize,
    field: fn(&'db C::Fields) -> &'db T,
    convert: fn(&'db T) -> R,
    mode: FieldReturnMode,
}

impl<'db, C: input::Configuration<Struct: Copy>, T, R> InputFieldRequest<'db, C, T, R> {
    pub fn new(
        context: FieldRequestContext<'db>,
        value: C::Struct,
        index: usize,
        field: fn(&'db C::Fields) -> &'db T,
        convert: fn(&'db T) -> R,
        mode: FieldReturnMode,
    ) -> Self {
        Self {
            context,
            value,
            index,
            field,
            convert,
            mode,
        }
    }
}

impl<C: input::Configuration<Struct: Copy>, T, R> sealed::Sealed
    for InputFieldRequest<'_, C, T, R>
{
}

impl<'db, C: input::Configuration<Struct: Copy>, T: 'db, R: 'db> FieldRequest<'db>
    for InputFieldRequest<'db, C, T, R>
{
    type Stored = T;
    type Output = R;
    type Dependency = OptionalFieldDependency;

    fn storage(&self) -> (&'db Zalsa, &'db ZalsaLocal) {
        self.context.storage
    }

    fn select(&self) -> FieldSelection<'db, T, Self::Dependency> {
        let zalsa = self.context.storage.0;
        let id = self.value.as_id();
        let ingredient = zalsa
            .lookup_ingredient(zalsa.table().ingredient_index(id))
            .assert_type::<input::IngredientImpl<C>>();
        let (fields, input, durability, changed_at) =
            ingredient.select_field(zalsa, self.value, self.index);
        let read = FieldDependency {
            input,
            durability,
            changed_at,
        };
        FieldSelection {
            stored: (self.field)(fields),
            read: OptionalFieldDependency(Some(read)),
        }
    }

    fn convert(&self, stored: &'db T) -> R {
        (self.convert)(stored)
    }
    fn mode(&self) -> FieldReturnMode {
        self.mode
    }
}

#[doc(hidden)]
pub struct TrackedFieldRequest<'db, C: tracked_struct::Configuration, T, R> {
    context: FieldRequestContext<'db>,
    value: C::Struct<'db>,
    index: Option<usize>,
    field: fn(&'db C::Fields<'db>) -> &'db T,
    convert: fn(&'db T) -> R,
    mode: FieldReturnMode,
}

impl<'db, C: tracked_struct::Configuration, T, R> TrackedFieldRequest<'db, C, T, R> {
    pub fn new(
        context: FieldRequestContext<'db>,
        value: C::Struct<'db>,
        index: Option<usize>,
        field: fn(&'db C::Fields<'db>) -> &'db T,
        convert: fn(&'db T) -> R,
        mode: FieldReturnMode,
    ) -> Self {
        Self {
            context,
            value,
            index,
            field,
            convert,
            mode,
        }
    }
}

impl<C: tracked_struct::Configuration, T, R> sealed::Sealed for TrackedFieldRequest<'_, C, T, R> {}

impl<'db, C: tracked_struct::Configuration, T: 'db, R: 'db> FieldRequest<'db>
    for TrackedFieldRequest<'db, C, T, R>
{
    type Stored = T;
    type Output = R;
    type Dependency = OptionalFieldDependency;

    fn storage(&self) -> (&'db Zalsa, &'db ZalsaLocal) {
        self.context.storage
    }

    fn select(&self) -> FieldSelection<'db, T, Self::Dependency> {
        let zalsa = self.context.storage.0;
        let ingredient = zalsa
            .lookup_ingredient(zalsa.table().ingredient_index(self.value.as_id()))
            .assert_type::<tracked_struct::IngredientImpl<C>>();
        let (fields, read) = if let Some(index) = self.index {
            let (fields, input, durability, changed_at) =
                ingredient.select_tracked_field(zalsa, self.value, index);
            let read = FieldDependency {
                input,
                durability,
                changed_at,
            };
            (fields, Some(read))
        } else {
            (ingredient.select_untracked_field(zalsa, self.value), None)
        };
        FieldSelection {
            stored: (self.field)(fields),
            read: OptionalFieldDependency(read),
        }
    }

    fn convert(&self, stored: &'db T) -> R {
        (self.convert)(stored)
    }
    fn mode(&self) -> FieldReturnMode {
        self.mode
    }
}

#[doc(hidden)]
pub struct InternedFieldRequest<'db, C: interned::Configuration, T, R> {
    context: FieldRequestContext<'db>,
    value: C::Struct<'db>,
    field: fn(&'db C::Fields<'db>) -> &'db T,
    convert: fn(&'db T) -> R,
    mode: FieldReturnMode,
}

impl<'db, C: interned::Configuration, T, R> InternedFieldRequest<'db, C, T, R> {
    pub fn new(
        context: FieldRequestContext<'db>,
        value: C::Struct<'db>,
        field: fn(&'db C::Fields<'db>) -> &'db T,
        convert: fn(&'db T) -> R,
        mode: FieldReturnMode,
    ) -> Self {
        Self {
            context,
            value,
            field,
            convert,
            mode,
        }
    }
}

impl<C: interned::Configuration, T, R> sealed::Sealed for InternedFieldRequest<'_, C, T, R> {}

impl<'db, C: interned::Configuration, T: 'db, R: 'db> FieldRequest<'db>
    for InternedFieldRequest<'db, C, T, R>
{
    type Stored = T;
    type Output = R;
    type Dependency = NoFieldDependency;

    fn storage(&self) -> (&'db Zalsa, &'db ZalsaLocal) {
        self.context.storage
    }

    fn select(&self) -> FieldSelection<'db, T, Self::Dependency> {
        let zalsa = self.context.storage.0;
        let ingredient = zalsa
            .lookup_ingredient(zalsa.table().ingredient_index(self.value.as_id()))
            .assert_type::<interned::IngredientImpl<C>>();
        FieldSelection {
            stored: (self.field)(ingredient.select_fields(zalsa, self.value)),
            read: NoFieldDependency,
        }
    }

    fn convert(&self, stored: &'db T) -> R {
        (self.convert)(stored)
    }

    fn mode(&self) -> FieldReturnMode {
        self.mode
    }
}

pub(super) async fn read<'call, 'run: 'call, 'db: 'run, R, P>(
    endpoint: &'call TaskEndpoint<'run, 'db>,
    request: R,
    profile: &'call P,
) -> R::Output
where
    R: FieldRequest<'db> + 'call,
    P: FieldReadProfile<R::Stored>,
{
    if !endpoint.inner.local_call_is_eligible() {
        let _held = request;
        return pending().await;
    }
    let scope = match EntryScope::capture(&endpoint.inner.context) {
        Ok(scope) => scope,
        Err(error) => match callback::reject(&endpoint.inner, error, request).await {},
    };
    callback::complete(
        &endpoint.inner,
        &scope,
        CallbackKind::NativeAdmission,
        || {
            ready((|| {
                let (zalsa, local) = request.storage();
                if !std::ptr::eq(zalsa, endpoint.inner.context.db.zalsa())
                    || !std::ptr::eq(local, endpoint.inner.context.db.zalsa_local())
                {
                    return Err(RunError::Contract(
                        "field request belongs to another database",
                    ));
                }
                // Selection uses direct table/ingredient indices, a type check, and (for tracked
                // fields) the canonical revision read lock. Interned fields retain the canonical
                // reusable-value check under their shard lock. Selection performs no user conversion.
    endpoint.admit_work(16 + RECORDING_PREPARATION_WORK)?;
            endpoint.admit(super::ExecutionWork::Resource {
                requested_bytes: RECORDING_PREPARATION_BYTES,
            })?;
            let Some((work, requested_bytes)) = R::Dependency::RECORDING_QUOTE else {
                return Err(RunError::Contract("field recording quotation overflow"));
            };
            endpoint.admit_work(work)?;
            endpoint.admit(super::ExecutionWork::Resource { requested_bytes })
            })())
        },
    )
    .await;
    let selected = callback::complete(&endpoint.inner, &scope, CallbackKind::NativeCall, || {
        ready(Ok(request.select()))
    })
    .await;
    let quote = callback::complete(
        &endpoint.inner,
        &scope,
        CallbackKind::NativeQuotation,
        || profile.quote(endpoint, selected.stored, request.mode()),
    )
    .await;
    native_values::admit_quote(&endpoint.inner, &scope, quote).await;
    selected
        .read
        .record(FieldRecordingContext {
            endpoint,
            owner: &scope,
            local: request.storage().1,
        })
        .await;
    callback::complete(&endpoint.inner, &scope, CallbackKind::NativeCall, || {
        ready(Ok(request.convert(selected.stored)))
    })
    .await
}
