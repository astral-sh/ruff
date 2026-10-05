#[cfg(all(test, not(feature = "shuttle")))]
pub(crate) use delete::{Allocations, observe_allocations};
pub(crate) use maybe_changed_after::VerifyResult;
pub(crate) use memo::ErasedMemo;
pub(crate) use sync::{
    ClaimGuard, ClaimResult, Reentrancy, SyncGuard, SyncOwner, SyncTable, TransferSource,
};

use std::any::Any;
use std::fmt;
use std::hash::Hash;
use std::ptr::NonNull;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use crate::cycle::{CycleRecoveryStrategy, ProvisionalStatus};
use crate::database::RawDatabase;
use crate::function::delete::{DeletedEntries, PreparedRetirement};
use crate::hash::{FxHashSet, FxIndexSet};
use crate::ingredient::Ingredient;
use crate::key::DatabaseKeyIndex;
use crate::plumbing::{self, MemoIngredientMap};
use crate::salsa_struct::SalsaStructInDb;
use crate::sync::Arc;
use crate::table::Table;
use crate::table::memo::{MemoSlot, MemoTableTypes, PreparedMemoSlot};
use crate::views::DatabaseDownCaster;
use crate::zalsa::{IngredientIndex, JarKind, MemoIngredientIndex, Zalsa};
use crate::zalsa_local::QueryEdge;
use crate::{Cycle, Id, Revision};

#[cfg(feature = "accumulator")]
mod accumulated;
mod backdate;
mod delete;
mod diff_outputs;
mod eviction;
pub(crate) mod execute;
mod fetch;
mod maybe_changed_after;
mod memo;
mod specify;
mod sync;

pub use eviction::{EvictionPolicy, HasCapacity, Lru, NoopEviction};

pub type Memo<C> = memo::Memo<C>;

/// Stored query arguments, inspected without running their native conversion.
///
/// Interned arguments borrow the tuple that `id_to_input` will clone. A query associated with
/// one Salsa struct needs only its identity; constructing that handle remains a separately
/// admitted operation.
pub enum RetainedInput<'call, 'db, C: Configuration + ?Sized> {
    Interned(&'call C::Input<'db>),
    SalsaStruct(Id),
}

/// Configuration for a Salsa function ingredient.
///
/// # Safety
///
/// For every lifetime `'db`, `Output<'db>` must be safe for Salsa to retain
/// after erasing `'db` and to use after rebranding it with a later database
/// lifetime. This is guaranteed when the output implements [`crate::SalsaValue`]
/// or when it is the same `'static` type for every `'db`.
pub unsafe trait Configuration: Any {
    const DEBUG_NAME: &'static str;
    const LOCATION: crate::ingredient::Location;
    const PERSIST: bool;

    /// The database that this function is associated with.
    type DbView: ?Sized + crate::Database;

    /// The "salsa struct type" that this function is associated with.
    /// This can be just `salsa::Id` for functions that intern their arguments
    /// and are not clearly associated with any one salsa struct.
    type SalsaStruct<'db>: SalsaStructInDb;

    /// The input to the function
    type Input<'db>: Send + Sync;

    /// The value computed by the function.
    type Output<'db>: Send + Sync;

    /// The eviction policy for this function's memoized values.
    type Eviction: EvictionPolicy;

    /// Determines whether this function can recover from being a participant in a cycle
    /// (and, if so, how).
    const CYCLE_STRATEGY: CycleRecoveryStrategy;
    const ATTEMPT_POLICY: crate::attempt_probe::QueryPolicy =
        crate::attempt_probe::QueryPolicy::Unclassified;

    /// Invokes after a new result `new_value` has been computed for which an older memoized value
    /// existed `old_value`, or in fixpoint iteration. Returns true if the new value is equal to
    /// the older one.
    ///
    /// This invokes user code in form of the `Eq` impl.
    fn values_equal<'db>(old_value: &Self::Output<'db>, new_value: &Self::Output<'db>) -> bool;

    /// Convert from the id used internally to the value that execute is expecting.
    /// This is a no-op if the input to the function is a salsa struct.
    fn id_to_input(zalsa: &Zalsa, key: Id) -> Self::Input<'_>;

    /// Returns a bounded borrowed view for quoting `id_to_input`, without cloning arguments.
    ///
    /// An implementation may only perform a fixed amount of metadata lookup. Configurations
    /// without this accessor cannot convert inputs through controlled execution.
    fn retained_input<'db>(_zalsa: &'db Zalsa, _key: Id) -> Option<RetainedInput<'db, 'db, Self>> {
        None
    }

    /// Returns the size of any heap allocations in the output value, in bytes.
    fn heap_size(_value: &Self::Output<'_>) -> Option<usize> {
        None
    }

    /// Invoked when we need to compute the value for the given key, either because we've never
    /// computed it before or because the old one relied on inputs that have changed.
    ///
    /// This invokes the function the user wrote.
    fn execute<'db>(db: &'db Self::DbView, input: Self::Input<'db>) -> Self::Output<'db>;

    /// Get the cycle recovery initial value.
    fn cycle_initial<'db>(
        db: &'db Self::DbView,
        id: Id,
        input: Self::Input<'db>,
    ) -> Self::Output<'db>;

    /// Decide what value to use for this cycle iteration. Takes ownership of the new value
    /// and returns an owned value to use.
    ///
    /// The function is called for every iteration of the cycle head, regardless of whether the cycle
    /// has converged (the values are equal).
    ///
    /// # Id
    ///
    /// The id can be used to uniquely identify the query instance. This can be helpful
    /// if the cycle function has to re-identify a value it returned previously.
    ///
    /// # Values
    ///
    /// The `last_provisional_value` is the value from the previous iteration of this cycle
    /// and `value` is the new value that was computed in the current iteration.
    ///
    /// # Iteration count
    ///
    /// The `iteration` parameter isn't guaranteed to start from zero or to be contiguous:
    ///
    /// * **Initial value**: `iteration` may be non-zero on the first call for a given query if that
    ///   query becomes the outermost cycle head after a nested cycle complete a few iterations. In this case,
    ///   `iteration` continues from the nested cycle's iteration count rather than resetting to zero.
    /// * **Non-contiguous values**: The iteration count can be non-contigious for cycle heads
    ///   that are only conditionally part of a cycle.
    ///
    /// # Return value
    ///
    /// The function should return the value to use for this iteration. This can be the `value`
    /// that was computed, or a different value (e.g., a fallback value). This cycle will continue
    /// iterating until the returned value equals the previous iteration's value.
    fn recover_from_cycle<'db>(
        db: &'db Self::DbView,
        cycle: &Cycle,
        last_provisional_value: &Self::Output<'db>,
        value: Self::Output<'db>,
        input: Self::Input<'db>,
    ) -> Self::Output<'db>;

    /// Serialize the output type using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn serialize<S>(value: &Self::Output<'_>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: plumbing::serde::Serializer;

    /// Deserialize the output type using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn deserialize<'de, D>(deserializer: D) -> Result<Self::Output<'static>, D::Error>
    where
        D: plumbing::serde::Deserializer<'de>;
}

/// Connects a tracked function to its generated argument interner.
/// The function's inputs and key handles are the interner's fields and struct, respectively.
#[doc(hidden)]
pub trait InternedQueryConfiguration:
    crate::interned::Configuration
    + for<'db> Configuration<
        Input<'db> = <Self as crate::interned::Configuration>::Fields<'db>,
        SalsaStruct<'db> = <Self as crate::interned::Configuration>::Struct<'db>,
    >
{
    fn argument_ingredient(zalsa: &Zalsa) -> &crate::interned::IngredientImpl<Self>;
}

/// Query-key fields whose hashing and equality inspect only a fixed amount of scalar data.
///
/// Implementations must not follow semantic data, execute queries, or invoke user callbacks.
/// This lets supervised key requests use ordinary finite collection operations between progress
/// checks. It does not make hash-table probing constant-time or bound allocator latency.
#[doc(hidden)]
pub trait FixedQueryFields: Copy + Eq + Hash {}

/// Cached outputs whose destruction performs finite, nonsemantic work.
///
/// The quote must inspect only native payload metadata, without allocation, mutation, queries,
/// or callbacks. Every stored output, including provisional values, must have passive destruction.
#[doc(hidden)]
pub trait PassiveMemoProfile<C: Configuration> {
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize>;

    /// Returns the `retired_output_work` total, consuming fuel before variable metadata visits.
    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut crate::quote::QuoteFuel,
    ) -> Result<usize, crate::quote::QuoteError>;
}

#[doc(hidden)]
pub struct CopyMemoProfile;

impl<C: Configuration> PassiveMemoProfile<C> for CopyMemoProfile
where
    for<'db> C::Output<'db>: Copy,
{
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        Some(0)
    }

    fn retired_output_work_bounded<'db>(
        _: &C::Output<'db>,
        fuel: &mut crate::quote::QuoteFuel,
    ) -> Result<usize, crate::quote::QuoteError> {
        fuel.consume(1)?;
        Ok(0)
    }
}

/// Generated query keys with finite, nonsemantic field and output retirement work.
///
/// Implementations certify passive generated handle construction, finite Hash/Eq and passive
/// destruction for both submitted and retained fields. These operations must not execute queries,
/// invoke callbacks or walk semantic graphs. The input quote inspects only native payload metadata, without
/// allocation, mutation, database access, or callbacks. Copy fields alone do not establish these
/// contracts. Output retirement, including provisional values, follows [`PassiveMemoProfile`].
/// Quotes use checked arithmetic and return `None` when their work cannot be represented.
///
/// The input quote counts payload work for interning and for destruction of retired fields.
/// The producer must separately admit construction or cloning of submitted fields and prepay
/// their cleanup: the key request can be refused before accepting its input quote. Ordinary finite
/// collection operations remain permitted; the quote does not bound hash probes or allocator latency.
#[doc(hidden)]
pub trait QueryKeyProfile<C: InternedQueryConfiguration>: PassiveMemoProfile<C> {
    fn input_work<'db>(input: &<C as crate::interned::Configuration>::Fields<'db>)
    -> Option<usize>;

    /// Returns the `input_work` total, consuming fuel before variable metadata visits.
    fn input_work_bounded<'db>(
        input: &<C as crate::interned::Configuration>::Fields<'db>,
        fuel: &mut crate::quote::QuoteFuel,
    ) -> Result<usize, crate::quote::QuoteError>;
}

#[doc(hidden)]
pub type FixedQueryKeyProfile = CopyMemoProfile;

impl<C> QueryKeyProfile<C> for CopyMemoProfile
where
    C: InternedQueryConfiguration,
    for<'db> <C as crate::interned::Configuration>::Fields<'db>: FixedQueryFields,
    for<'db> <C as Configuration>::Output<'db>: Copy,
{
    fn input_work<'db>(_: &<C as crate::interned::Configuration>::Fields<'db>) -> Option<usize> {
        Some(0)
    }

    fn input_work_bounded<'db>(
        _: &<C as crate::interned::Configuration>::Fields<'db>,
        fuel: &mut crate::quote::QuoteFuel,
    ) -> Result<usize, crate::quote::QuoteError> {
        fuel.consume(1)?;
        Ok(0)
    }
}

impl FixedQueryFields for () {}
impl FixedQueryFields for bool {}
impl FixedQueryFields for char {}
impl FixedQueryFields for u8 {}
impl FixedQueryFields for u16 {}
impl FixedQueryFields for u32 {}
impl FixedQueryFields for u64 {}
impl FixedQueryFields for u128 {}
impl FixedQueryFields for usize {}
impl FixedQueryFields for i8 {}
impl FixedQueryFields for i16 {}
impl FixedQueryFields for i32 {}
impl FixedQueryFields for i64 {}
impl FixedQueryFields for i128 {}
impl FixedQueryFields for isize {}
impl FixedQueryFields for Id {}

impl<A: FixedQueryFields, B: FixedQueryFields> FixedQueryFields for (A, B) {}

/// The function-specific interface for an [`Ingredient`].
///
/// This type is public because it appears in the public [`Ingredient`] trait, but it can only be
/// constructed and used inside salsa.
#[doc(hidden)]
pub struct FunctionIngredientRef<'db> {
    ingredient: &'db dyn FunctionIngredient,
}

impl<'db> FunctionIngredientRef<'db> {
    fn new(ingredient: &'db dyn FunctionIngredient) -> Self {
        Self { ingredient }
    }

    pub(crate) fn memo(&self, zalsa: &'db Zalsa, input: Id) -> Option<ErasedMemo<'db>> {
        self.ingredient.memo(zalsa, input)
    }

    pub(crate) fn sync_table(&self) -> &'db SyncTable {
        self.ingredient.sync_table()
    }

    pub(crate) fn attempt_policy(&self) -> crate::attempt_probe::QueryPolicy {
        self.ingredient.attempt_policy()
    }
}

pub(crate) trait FunctionIngredient: Send + Sync {
    fn memo<'db>(&'db self, zalsa: &'db Zalsa, input: Id) -> Option<ErasedMemo<'db>>;

    fn sync_table(&self) -> &SyncTable;

    fn attempt_policy(&self) -> crate::attempt_probe::QueryPolicy;
}

/// Function ingredients are the "workhorse" of salsa.
///
/// They are used for tracked functions, for the "value" fields of tracked structs, and for the fields of input structs.
/// The function ingredient is fairly complex and so its code is spread across multiple modules, typically one per method.
/// The main entry points are:
///
/// * the `fetch` method, which is invoked when the function is called by the user's code;
///   it will return a memoized value if one exists, or execute the function otherwise.
/// * the `specify` method, which can only be used when the key is an entity created by the active query.
///   It sets the value of the function imperatively, so that when later fetches occur, they'll return this value.
/// * the `store` method, which can only be invoked with an `&mut` reference, and is to set input fields.
pub struct IngredientImpl<C: Configuration> {
    /// The ingredient index we were assigned in the database.
    /// Used to construct `DatabaseKeyIndex` values.
    index: IngredientIndex,

    /// The index for the memo/sync tables
    ///
    /// This may be a [`crate::memo_ingredient_indices::MemoIngredientSingletonIndex`] or a
    /// [`crate::memo_ingredient_indices::MemoIngredientIndices`], depending on whether the
    /// tracked function's struct is a plain salsa struct or an enum `#[derive(Supertype)]`.
    memo_ingredient_indices: <C::SalsaStruct<'static> as SalsaStructInDb>::MemoIngredientMap,

    /// Eviction policy - type determined by Configuration.
    /// Used to find memos to throw out when we have too many memoized values.
    eviction: C::Eviction,

    /// An downcaster to `C::DbView`.
    ///
    /// # Safety
    ///
    /// The supplied database must be be the same as the database used to construct the [`Views`]
    /// instances that this downcaster was derived from.
    view_caster: OnceLock<DatabaseDownCaster<C::DbView>>,

    sync_table: SyncTable,

    /// When `fetch` and friends executes, they return a reference to the
    /// value stored in the memo that is extended to live as long as the `&self`
    /// reference we start with. This means that whenever we remove something
    /// from `memo_map` with an `&self` reference, there *could* be references to its
    /// internals still in use. Therefore we push the memo into this queue and
    /// only *actually* free up memory when a new revision starts (which means
    /// we have an `&mut` reference to self).
    ///
    /// You might think that we could do this only if the memo was verified in the
    /// current revision: you would be right, but we are being defensive, because
    /// we don't know that we can trust the database to give us the same runtime
    /// everytime and so forth.
    deleted_entries: DeletedEntries<C>,
}

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    pub fn new(
        index: IngredientIndex,
        memo_ingredient_indices: <C::SalsaStruct<'static> as SalsaStructInDb>::MemoIngredientMap,
        eviction_capacity: usize,
    ) -> Self {
        Self {
            index,
            memo_ingredient_indices,
            eviction: C::Eviction::new(eviction_capacity),
            deleted_entries: Default::default(),
            view_caster: OnceLock::new(),
            sync_table: SyncTable::new(index),
        }
    }

    /// Set the view-caster for this tracked function ingredient, if it has
    /// not already been initialized.
    #[inline]
    pub fn get_or_init(
        &self,
        view_caster: impl FnOnce() -> DatabaseDownCaster<C::DbView>,
    ) -> &Self {
        // Note that we must set this lazily as we don't have access to the database
        // type when ingredients are registered into the `Zalsa`.
        self.view_caster.get_or_init(view_caster);
        self
    }

    #[inline]
    pub fn database_key_index(&self, key: Id) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.index, key)
    }

    /// Set eviction capacity. Only available when eviction policy supports it.
    pub fn set_capacity(&mut self, capacity: usize)
    where
        C::Eviction: HasCapacity,
    {
        self.eviction.set_capacity(capacity);
    }

    /// Returns a reference to the memo value that lives as long as self.
    /// This is UNSAFE: the caller is responsible for ensuring that the
    /// memo will not be released so long as the `&self` is valid.
    /// This is done by (a) ensuring the memo is present in the memo-map
    /// when this function is called and (b) ensuring that any entries
    /// removed from the memo-map are added to `deleted_entries`, which is
    /// only cleared with `&mut self`.
    unsafe fn extend_memo_lifetime<'this>(
        &'this self,
        memo: &memo::Memo<C>,
    ) -> &'this memo::Memo<C> {
        // SAFETY: the caller must guarantee that the memo will not be released before `&self`
        unsafe { std::mem::transmute(memo) }
    }

    fn insert_memo<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        id: Id,
        memo: memo::Memo<C>,
        memo_ingredient_index: MemoIngredientIndex,
    ) -> &'db memo::Memo<C> {
        let memo = memo::prepare_memo_allocation(memo);
        let slot = self
            .prepare_memo_slot(zalsa, id, memo_ingredient_index)
            .expect("memo index must identify a registered slot");
        self.install_memo_allocation(slot, memo, None)
    }

    fn install_memo_allocation<'db>(
        &'db self,
        slot: PreparedMemoSlot<'db, memo::Memo<C>>,
        memo: Box<memo::Memo<C>>,
        retirement: Option<PreparedRetirement<'db, C>>,
    ) -> &'db memo::Memo<C> {
        // We convert to a `NonNull` here as soon as possible because we are going to alias
        // into the `Box`, which is a `noalias` type.
        // FIXME: Use `Box::into_non_null` once stable
        let memo = NonNull::from(Box::leak(memo));
        let replaced = slot.swap(memo);
        if let Some(retirement) = retirement {
            // SAFETY: The swap removed the allocation from the table and this unique token
            // takes its ownership. No callback or allocation separates the two operations.
            unsafe { retirement.finish(replaced) };
        } else if let Some(old_value) = replaced {
            // In case there is a reference to the old memo out there, we have to store it
            // in the deleted entries. This will get cleared when a new revision starts.
            //
            // SAFETY: Once the revision starts, there will be no outstanding borrows to the
            // memo contents, and so it will be safe to free.
            unsafe { self.deleted_entries.push(old_value) };
        }
        // SAFETY: memo has been inserted into the table
        unsafe { self.extend_memo_lifetime(memo.as_ref()) }
    }

    #[inline]
    fn memo_ingredient_index(&self, zalsa: &Zalsa, id: Id) -> MemoIngredientIndex {
        self.memo_ingredient_indices.get_zalsa_id(zalsa, id)
    }

    fn view_caster(&self) -> &DatabaseDownCaster<C::DbView> {
        self.view_caster
            .get()
            .expect("tracked function ingredients cannot be accessed before calling `init`")
    }
}

impl<C> FunctionIngredient for IngredientImpl<C>
where
    C: Configuration,
{
    fn memo<'db>(&'db self, zalsa: &'db Zalsa, input: Id) -> Option<ErasedMemo<'db>> {
        // SAFETY: `self` is borrowed for `'db`, so `deleted_entries` retains every allocation
        // observed in this slot for that lifetime.
        let memo_slot = unsafe {
            MemoSlot::new(
                zalsa.memo_table_for::<C::SalsaStruct<'_>>(input),
                self.memo_ingredient_index(zalsa, input),
            )
        };

        memo_slot.get_erased()
    }

    fn sync_table(&self) -> &SyncTable {
        &self.sync_table
    }

    fn attempt_policy(&self) -> crate::attempt_probe::QueryPolicy {
        C::ATTEMPT_POLICY
    }
}

impl<C> Ingredient for IngredientImpl<C>
where
    C: Configuration,
{
    fn location(&self) -> &'static crate::ingredient::Location {
        &C::LOCATION
    }

    fn ingredient_index(&self) -> IngredientIndex {
        self.index
    }

    unsafe fn maybe_changed_after(
        &self,
        _zalsa: &Zalsa,
        db: RawDatabase<'_>,
        input: Id,
        revision: Revision,
    ) -> VerifyResult {
        // SAFETY: The `db` belongs to the ingredient as per caller invariant
        let db = unsafe { self.view_caster().downcast_unchecked(db) };
        self.maybe_changed_after(db, input, revision)
    }

    fn collect_minimum_serialized_edges(
        &self,
        zalsa: &Zalsa,
        edge: QueryEdge,
        serialized_edges: &mut FxIndexSet<QueryEdge>,
        visited_edges: &mut FxHashSet<QueryEdge>,
    ) {
        collect_minimum_serialized_edges(self, zalsa, edge, serialized_edges, visited_edges);
    }

    fn as_function(&self) -> Option<FunctionIngredientRef<'_>> {
        Some(FunctionIngredientRef::new(self))
    }

    fn flatten_cycle_head_dependencies(
        &self,
        zalsa: &Zalsa,
        id: Id,
        flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        seen: &mut FxHashSet<DatabaseKeyIndex>,
    ) {
        flatten_cycle_head_dependencies(
            self,
            zalsa,
            self.database_key_index(id),
            C::CYCLE_STRATEGY,
            flattened_input_outputs,
            seen,
        );
    }

    fn mark_validated_output(
        &self,
        zalsa: &Zalsa,
        executor: DatabaseKeyIndex,
        output_key: crate::Id,
    ) {
        let memo_ingredient_index = self.memo_ingredient_index(zalsa, output_key);
        let database_key_index = self.database_key_index(output_key);
        let memo_slot = self.memo_slot(zalsa, output_key, memo_ingredient_index);
        specify::validate_specified_value(zalsa, executor, database_key_index, memo_slot);
    }

    fn remove_stale_output(
        &self,
        _zalsa: &Zalsa,
        _executor: DatabaseKeyIndex,
        _stale_output_key: crate::Id,
    ) {
        // This function is invoked when a query Q specifies the value for `stale_output_key` in rev 1,
        // but not in rev 2. We don't do anything in this case, we just leave the (now stale) memo.
        // Since its `verified_at` field has not changed, it will be considered dirty if it is invoked.
    }

    fn requires_reset_for_new_revision(&self) -> bool {
        true
    }

    fn reset_for_new_revision(&mut self, table: &mut Table) {
        self.eviction.for_each_evicted(|evict| {
            let ingredient_index = table.ingredient_index(evict);
            Self::evict_value_from_memo_for(
                table.memos_mut(evict),
                self.memo_ingredient_indices.get(ingredient_index),
            )
        });

        self.deleted_entries.clear();
    }

    fn debug_name(&self) -> &'static str {
        C::DEBUG_NAME
    }

    fn jar_kind(&self) -> JarKind {
        JarKind::TrackedFn
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        unreachable!("function does not allocate pages")
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        unreachable!("function does not allocate pages")
    }

    #[cfg(feature = "accumulator")]
    unsafe fn accumulated<'db>(
        &'db self,
        db: RawDatabase<'db>,
        key_index: Id,
    ) -> (
        Option<&'db crate::accumulator::accumulated_map::AccumulatedMap>,
        crate::accumulator::accumulated_map::InputAccumulatedValues,
    ) {
        // SAFETY: The `db` belongs to the ingredient as per caller invariant
        let db = unsafe { self.view_caster().downcast_unchecked(db) };
        self.accumulated_map(db, key_index)
    }

    fn is_persistable(&self) -> bool {
        C::PERSIST
    }

    fn should_serialize(&self, zalsa: &Zalsa) -> bool {
        if !C::PERSIST {
            return false;
        }

        // We only serialize the query if there are any memos associated with it.
        for entry in <C::SalsaStruct<'_> as SalsaStructInDb>::entries(zalsa) {
            let memo_ingredient_index = self.memo_ingredient_indices.get(entry.ingredient_index());

            let memo =
                self.get_memo_from_table_for(zalsa, entry.key_index(), memo_ingredient_index);

            if memo.is_some_and(|memo| memo.should_serialize()) {
                return true;
            }
        }

        false
    }

    #[cfg(feature = "persistence")]
    unsafe fn serialize<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        f: &mut dyn FnMut(&dyn erased_serde::Serialize),
    ) {
        f(&persistence::SerializeIngredient {
            zalsa,
            ingredient: self,
        })
    }

    #[cfg(feature = "persistence")]
    fn deserialize(
        &mut self,
        zalsa: &mut Zalsa,
        deserializer: &mut dyn erased_serde::Deserializer,
    ) -> Result<(), erased_serde::Error> {
        let deserialize = persistence::DeserializeIngredient {
            zalsa,
            ingredient: self,
        };

        serde::de::DeserializeSeed::deserialize(deserialize, deserializer)
    }
}

fn collect_minimum_serialized_edges(
    ingredient: &dyn FunctionIngredient,
    zalsa: &Zalsa,
    edge: QueryEdge,
    serialized_edges: &mut FxIndexSet<QueryEdge>,
    visited_edges: &mut FxHashSet<QueryEdge>,
) {
    let Some(memo) = ingredient.memo(zalsa, edge.key().key_index()) else {
        return;
    };

    visited_edges.insert(edge);

    // Collect the minimum dependency tree.
    for edge in memo.header().origin().edges() {
        // Avoid forming cycles.
        if visited_edges.contains(&edge) {
            continue;
        }

        // Avoid flattening edges that we're going to serialize directly.
        if serialized_edges.contains(&edge) {
            continue;
        }

        let dependency = zalsa.lookup_ingredient(edge.key().ingredient_index());
        dependency.collect_minimum_serialized_edges(zalsa, edge, serialized_edges, visited_edges);
    }
}

fn flatten_cycle_head_dependencies(
    ingredient: &dyn FunctionIngredient,
    zalsa: &Zalsa,
    database_key_index: DatabaseKeyIndex,
    cycle_recovery_strategy: CycleRecoveryStrategy,
    flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
    seen: &mut FxHashSet<DatabaseKeyIndex>,
) {
    let Some(memo) = ingredient.memo(zalsa, database_key_index.key_index()) else {
        return;
    };

    let memo_header = memo.header();

    // Only flatten dependencies of provisional queries, because only those
    // contain cyclic dependencies.
    if !memo_header.may_be_provisional() {
        flattened_input_outputs.insert(QueryEdge::input(database_key_index));
        return;
    }

    // There's nothing to do if we've visited this query before.
    if !seen.insert(database_key_index) {
        return;
    }

    let inputs = memo_header.origin().inputs();

    match cycle_recovery_strategy {
        // Queries with cycle handling already flattened their own dependencies when completing
        // the query. Cycle participants commonly share most of those inputs, often in long
        // contiguous runs, so compare by ordered index to avoid a hash-table lookup for every
        // edge. On a mismatch, `insert_full` preserves insertion order and resynchronizes the
        // expected index.
        CycleRecoveryStrategy::FallbackImmediate | CycleRecoveryStrategy::Fixpoint => {
            let mut expected_index = 0;
            for input in inputs.map(QueryEdge::input) {
                if flattened_input_outputs.get_index(expected_index) == Some(&input) {
                    expected_index += 1;
                } else {
                    let (index, _) = flattened_input_outputs.insert_full(input);
                    expected_index = index + 1;
                }
            }
        }
        // For regular queries, recurse
        CycleRecoveryStrategy::Panic => {
            for input in inputs {
                let ingredient = zalsa.lookup_ingredient(input.ingredient_index());
                ingredient.flatten_cycle_head_dependencies(
                    zalsa,
                    input.key_index(),
                    flattened_input_outputs,
                    seen,
                );
            }
        }
    }
}

impl memo::MemoHeader {
    fn provisional_status(&self, has_value: bool) -> ProvisionalStatus {
        if !has_value && self.may_be_provisional() {
            return ProvisionalStatus::Poisoned {
                iteration: self.revisions.iteration(),
                verified_at: self.verified_at.load(),
            };
        }
        if self.has_incomplete_attempt() {
            return ProvisionalStatus::Incomplete;
        }
        if self.revisions.verified_final.load(Ordering::Acquire) {
            ProvisionalStatus::Final
        } else {
            ProvisionalStatus::Provisional
        }
    }

    fn cycle_converged(&self) -> bool {
        self.revisions.cycle_converged()
    }
}

impl<C> std::fmt::Debug for IngredientImpl<C>
where
    C: Configuration,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("index", &self.index)
            .finish()
    }
}

#[cfg(feature = "persistence")]
mod persistence {
    use super::{Configuration, IngredientImpl, Memo};
    use crate::hash::{FxHashSet, FxIndexSet};
    use crate::plumbing::{MemoIngredientMap, SalsaStructInDb};
    use crate::zalsa::Zalsa;
    use crate::zalsa_local::persistence::PersistentQueryOrigin;
    use crate::zalsa_local::{QueryEdge, QueryOriginRef};
    use crate::{Id, IngredientIndex};

    use serde::de;
    use serde::ser::SerializeMap;

    pub struct SerializeIngredient<'db, C>
    where
        C: Configuration,
    {
        pub zalsa: &'db Zalsa,
        pub ingredient: &'db IngredientImpl<C>,
    }

    impl<C> serde::Serialize for SerializeIngredient<'_, C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let Self { ingredient, zalsa } = self;

            let count = <C::SalsaStruct<'_> as SalsaStructInDb>::entries(zalsa)
                .filter(|entry| {
                    let memo_ingredient_index = ingredient
                        .memo_ingredient_indices
                        .get(entry.ingredient_index());

                    let memo = ingredient.get_memo_from_table_for(
                        zalsa,
                        entry.key_index(),
                        memo_ingredient_index,
                    );

                    memo.is_some_and(|memo| memo.should_serialize())
                })
                .count();

            let mut map = serializer.serialize_map(Some(count))?;

            let mut visited_edges = FxHashSet::default();
            let mut flattened_edges = FxIndexSet::default();

            for entry in <C::SalsaStruct<'_> as SalsaStructInDb>::entries(zalsa) {
                let memo_ingredient_index = ingredient
                    .memo_ingredient_indices
                    .get(entry.ingredient_index());

                let memo = ingredient.get_memo_from_table_for(
                    zalsa,
                    entry.key_index(),
                    memo_ingredient_index,
                );

                if let Some(memo) = memo.filter(|memo| memo.should_serialize()) {
                    // Flatten the dependencies of this query down to the base inputs.
                    let flattened_origin = match memo.header.origin() {
                        QueryOriginRef::Derived(edges) => {
                            collect_minimum_serialized_edges(
                                zalsa,
                                edges,
                                &mut visited_edges,
                                &mut flattened_edges,
                            );

                            PersistentQueryOrigin::derived(flattened_edges.drain(..))
                        }
                        QueryOriginRef::DerivedUntracked(edges) => {
                            collect_minimum_serialized_edges(
                                zalsa,
                                edges,
                                &mut visited_edges,
                                &mut flattened_edges,
                            );

                            PersistentQueryOrigin::derived_untracked(flattened_edges.drain(..))
                        }
                        QueryOriginRef::Assigned(key) => {
                            let dependency = zalsa.lookup_ingredient(key.ingredient_index());
                            assert!(
                                dependency.is_persistable(),
                                "specified query `{}` must be persistable",
                                dependency.debug_name()
                            );

                            PersistentQueryOrigin::assigned(key)
                        }
                    };

                    let memo = memo.with_origin(flattened_origin);

                    // TODO: Group structs by ingredient index into a nested map.
                    let key = format!(
                        "{}:{}",
                        entry.ingredient_index().as_u32(),
                        entry.key_index().as_bits()
                    );

                    map.serialize_entry(&key, &memo)?;

                    visited_edges.clear();
                }
            }

            map.end()
        }
    }

    // Flatten the dependency edges before serialization.
    fn collect_minimum_serialized_edges(
        zalsa: &Zalsa,
        edges: crate::zalsa_local::QueryEdges<'_>,
        visited_edges: &mut FxHashSet<QueryEdge>,
        flattened_edges: &mut FxIndexSet<QueryEdge>,
    ) {
        for edge in edges {
            let dependency = zalsa.lookup_ingredient(edge.key().ingredient_index());

            if dependency.is_persistable() {
                // If the dependency will be serialized, we can serialize the edge directly.
                flattened_edges.insert(edge);
            } else {
                // Otherwise, serialize the minimum edges necessary to cover the dependency.
                dependency.collect_minimum_serialized_edges(
                    zalsa,
                    edge,
                    flattened_edges,
                    visited_edges,
                );
            }
        }
    }

    pub struct DeserializeIngredient<'db, C>
    where
        C: Configuration,
    {
        pub zalsa: &'db Zalsa,
        pub ingredient: &'db mut IngredientImpl<C>,
    }

    impl<'de, C> de::DeserializeSeed<'de> for DeserializeIngredient<'_, C>
    where
        C: Configuration,
    {
        type Value = ();

        fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            deserializer.deserialize_map(self)
        }
    }

    impl<'de, C> de::Visitor<'de> for DeserializeIngredient<'_, C>
    where
        C: Configuration,
    {
        type Value = ();

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let DeserializeIngredient { zalsa, ingredient } = self;

            while let Some((key, memo)) = access.next_entry::<&str, Memo<C>>()? {
                let (ingredient_index, id) = key
                    .split_once(':')
                    .ok_or_else(|| de::Error::custom("invalid database key"))?;

                let ingredient_index = IngredientIndex::new(
                    ingredient_index.parse::<u32>().map_err(de::Error::custom)?,
                );

                let id = Id::from_bits(id.parse::<u64>().map_err(de::Error::custom)?);

                let memo_ingredient_index =
                    ingredient.memo_ingredient_indices.get(ingredient_index);

                // SAFETY: We provide the current revision.
                let memo_table = unsafe { zalsa.table().dyn_memos(id, zalsa.current_revision()) };

                let slot = memo_table
                    .prepare_slot::<Memo<C>>(memo_ingredient_index)
                    .ok_or_else(|| de::Error::custom("invalid memo table slot"))?;
                ingredient.install_memo_allocation(slot, Box::new(memo), None);
            }

            Ok(())
        }
    }
}
