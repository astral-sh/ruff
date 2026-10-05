use std::any::TypeId;
use std::borrow::Cow;
use std::cell::UnsafeCell;
use std::convert::Infallible;
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use crossbeam_utils::CachePadded;
use intrusive_collections::{LinkedList, LinkedListLink, UnsafeRef, intrusive_adapter};
use rustc_hash::FxBuildHasher;
use smallvec::SmallVec;

use crate::durability::Durability;
use crate::function::VerifyResult;
use crate::function::execute::execution_run::explicit_reads::{
    OrdinaryAccess, assert_ordinary_execution_allowed,
};
use crate::hash::{FxHashSet, FxIndexSet};
use crate::id::{AsId, FromId};
use crate::ingredient::Ingredient;
use crate::plumbing::{self, Jar, ZalsaLocal};
use crate::revision::AtomicRevision;
use crate::sync::{Arc, Mutex, OnceLock};
use crate::table::Slot;
use crate::table::memo::{DetachedMemoTable, MemoTable, MemoTableTypes, MemoTableWithTypesMut};
use crate::zalsa::{IngredientIndex, JarKind, Zalsa};
use crate::zalsa_local::QueryEdge;
use crate::zalsa_local::dependency_read::DependencyRead;
use crate::{DatabaseKeyIndex, Event, EventKind, Id, Revision};

#[cfg(not(test))]
const DEFAULT_REVISIONS: usize = 3;

// More aggressive garbage collection by default when testing.
#[cfg(test)]
const DEFAULT_REVISIONS: usize = 1;

/// Trait that defines the key properties of an interned struct.
///
/// Implemented by the `#[salsa::interned]` macro when applied to
/// a struct.
///
/// # Safety
///
/// For every lifetime `'db`, `Fields<'db>` must be safe for Salsa to retain
/// as `Fields<'static>` and later expose with the lifetime of a database
/// borrow. Both types must have identical layouts and validity invariants.
pub unsafe trait Configuration: Sized + 'static {
    const LOCATION: crate::ingredient::Location;
    const DEBUG_NAME: &'static str;

    /// Whether this struct should be persisted with the database.
    const PERSIST: bool;

    // The minimum number of revisions that must pass before a stale value is garbage collected.
    const REVISIONS: NonZeroUsize = NonZeroUsize::new(DEFAULT_REVISIONS).unwrap();

    /// The fields of the struct being interned.
    type Fields<'db>: InternedData;

    /// The end user struct
    type Struct<'db>: Copy + FromId + AsId;

    /// Returns the size of any heap allocations in the output value, in bytes.
    fn heap_size(_value: &Self::Fields<'_>) -> Option<usize> {
        None
    }

    /// Serialize the fields using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn serialize<S>(value: &Self::Fields<'_>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: plumbing::serde::Serializer;

    /// Deserialize the fields using `serde`.
    ///
    /// Panics if the value is not persistable, i.e. `Configuration::PERSIST` is `false`.
    fn deserialize<'de, D>(deserializer: D) -> Result<Self::Fields<'static>, D::Error>
    where
        D: plumbing::serde::Deserializer<'de>;
}

pub trait InternedData: Sized + Eq + Hash + Clone + Sync + Send {}
impl<T: Eq + Hash + Clone + Sync + Send> InternedData for T {}

/// Interned fields whose collection operations perform finite, nonsemantic work.
///
/// Hashing, equality and destruction of both submitted and retained fields must not execute
/// queries, invoke callbacks or walk a semantic graph. Constructing `Struct` from an ID must be
/// passive too. This contract permits ordinary collection primitives between progress checks;
/// it does not bound hash probes, allocator latency or interruption latency.
#[doc(hidden)]
pub trait FiniteInternedConfiguration: Configuration {
    /// Counts directly observable payload work, or returns `None` on overflow.
    ///
    /// This inspection must itself be a finite, nonsemantic metadata walk. Implementations
    /// should count collection entries and variable field bytes, not retained table capacity.
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize>;

    /// Returns the same field-work total as `field_work`, consuming fuel before metadata visits.
    /// Exhaustion returns before the next unpaid visit; it does not change the completed total.
    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut crate::quote::QuoteFuel,
    ) -> Result<usize, crate::quote::QuoteError>;
}

pub struct JarImpl<C: Configuration> {
    phantom: PhantomData<C>,
}

/// The interned ingredient hashes values of type `C::Fields` to produce an `Id`.
///
/// It used to store interned structs but also to store the ID fields of a tracked struct.
/// Interned values are garbage collected and their memory reused based on an LRU heuristic.
pub struct IngredientImpl<C: Configuration> {
    /// Index of this ingredient in the database (used to construct database-IDs, etc).
    ingredient_index: IngredientIndex,

    /// A hasher for the sharded ID maps.
    hasher: FxBuildHasher,

    /// A shift used to determine the shard for a given hash.
    shift: u32,

    /// Sharded data that can only be accessed through a lock.
    shards: Box<[CachePadded<Mutex<IngredientShard>>]>,

    /// A queue of recent revisions in which values were interned.
    revision_queue: RevisionQueue,

    memo_table_types: Arc<MemoTableTypes>,

    _marker: PhantomData<fn() -> C>,
}

struct IngredientShard {
    /// Maps from data to the existing interned value for that data.
    ///
    /// The pointer is stable for the lifetime of the database. Storing it directly avoids a table
    /// lookup when hashing or comparing an existing entry.
    key_map: hashbrown::HashTable<ValueKey>,

    /// An intrusive linked list for LRU.
    lru: LinkedList<LruEntryAdapter>,
}

impl Default for IngredientShard {
    fn default() -> Self {
        Self {
            lru: LinkedList::default(),
            key_map: hashbrown::HashTable::new(),
        }
    }
}

// SAFETY: `IngredientShard` is only accessed through its mutex. Its LRU contains pointers to live,
// stable values from this ingredient, and those pointers and their links are accessed only while
// holding that mutex.
unsafe impl Send for IngredientShard {}

// SAFETY: `shard` is immutable. `lru` and `durability` are accessed only while holding the owning
// ingredient shard lock. `fields` is mutated only while holding that lock after stale-slot reuse
// guarantees no references remain, and is read only while holding the lock or after validation in
// the current revision. `memos` supports concurrent shared access, and is mutated only with
// exclusive access or after stale-slot reuse guarantees no shared references remain.
unsafe impl<C: Configuration> Sync for Value<C> {}

intrusive_adapter!(LruEntryAdapter = UnsafeRef<LruEntry>: LruEntry { link => LinkedListLink });

/// Struct storing the interned fields.
pub struct Value<C>
where
    C: Configuration,
{
    /// The index of the shard containing this value.
    ///
    /// This is immutable after construction and may be read to locate the protecting shard lock.
    shard: u16,

    /// Type-erased state used by the LRU scan.
    lru: LruEntry,

    /// The interned fields for this value.
    ///
    /// These are valid for read-only access as long as the lock is held
    /// or the value has been validated in the current revision.
    fields: UnsafeCell<C::Fields<'static>>,

    /// Memos attached to this interned value.
    ///
    /// This is valid for read-only access as long as the lock is held
    /// or the value has been validated in the current revision.
    memos: UnsafeCell<MemoTable>,

    /// The minimum durability of all inputs consumed by the creator query.
    ///
    /// Durability determines whether the value is added to the LRU list, but the LRU scan does
    /// not read it. Keeping this byte outside `LruEntry` lets it occupy padding in the outer value;
    /// adding it to the already aligned entry would increase the entry's size.
    /// This may only be accessed while holding the value's shard lock, or with exclusive access
    /// to the database.
    durability: UnsafeCell<Durability>,
}

/// Type-erased state stored in the intrusive LRU list.
struct LruEntry {
    /// Intrusive links, accessed only while holding the owning shard lock.
    link: LinkedListLink,

    /// Metadata used to decide whether and how this slot can be reused.
    ///
    /// This may only be accessed while holding the owning shard lock, or with exclusive access to
    /// the database.
    metadata: UnsafeCell<EntryMetadata>,
}

impl LruEntry {
    /// Returns a pointer to `value.lru` with provenance derived from `value`.
    #[inline]
    fn ptr_from_value<C: Configuration>(value: &Value<C>) -> *const Self {
        let value = std::ptr::from_ref(value).cast::<u8>();

        // SAFETY: `lru` is a field within `value`, so adding its offset is in bounds. Starting from
        // `value`, rather than `value.lru`, gives the result provenance that permits
        // `value_from_ptr` to subtract the offset again.
        unsafe { value.add(std::mem::offset_of!(Value<C>, lru)).cast() }
    }

    /// Recovers a value from its LRU entry.
    ///
    /// # Safety
    ///
    /// `entry` must have been produced by `Self::ptr_from_value` for the same configuration, and
    /// its value must remain live for the returned lifetime.
    #[inline]
    unsafe fn value_from_ptr<'a, C: Configuration>(entry: *const Self) -> &'a Value<C> {
        // SAFETY: `ptr_from_value` derives `entry` from the enclosing `Value<C>` pointer, so
        // subtracting the same field offset recovers that pointer. The other requirements are
        // guaranteed by the caller.
        unsafe {
            &*entry
                .cast::<u8>()
                .sub(std::mem::offset_of!(Value<C>, lru))
                .cast::<Value<C>>()
        }
    }
}

/// Metadata read by the type-erased LRU scan.
///
/// Durability is deliberately not stored here: it determines whether an entry is added to the
/// list, but is not needed once the scan begins. Keeping it on `Value` also lets Rust place the
/// byte in outer padding instead of growing the aligned `LruEntry`.
#[derive(Clone, Copy)]
struct EntryMetadata {
    /// The interned ID for this value.
    ///
    /// Storing this on the value itself is necessary to identify slots
    /// from the LRU list, as well as keep track of the generation.
    ///
    /// Values that are reused increment the ID generation, as if they had
    /// allocated a new slot. This eliminates the need for dependency edges
    /// on queries that *read* from an interned value, as any memos dependent
    /// on the previous value will not match the new ID.
    ///
    /// However, reusing a slot invalidates the previous ID, so queries that
    /// *create* a reusable interned value record dependency edges to ensure the
    /// value is re-interned with a new ID.
    id: Id,

    /// The revision the value was most-recently interned in.
    last_interned_at: Revision,
}

/// Returns `true` if a value slot with the given durability can be reused when interning.
///
/// This intentionally remains generic so the compiler specializes the immortal and reusable
/// configurations instead of branching on `REVISIONS` at runtime.
fn is_reusable<C: Configuration>(durability: Durability) -> bool {
    // Garbage collection is disabled.
    if C::REVISIONS == IMMORTAL {
        return false;
    }

    // Collecting higher durability values requires invalidating the revision for their
    // durability (see `Database::synthetic_write`, which requires a mutable reference to
    // the database) to avoid short-circuiting calls to `maybe_changed_after`. This is
    // necessary because `maybe_changed_after` for interned values is not "pure"; it updates
    // the `last_interned_at` field before validating a given value to ensure that it is not
    // reused after read in the current revision.
    durability == Durability::LOW
}

/// Record a dependency on this value if its slot can be reused.
fn report_tracked_read_if_reusable<C: Configuration>(
    zalsa_local: &ZalsaLocal,
    index: DatabaseKeyIndex,
    current_revision: Revision,
    durability: Durability,
) {
    if is_reusable::<C>(durability) {
        zalsa_local.report_tracked_read_simple(index, durability, current_revision);
    } else {
        // The value cannot be reused, so the dependency edge is unnecessary. Its durability
        // is derived from the active query, but its revision must still contribute to the
        // query's stamp because the interned ID may have changed due to reuse.
        zalsa_local.report_tracked_read_revision(current_revision);
    }
}

struct ReusableSlot {
    entry: *const LruEntry,
    old_id: Id,
    new_id: Id,
}

enum InternEvent {
    None,
    Validate,
    Intern,
    Reuse,
}

fn emit_intern_event(
    event: &InternEvent,
    zalsa: &Zalsa,
    ingredient: IngredientIndex,
    id: Id,
    revision: Revision,
) {
    match event {
        InternEvent::None => {}
        InternEvent::Validate => {
            let key = DatabaseKeyIndex::new(ingredient, id);
            zalsa.event(&|| Event::new(EventKind::DidValidateInternedValue { key, revision }));
        }
        InternEvent::Intern => {
            let key = DatabaseKeyIndex::new(ingredient, id);
            zalsa.event(&|| Event::new(EventKind::DidInternValue { key, revision }));
        }
        InternEvent::Reuse => {
            let key = DatabaseKeyIndex::new(ingredient, id);
            zalsa.event(&|| Event::new(EventKind::DidReuseInternedValue { key, revision }));
        }
    }
}

struct InternedMetadata {
    id: Id,
    revision: Revision,
    durability: Durability,
    event: InternEvent,
}

impl InternedMetadata {
    fn into_commit<'db, C: Configuration>(
        self,
        ingredient: &'db IngredientImpl<C>,
        retired: Option<RetiredInternedValue<'db, C>>,
    ) -> InternCommit<'db, C> {
        InternCommit {
            ingredient,
            id: self.id,
            revision: self.revision,
            durability: self.durability,
            event: self.event,
            retired,
        }
    }
}

struct RetiredInternedValue<'db, C: Configuration> {
    memos: DetachedMemoTable<'db>,
    _fields: C::Fields<'static>,
    old_id: Id,
}

/// A coherent key and its old generation's independent retirement ownership.
pub(crate) struct InternCommit<'db, C: Configuration> {
    ingredient: &'db IngredientImpl<C>,
    id: Id,
    revision: Revision,
    durability: Durability,
    event: InternEvent,
    retired: Option<RetiredInternedValue<'db, C>>,
}

/// Locked selection steps; no borrowed value may escape the inspection callback.
pub(crate) enum ReuseInspection<'a, 'db, C: Configuration> {
    /// The next LRU entry will be inspected; refusal stops before touching its metadata.
    Visit,
    /// No reusable generation is available, so committing would allocate fresh storage.
    Vacant,
    /// A stale generation is eligible for replacement. Hits do not invoke this variant.
    Selected {
        id: Id,
        fields: &'a C::Fields<'db>,
        memos: MemoTableWithTypesMut<'a>,
    },
}

pub(crate) struct PreparedIntern<'db, C: Configuration, Key> {
    pub(crate) commit: InternCommit<'db, C>,
    pub(crate) unused_input: Option<Key>,
}

/// Delivers coherent storage after the shard and unused closure captures are released.
///
/// Ordinary hits and allocations receive only metadata and unused input; only actual reuse
/// supplies ownership of a retired generation.
trait InternDelivery<'db, C: Configuration, Key> {
    type Output;

    fn live(
        ingredient: &'db IngredientImpl<C>,
        zalsa: &'db Zalsa,
        local: &'db ZalsaLocal,
        metadata: InternedMetadata,
        unused_input: Option<Key>,
    ) -> Self::Output;

    fn reused(
        ingredient: &'db IngredientImpl<C>,
        zalsa: &'db Zalsa,
        local: &'db ZalsaLocal,
        metadata: InternedMetadata,
        retired: RetiredInternedValue<'db, C>,
    ) -> Self::Output;
}

struct OrdinaryDelivery;

impl<'db, C: Configuration, Key> InternDelivery<'db, C, Key> for OrdinaryDelivery {
    type Output = Id;

    #[inline(always)]
    fn live(
        ingredient: &'db IngredientImpl<C>,
        zalsa: &'db Zalsa,
        local: &'db ZalsaLocal,
        metadata: InternedMetadata,
        unused_input: Option<Key>,
    ) -> Id {
        drop(unused_input);
        report_tracked_read_if_reusable::<C>(
            local,
            ingredient.database_key_index(metadata.id),
            metadata.revision,
            metadata.durability,
        );
        if !matches!(&metadata.event, InternEvent::None) {
            emit_intern_event(
                &metadata.event,
                zalsa,
                ingredient.ingredient_index,
                metadata.id,
                metadata.revision,
            );
        }
        metadata.id
    }

    fn reused(
        ingredient: &'db IngredientImpl<C>,
        zalsa: &'db Zalsa,
        local: &'db ZalsaLocal,
        metadata: InternedMetadata,
        retired: RetiredInternedValue<'db, C>,
    ) -> Id {
        metadata
            .into_commit(ingredient, Some(retired))
            .complete_ordinary(zalsa, local)
    }
}

struct PreparedDelivery;

impl<'db, C: Configuration, Key> InternDelivery<'db, C, Key> for PreparedDelivery {
    type Output = PreparedIntern<'db, C, Key>;

    fn live(
        ingredient: &'db IngredientImpl<C>,
        _zalsa: &'db Zalsa,
        _local: &'db ZalsaLocal,
        metadata: InternedMetadata,
        unused_input: Option<Key>,
    ) -> Self::Output {
        PreparedIntern {
            commit: metadata.into_commit(ingredient, None),
            unused_input,
        }
    }

    fn reused(
        ingredient: &'db IngredientImpl<C>,
        _zalsa: &'db Zalsa,
        _local: &'db ZalsaLocal,
        metadata: InternedMetadata,
        retired: RetiredInternedValue<'db, C>,
    ) -> Self::Output {
        PreparedIntern {
            commit: metadata.into_commit(ingredient, Some(retired)),
            unused_input: None,
        }
    }
}

impl<C: Configuration> InternCommit<'_, C> {
    pub(crate) fn retired_id(&self) -> Option<Id> {
        self.retired.as_ref().map(|retired| retired.old_id)
    }

    pub(crate) fn id(&self) -> Id {
        self.id
    }

    pub(crate) fn dependency_read(&self) -> DependencyRead<'_> {
        if is_reusable::<C>(self.durability) {
            DependencyRead::Simple {
                input: self.ingredient.database_key_index(self.id),
                durability: self.durability,
                changed_at: self.revision,
            }
        } else {
            DependencyRead::Revision(self.revision)
        }
    }

    pub(crate) fn report_read(&self, local: &ZalsaLocal) {
        report_tracked_read_if_reusable::<C>(
            local,
            self.ingredient.database_key_index(self.id),
            self.revision,
            self.durability,
        );
    }

    pub(crate) fn has_event(&self) -> bool {
        !matches!(self.event, InternEvent::None)
    }

    pub(crate) fn emit_event(&self, zalsa: &Zalsa) {
        emit_intern_event(
            &self.event,
            zalsa,
            self.ingredient.ingredient_index,
            self.id,
            self.revision,
        );
    }

    fn complete_ordinary(mut self, zalsa: &Zalsa, local: &ZalsaLocal) -> Id {
        self.report_read(local);
        if let Some(mut retired) = self.retired.take() {
            // SAFETY: Stale-slot replacement transferred this old table with its actual types.
            unsafe {
                self.ingredient
                    .clear_memos(zalsa, retired.memos.table_mut(), retired.old_id);
            }
            drop(retired);
        }
        self.emit_event(zalsa);
        self.id
    }
}

impl<C> Value<C>
where
    C: Configuration,
{
    /// Fields of this interned struct.
    #[cfg(feature = "salsa_unstable")]
    pub fn fields<'db>(&'db self) -> &'db C::Fields<'db> {
        // SAFETY: The fact that this function is safe is technically unsound. However, interned
        // values are only exposed if they have been validated in the current revision, which
        // ensures that they are not reused while being accessed.
        // SAFETY: Guaranteed by `Configuration`; the restored lifetime is
        // bounded by the borrow of this interned value.
        unsafe { std::mem::transmute::<&C::Fields<'static>, &C::Fields<'db>>(&*self.fields.get()) }
    }

    /// Returns memory usage information about the interned value.
    ///
    /// # Safety
    ///
    /// The `MemoTable` must belong to a `Value` of the correct type. Additionally, the
    /// lock must be held for the shard containing the value.
    #[cfg(all(not(feature = "shuttle"), feature = "salsa_unstable"))]
    unsafe fn memory_usage(&self, memo_table_types: &MemoTableTypes) -> crate::database::SlotInfo {
        let heap_size = C::heap_size(self.fields());
        // SAFETY: The caller guarantees we hold the lock for the shard containing the value, so we
        // have at-least read-only access to the value's memos.
        let memos = unsafe { &*self.memos.get() };
        // SAFETY: The caller guarantees this is the correct types table.
        let memos = unsafe { memo_table_types.attach_memos(memos) };

        crate::database::SlotInfo {
            debug_name: C::DEBUG_NAME,
            size_of_metadata: std::mem::size_of::<Self>()
                - std::mem::size_of::<C::Fields<'static>>(),
            size_of_fields: std::mem::size_of::<C::Fields<'static>>(),
            heap_size_of_fields: heap_size,
            memos: memos.memory_usage(),
        }
    }
}

impl<C: Configuration> Default for JarImpl<C> {
    fn default() -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<C: Configuration> Jar for JarImpl<C> {
    fn create_ingredients(
        _zalsa: &mut Zalsa,
        first_index: IngredientIndex,
    ) -> Vec<Box<dyn Ingredient>> {
        vec![Box::new(IngredientImpl::<C>::new(first_index)) as _]
    }

    fn id_struct_type_id() -> TypeId {
        TypeId::of::<C::Struct<'static>>()
    }
}

impl<C> IngredientImpl<C>
where
    C: Configuration,
{
    pub fn new(ingredient_index: IngredientIndex) -> Self {
        let shards = new_shards();
        let shift = usize::BITS - shards.len().trailing_zeros();

        Self {
            ingredient_index,
            hasher: FxBuildHasher,
            memo_table_types: Arc::new(MemoTableTypes::default()),
            revision_queue: RevisionQueue::new(C::REVISIONS),
            shift,
            shards,
            _marker: PhantomData,
        }
    }
    /// Returns the shard for a given hash.
    ///
    /// Note that this value is guaranteed to be in-bounds for `self.shards`.
    #[inline]
    fn shard(&self, hash: u64) -> usize {
        // https://github.com/xacrimon/dashmap/blob/366ce7e7872866a06de66eb95002fa6cf2c117a7/src/lib.rs#L421
        ((hash as usize) << 7) >> self.shift
    }

    /// # Safety
    ///
    /// The `from_internal_data` function must be called to restore the correct lifetime
    /// before access.
    unsafe fn to_internal_data<'db>(&'db self, data: C::Fields<'db>) -> C::Fields<'static> {
        // SAFETY: Guaranteed by `Configuration` and retained in this ingredient.
        unsafe { std::mem::transmute::<C::Fields<'db>, C::Fields<'static>>(data) }
    }

    fn from_internal_data<'db>(data: &'db C::Fields<'static>) -> &'db C::Fields<'db> {
        // SAFETY: Guaranteed by `Configuration`; the restored lifetime is
        // bounded by the borrow of the retained data.
        unsafe { std::mem::transmute::<&'db C::Fields<'static>, &'db C::Fields<'db>>(data) }
    }

    /// Intern data to a unique reference.
    ///
    /// If `key` is already interned, returns the existing [`Id`] for the interned data without
    /// invoking `assemble`.
    ///
    /// Otherwise, invokes `assemble` with the given `key` and the [`Id`] to be allocated for this
    /// interned value. The resulting [`C::Data`] will then be interned.
    ///
    /// Note: Using the database within the `assemble` function may result in a deadlock if
    /// the database ends up trying to intern or allocate a new value.
    pub fn intern<'db, Key>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        key: Key,
        assemble: impl FnOnce(Id, Key) -> C::Fields<'db>,
    ) -> C::Struct<'db>
    where
        Key: Hash,
        C::Fields<'db>: HashEqLike<Key>,
    {
        FromId::from_id(self.intern_id(zalsa, zalsa_local, key, assemble))
    }

    /// Intern data to a unique reference.
    ///
    /// If `key` is already interned, returns the existing [`Id`] for the interned data without
    /// invoking `assemble`.
    ///
    /// Otherwise, invokes `assemble` with the given `key` and the [`Id`] to be allocated for this
    /// interned value. The resulting [`C::Data`] will then be interned.
    ///
    /// Note: Using the database within the `assemble` function may result in a deadlock if
    /// the database ends up trying to intern or allocate a new value.
    pub fn intern_id<'db, Key>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        key: Key,
        assemble: impl FnOnce(Id, Key) -> C::Fields<'db>,
    ) -> crate::Id
    where
        Key: Hash,
        // We'd want the following predicate, but this currently implies `'static` due to a rustc
        // bug
        // for<'db> C::Data<'db>: HashEqLike<Key>,
        // so instead we go with this and transmute the lifetime in the `eq` closure
        C::Fields<'db>: HashEqLike<Key>,
    {
        assert_ordinary_execution_allowed((zalsa, zalsa_local).into(), "interned identity");
        match self.intern_with_delivery::<Key, Infallible, OrdinaryDelivery>(
            zalsa,
            zalsa_local,
            key,
            assemble,
            |_| Ok(()),
        ) {
            Ok(id) => id,
            Err((never, _input)) => match never {},
        }
    }

    /// Commits canonical storage without delivering events or dropping the previous generation.
    pub(crate) fn prepare_intern<'db, Key, E>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        key: Key,
        assemble: impl FnOnce(Id, Key) -> C::Fields<'db>,
        check_reuse: impl FnOnce(MemoTableWithTypesMut<'_>) -> Result<(), E>,
    ) -> Result<InternCommit<'db, C>, E>
    where
        Key: Hash,
        C::Fields<'db>: HashEqLike<Key>,
    {
        match self.prepare_intern_retaining(zalsa, zalsa_local, key, assemble, check_reuse) {
            Ok(PreparedIntern {
                commit,
                unused_input,
            }) => {
                drop(unused_input);
                Ok(commit)
            }
            Err((error, input)) => {
                drop(input);
                Err(error)
            }
        }
    }

    /// Interns a key, returning its committed storage and any unused input.
    /// Reuse inspection runs before ownership transfer; rejection returns the submitted input.
    pub(crate) fn prepare_intern_retaining<'db, Key, E>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        key: Key,
        assemble: impl FnOnce(Id, Key) -> C::Fields<'db>,
        check_reuse: impl FnOnce(MemoTableWithTypesMut<'_>) -> Result<(), E>,
    ) -> Result<PreparedIntern<'db, C, Key>, (E, Key)>
    where
        Key: Hash,
        C::Fields<'db>: HashEqLike<Key>,
    {
        let mut check_reuse = Some(check_reuse);
        self.prepare_intern_checked(zalsa, zalsa_local, key, assemble, |step| {
            if let ReuseInspection::Selected { memos, .. } = step {
                if let Some(check) = check_reuse.take() {
                    check(memos)?;
                }
            }
            Ok(())
        })
    }

    #[cfg(test)]
    pub(crate) fn exhaust_generation_for_tests(&self, zalsa: &Zalsa, id: Id) -> Option<Id> {
        self.revision_queue.record(zalsa.current_revision());
        for shard in &self.shards {
            let shard = shard.lock();
            for key in shard.key_map.iter() {
                // SAFETY: This shard lock protects its live values and metadata.
                let value = unsafe { key.value::<C>() };
                let metadata = unsafe { &mut *value.lru.metadata.get() };
                if metadata.id == id && self.revision_queue.is_stale(metadata.last_interned_at) {
                    let exhausted = id.with_generation(u32::MAX);
                    metadata.id = exhausted;
                    return Some(exhausted);
                }
            }
        }
        None
    }

    /// Selects and inspects old storage without transferring fields or memo ownership.
    pub(crate) fn inspect_intern<'db, Key, E>(
        &'db self,
        zalsa: &'db Zalsa,
        key: &Key,
        mut inspect: impl FnMut(ReuseInspection<'_, 'db, C>) -> Result<(), E>,
    ) -> Result<(), E>
    where
        Key: Hash,
        C::Fields<'db>: HashEqLike<Key>,
    {
        let current_revision = zalsa.current_revision();
        if C::REVISIONS != IMMORTAL {
            self.revision_queue.record(current_revision);
        }
        let hash = self.hasher.hash_one(key);
        let mut shard = self.shards[self.shard(hash)].lock();
        // SAFETY: The shard lock protects the fields inspected by equality.
        let eq = |value: &ValueKey| unsafe { Self::value_eq(value.value::<C>(), key) };
        if shard.key_map.find(hash, eq).is_some() {
            return Ok(());
        }
        if !self.revision_queue.is_primed() {
            return inspect(ReuseInspection::Vacant);
        }
        // SAFETY: The current revision and locked shard establish the selector's requirements.
        let selected = unsafe {
            self.find_reusable_slot(current_revision, &mut shard, || {
                inspect(ReuseInspection::Visit)
            })
        }?;
        if let Some((slot, value)) = selected {
            // SAFETY: Only stale reusable slots are returned. The lock excludes replacement;
            // stale eligibility excludes outstanding field and memo borrows.
            let fields = unsafe { Self::from_internal_data(&*value.fields.get()) };
            let memos = unsafe {
                self.memo_table_types
                    .attach_memos_mut(&mut *value.memos.get())
            };
            inspect(ReuseInspection::Selected {
                id: slot.old_id,
                fields,
                memos,
            })?;
        } else {
            inspect(ReuseInspection::Vacant)?;
        }
        Ok(())
    }

    /// Interns a key after inspecting its current reusable generation under the shard lock.
    /// Rejection returns the submitted input without detaching storage; success returns the
    /// committed key and any unused input, leaving events and old-generation cleanup to the caller.
    pub(crate) fn prepare_intern_checked<'db, Key, E>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        key: Key,
        assemble: impl FnOnce(Id, Key) -> C::Fields<'db>,
        inspect: impl FnMut(ReuseInspection<'_, 'db, C>) -> Result<(), E>,
    ) -> Result<PreparedIntern<'db, C, Key>, (E, Key)>
    where
        Key: Hash,
        C::Fields<'db>: HashEqLike<Key>,
    {
        self.intern_with_delivery::<Key, E, PreparedDelivery>(
            zalsa,
            zalsa_local,
            key,
            assemble,
            inspect,
        )
    }

    fn intern_with_delivery<'db, Key, E, D>(
        &'db self,
        zalsa: &'db Zalsa,
        zalsa_local: &'db ZalsaLocal,
        key: Key,
        assemble: impl FnOnce(Id, Key) -> C::Fields<'db>,
        mut check_reuse: impl FnMut(ReuseInspection<'_, 'db, C>) -> Result<(), E>,
    ) -> Result<D::Output, (E, Key)>
    where
        Key: Hash,
        C::Fields<'db>: HashEqLike<Key>,
        D: InternDelivery<'db, C, Key>,
    {
        // Record the current revision as active.
        let current_revision = zalsa.current_revision();
        if C::REVISIONS != IMMORTAL {
            self.revision_queue.record(current_revision);
        }

        // Hash the value before acquiring the lock.
        let hash = self.hasher.hash_one(&key);

        let shard_index = self.shard(hash);
        // SAFETY: `shard_index` is guaranteed to be in-bounds for `self.shards`.
        let mut shard_guard = unsafe { self.shards.get_unchecked(shard_index) }.lock();
        let shard = &mut *shard_guard;

        // SAFETY: We hold the lock for the shard containing the value.
        let eq = |value: &ValueKey| unsafe { Self::value_eq(value.value::<C>(), &key) };

        // Attempt a fast-path lookup of already interned data.
        if let Some(value) = shard.key_map.find(hash, eq) {
            // SAFETY: Values remain allocated for the lifetime of the database.
            let value = unsafe { value.value::<C>() };

            // SAFETY: We hold the lock for the shard containing the value, giving us exclusive
            // access to its entry metadata and durability.
            let (metadata, durability) =
                unsafe { (&mut *value.lru.metadata.get(), &mut *value.durability.get()) };
            let mut event = InternEvent::None;

            // Validate the value in this revision to avoid reuse.
            if metadata.last_interned_at < current_revision {
                metadata.last_interned_at = current_revision;

                event = InternEvent::Validate;

                if is_reusable::<C>(*durability) {
                    // Move the value to the front of the LRU list.
                    //
                    // SAFETY: We hold the lock for the shard containing the value, and `value` is
                    // a reusable value that was previously interned, so is in the list.
                    unsafe { shard.lru.cursor_mut_from_ptr(&value.lru).remove() };

                    // SAFETY: The value pointer is valid for the lifetime of the database
                    // and never accessed mutably directly.
                    unsafe {
                        shard
                            .lru
                            .push_front(UnsafeRef::from_raw(LruEntry::ptr_from_value(value)))
                    };
                }
            }

            if let Some((_, stamp)) = zalsa_local.active_query() {
                let was_reusable = is_reusable::<C>(*durability);

                // Record the maximum durability across all queries that intern this value.
                *durability = std::cmp::max(*durability, stamp.durability);

                // If the value is no longer reusable, i.e. the durability increased, remove it
                // from the LRU.
                if was_reusable && !is_reusable::<C>(*durability) {
                    // SAFETY: We hold the lock for the shard containing the value, and `value`
                    // was previously reusable, so is in the list.
                    unsafe { shard.lru.cursor_mut_from_ptr(&value.lru).remove() };
                }
            }

            let metadata = InternedMetadata {
                id: metadata.id,
                revision: current_revision,
                durability: *durability,
                event,
            };
            // Drop unused captures before the input, after releasing the shard lock. Keeping
            // the input in a local also releases it if a capture's destructor panics.
            let unused_input = Some(key);
            let unused_closures = (check_reuse, assemble);
            drop(shard_guard);
            drop(unused_closures);
            return Ok(D::live(self, zalsa, zalsa_local, metadata, unused_input));
        }

        // Fill up the table for the first few revisions without attempting garbage collection.
        if !self.revision_queue.is_primed() {
            if let Err(error) = check_reuse(ReuseInspection::Vacant) {
                drop(shard_guard);
                drop(assemble);
                return Err((error, key));
            }
            let metadata =
                self.intern_id_cold(key, zalsa, zalsa_local, assemble, shard, shard_index, hash);
            drop(shard_guard);
            drop(check_reuse);
            return Ok(D::live(self, zalsa, zalsa_local, metadata, None));
        }

        // Otherwise, try to reuse a stale slot.
        // SAFETY: We hold the lock for this ingredient's shard, and `current_revision` is the
        // database's current revision.
        let selected = unsafe {
            self.find_reusable_slot(current_revision, shard, || {
                check_reuse(ReuseInspection::Visit)
            })
        };
        let selected = match selected {
            Ok(selected) => selected,
            Err(error) => {
                drop(shard_guard);
                drop(assemble);
                return Err((error, key));
            }
        };
        let Some((slot, value)) = selected else {
            // If we could not find a stale slot, we are forced to allocate a new one.
            if let Err(error) = check_reuse(ReuseInspection::Vacant) {
                drop(shard_guard);
                drop(assemble);
                return Err((error, key));
            }
            let metadata =
                self.intern_id_cold(key, zalsa, zalsa_local, assemble, shard, shard_index, hash);
            drop(shard_guard);
            drop(check_reuse);
            return Ok(D::live(self, zalsa, zalsa_local, metadata, None));
        };

        // SAFETY: The selected slot is stale and belongs to this ingredient. The shard grants
        // exclusive access to its old table, which was created with these exact memo types.
        let memo_table = unsafe { &mut *value.memos.get() };
        if let Err(error) = check_reuse(ReuseInspection::Selected {
            id: slot.old_id,
            // SAFETY: This selected generation is stale and the shard remains locked.
            fields: unsafe { Self::from_internal_data(&*value.fields.get()) },
            memos: unsafe { self.memo_table_types.attach_memos_mut(memo_table) },
        }) {
            let rejected = (error, key);
            drop(shard_guard);
            drop(assemble);
            return Err(rejected);
        }

        // Record the durability of the current query on the interned value.
        let (durability, last_interned_at) = zalsa_local
            .active_query()
            .map(|(_, stamp)| (stamp.durability, current_revision))
            // If there is no active query this durability does not actually matter.
            // `last_interned_at` needs to be `Revision::MAX`, see the `intern_access_in_different_revision` test.
            .unwrap_or((Durability::MAX, Revision::max()));

        // Assemble and hash the replacement before mutating the existing slot. Both operations
        // can invoke user code and panic.
        // SAFETY: We call `from_internal_data` to restore the correct lifetime before access.
        let new_fields = unsafe { self.to_internal_data(assemble(slot.new_id, key)) };

        // SAFETY: We hold the lock for the shard containing the value.
        let old_hash = self.hasher.hash_one(unsafe { &*value.fields.get() });

        // Reserve space while the old slot is still intact. Resizing can invoke user hashing, so
        // doing it before mutating the slot leaves the old value reachable if that panics. Once
        // capacity is available, replacing the pointer entry cannot invoke user code.
        // SAFETY: We hold the lock for the shard containing every value passed to `hasher`.
        let hasher = |value: &ValueKey| unsafe { self.value_hash(value.value::<C>()) };
        shard.key_map.reserve(1, hasher);

        // Remove the value from the LRU list.
        // SAFETY: We hold the shard lock and `value` is currently in the LRU.
        unsafe { shard.lru.cursor_mut_from_ptr(&value.lru).remove() };

        // Remove the previous value from the key map.
        //
        // The pointer stays the same when a slot is reused, but the fields and thus the hash can
        // change, so we need to re-insert it. The old and new hashes map to the same shard, because
        // we selected the LRU list based on the new fields.
        let value_key = ValueKey::new(value);
        shard
            .key_map
            .find_entry(old_hash, |found_value| found_value.0 == value_key.0)
            .unwrap_or_else(|_| panic!("interned value in LRU so must be in key_map"))
            .remove();

        // SAFETY: The stale-slot proof above excludes old memo references. Reset the live table
        // before publishing the new generation; subsequent callbacks may populate it again.
        let old_memos = unsafe { DetachedMemoTable::replace(memo_table, &self.memo_table_types) };

        // Replace the fields without dropping the previous value until the slot is consistent.
        //
        // SAFETY: `find_reusable_slot` guarantees that the value is reusable and stale, so no
        // references to its fields remain. We still hold the shard lock.
        let old_fields = unsafe { std::mem::replace(&mut *value.fields.get(), new_fields) };

        // Mark the slot as reused.
        // SAFETY: We still hold the lock for the shard containing the value, giving us exclusive
        // access to its entry metadata and durability.
        unsafe {
            *value.lru.metadata.get() = EntryMetadata {
                id: slot.new_id,
                last_interned_at,
            };
            *value.durability.get() = durability;
        }

        shard.key_map.insert_unique(hash, value_key, hasher);

        if is_reusable::<C>(durability) {
            // Move the value to the front of the LRU list.
            //
            // SAFETY: The value pointer is valid for the lifetime of the database
            // and is never accessed mutably directly.
            unsafe {
                shard
                    .lru
                    .push_front(UnsafeRef::from_raw(LruEntry::ptr_from_value(value)))
            };
        }

        let metadata = InternedMetadata {
            id: slot.new_id,
            revision: current_revision,
            durability,
            event: InternEvent::Reuse,
        };
        let retired = RetiredInternedValue {
            memos: old_memos,
            _fields: old_fields,
            old_id: slot.old_id,
        };
        drop(shard_guard);
        Ok(D::reused(self, zalsa, zalsa_local, metadata, retired))
    }

    /// The cold path for interning a value, allocating a new slot.
    ///
    #[allow(clippy::too_many_arguments)]
    fn intern_id_cold<'db, Key>(
        &'db self,
        key: Key,
        zalsa: &Zalsa,
        zalsa_local: &ZalsaLocal,
        assemble: impl FnOnce(Id, Key) -> C::Fields<'db>,
        shard: &mut IngredientShard,
        shard_index: usize,
        hash: u64,
    ) -> InternedMetadata
    where
        Key: Hash,
        C::Fields<'db>: HashEqLike<Key>,
    {
        let current_revision = zalsa.current_revision();

        // Record the durability of the current query on the interned value.
        let (durability, last_interned_at) = zalsa_local
            .active_query()
            .map(|(_, stamp)| (stamp.durability, current_revision))
            // If there is no active query this durability does not actually matter.
            // `last_interned_at` needs to be `Revision::MAX`, see the `intern_access_in_different_revision` test.
            .unwrap_or((Durability::MAX, Revision::max()));

        // Allocate the value slot.
        let (id, value) = zalsa_local.allocate(zalsa, self.ingredient_index, |id| Value::<C> {
            shard: shard_index as u16,
            lru: LruEntry {
                link: LinkedListLink::new(),
                metadata: UnsafeCell::new(EntryMetadata {
                    id,
                    last_interned_at,
                }),
            },
            // SAFETY: We call `from_internal_data` to restore the correct lifetime before access.
            fields: UnsafeCell::new(unsafe { self.to_internal_data(assemble(id, key)) }),
            // SAFETY: We only ever access the memos of a value that we allocated through
            // our `MemoTableTypes`.
            memos: UnsafeCell::new(unsafe { MemoTable::new(self.memo_table_types()) }),
            durability: UnsafeCell::new(durability),
        });

        // Insert the newly allocated value.
        // SAFETY: We hold this ingredient's shard lock, and `value` is the live, stable value that
        // we just allocated in that shard.
        unsafe { self.insert_value(shard, hash, value) };

        // Note that the ID is unique to this use of the interned slot, so it seems logical to use
        // `Revision::start()` here. However, it is possible that the ID we read is different from
        // the previous execution of this query if the previous slot has been reused. In that case,
        // the query has changed without a corresponding input changing. Using `current_revision`
        // for dependencies on interned values encodes the fact that interned IDs are not stable
        // across revisions.
        InternedMetadata {
            id,
            revision: current_revision,
            durability,
            event: InternEvent::Intern,
        }
    }

    /// Inserts a newly interned value into the LRU list and key map.
    ///
    /// # Safety
    ///
    /// `shard` must be a locked shard from this ingredient, and `value` must be the live, stable
    /// `Value<C>` allocated in that shard.
    unsafe fn insert_value(&self, shard: &mut IngredientShard, hash: u64, value: &Value<C>) {
        /// Inserts a newly allocated value without depending on `C`.
        ///
        /// # Safety
        ///
        /// The caller must hold `shard`'s lock. `value_key` and `entry` must have been produced
        /// from the same live value in this shard,
        /// `reusable` must reflect whether that value is reusable, and `hasher` must hash values
        /// while that lock is held.
        unsafe fn inner(
            shard: &mut IngredientShard,
            hash: u64,
            value_key: ValueKey,
            entry: *const LruEntry,
            reusable: bool,
            hasher: &dyn Fn(&ValueKey) -> u64,
        ) {
            if reusable {
                // SAFETY: The caller guarantees that `entry` points to a live `LruEntry` and was
                // derived from its enclosing value.
                unsafe { shard.lru.push_front(UnsafeRef::from_raw(entry)) };
            }

            insert_unique_erased(shard, hash, value_key, hasher);

            debug_assert_eq!(hash, hasher(&value_key));
        }

        // SAFETY: We hold the lock for the shard containing the value.
        let durability = unsafe { *value.durability.get() };
        let reusable = is_reusable::<C>(durability);
        let value_key = ValueKey::new(value);

        // SAFETY: We hold the lock for the shard containing every value passed to `hasher`.
        let hasher = |value: &ValueKey| unsafe { self.value_hash(value.value::<C>()) };

        // SAFETY: We hold the lock for `shard`; `value_key` and `ptr_from_value` were derived from
        // the same live value in that shard, and `reusable` was read from that value.
        unsafe {
            inner(
                shard,
                hash,
                value_key,
                LruEntry::ptr_from_value(value),
                reusable,
                &hasher,
            )
        };
    }

    /// Finds a reusable slot and reconstructs its typed value.
    ///
    /// # Safety
    ///
    /// `shard` must be a locked shard from this ingredient, and `current_revision` must be the
    /// database's current revision. Every LRU entry pointer must have been derived from its live
    /// enclosing `Value<C>` using `LruEntry::ptr_from_value`, so subtracting the field offset
    /// recovers that value. Those values must remain stable for the ingredient's lifetime.
    ///
    /// The LRU contains only reusable values. Therefore, a slot returned as stale has no
    /// outstanding references to its fields or memos and may be mutated while the lock is held.
    unsafe fn find_reusable_slot<E>(
        &self,
        current_revision: Revision,
        shard: &mut IngredientShard,
        mut visit: impl FnMut() -> Result<(), E>,
    ) -> Result<Option<(ReusableSlot, &Value<C>)>, E> {
        /// Finds the least-recently-used stale slot whose generation can be incremented.
        ///
        /// # Safety
        ///
        /// The caller must hold `shard`'s lock, and every pointer in its LRU must refer to a live
        /// `LruEntry` belonging to the shard.
        unsafe fn inner<E>(
            revision_queue: &RevisionQueue,
            current_revision: Revision,
            shard: &mut IngredientShard,
            visit: &mut impl FnMut() -> Result<(), E>,
        ) -> Result<Option<ReusableSlot>, E> {
            let mut cursor = shard.lru.back_mut();

            while let Some(entry) = cursor.as_cursor().clone_pointer() {
                visit()?;
                let entry = UnsafeRef::into_raw(entry);

                // SAFETY: The caller guarantees that `entry` points to a live value in this shard
                // and that we hold the shard lock, which grants exclusive access to `metadata`.
                let metadata = unsafe { &mut *(*entry).metadata.get() };

                // The value must not have been read in the current revision to be collected
                // soundly, but we also do not want to collect values that have been read recently.
                //
                // Note that the list is sorted by LRU, so if the tail of the list is not stale, we
                // will not find any stale slots.
                if !revision_queue.is_stale(metadata.last_interned_at) {
                    return Ok(None);
                }

                // We should never reuse a value that was accessed in the current revision.
                debug_assert!(metadata.last_interned_at < current_revision);

                let old_id = metadata.id;

                // Increment the generation of the ID, as if we allocated a new slot.
                //
                // If the ID is at its maximum generation, we are forced to leak the slot.
                if let Some(new_id) = old_id.next_generation() {
                    return Ok(Some(ReusableSlot {
                        entry,
                        old_id,
                        new_id,
                    }));
                }

                // This slot can never be reused. Remove it and retry with the previous element.
                cursor.remove().unwrap();
                cursor = shard.lru.back_mut();
            }

            Ok(None)
        }

        // SAFETY: Guaranteed by the caller.
        let Some(slot) =
            (unsafe { inner(&self.revision_queue, current_revision, shard, &mut visit) })?
        else {
            return Ok(None);
        };

        // SAFETY: The caller guarantees that this is a shard from `self`. Its LRU contains only
        // entries produced by `LruEntry::ptr_from_value` for stable values allocated by this
        // ingredient.
        let value = unsafe { LruEntry::value_from_ptr::<C>(slot.entry) };

        Ok(Some((slot, value)))
    }

    /// Clears the given memo table.
    ///
    /// # Safety
    ///
    /// `memo_table` must belong to the value identified by `id` in this ingredient. The caller
    /// must have exclusive access to the table.
    pub(crate) unsafe fn clear_memos(&self, zalsa: &Zalsa, memo_table: &mut MemoTable, id: Id) {
        /// Clears the given memo table without depending on `C`.
        ///
        /// # Safety
        ///
        /// `memo_table` must belong to the value identified by `id` in `ingredient_index` and must
        /// have been created with `memo_table_types`. The caller must have exclusive access to the
        /// table.
        unsafe fn inner(
            zalsa: &Zalsa,
            ingredient_index: IngredientIndex,
            memo_table_types: &MemoTableTypes,
            memo_table: &mut MemoTable,
            id: Id,
        ) {
            // SAFETY: The caller guarantees this is the correct types table.
            let table = unsafe { memo_table_types.attach_memos_mut(memo_table) };

            // `Database::salsa_event` is a user supplied callback which may panic
            // in that case we need a drop guard to free the memo table
            struct TableDropGuard<'a>(MemoTableWithTypesMut<'a>);

            impl Drop for TableDropGuard<'_> {
                fn drop(&mut self) {
                    // SAFETY: We have `&mut MemoTable`, so no more references to these memos exist and we are good
                    // to drop them.
                    unsafe { self.0.drop() };
                }
            }

            let mut table_guard = TableDropGuard(table);

            // SAFETY: We have `&mut MemoTable`, so no more references to these memos exist and we are good
            // to drop them.
            unsafe {
                table_guard.0.take_memos(|memo_ingredient_index, memo| {
                    let ingredient_index =
                        zalsa.ingredient_index_for_memo(ingredient_index, memo_ingredient_index);

                    let executor = DatabaseKeyIndex::new(ingredient_index, id);

                    zalsa.event(&|| Event::new(EventKind::DidDiscard { key: executor }));

                    memo.remove_outputs(zalsa, executor);
                })
            };

            std::mem::forget(table_guard);

            // Reset the table after having dropped any memos.
            memo_table.reset();
        }

        // SAFETY: The caller guarantees the table belongs to this ingredient's value, so these are
        // the correct ingredient index and memo-table types.
        unsafe {
            inner(
                zalsa,
                self.ingredient_index,
                &self.memo_table_types,
                memo_table,
                id,
            )
        }
    }

    // Hashes the value by its fields.
    //
    // # Safety
    //
    // The lock must be held for the shard containing the value.
    unsafe fn value_hash(&self, value: &Value<C>) -> u64 {
        // SAFETY: We hold the lock for the shard containing the value.
        unsafe { self.hasher.hash_one(&*value.fields.get()) }
    }

    // Compares the value by its fields to the given key.
    //
    // # Safety
    //
    // The lock must be held for the shard containing the value.
    unsafe fn value_eq<'db, Key>(value: &'db Value<C>, key: &Key) -> bool
    where
        C::Fields<'db>: HashEqLike<Key>,
    {
        // SAFETY: We hold the lock for the shard containing the value.
        let fields = unsafe { &*value.fields.get() };

        HashEqLike::eq(Self::from_internal_data(fields), key)
    }

    /// Returns the database key index for an interned value with the given id.
    #[inline]
    pub fn database_key_index(&self, id: Id) -> DatabaseKeyIndex {
        DatabaseKeyIndex::new(self.ingredient_index, id)
    }

    /// Lookup the data for an interned value based on its ID.
    pub fn data<'db>(&'db self, zalsa: &'db Zalsa, id: Id) -> &'db C::Fields<'db> {
        let value = zalsa.table().get::<Value<C>>(id);

        debug_assert!(
            {
                let _shard = self.shards[value.shard as usize].lock();

                // SAFETY: We hold the lock for the shard containing the value, giving us shared
                // access to its durability.
                let durability = unsafe { *value.durability.get() };

                !is_reusable::<C>(durability) || {
                    // SAFETY: We hold the lock for the shard containing the value, giving us shared
                    // access to its entry metadata.
                    let last_interned_at = unsafe { (*value.lru.metadata.get()).last_interned_at };

                    let last_changed_revision = zalsa.last_changed_revision(durability);
                    last_interned_at >= last_changed_revision
                }
            },
            "Data for reusable `{database_key:?}` was not interned in the latest revision for its durability.",
            database_key = self.database_key_index(id),
        );

        // SAFETY: Reusable interned values are only exposed if they have been validated
        // in the current revision, as checked by the assertion above, which ensures that
        // they are not reused while being accessed. Non-reusable values are never reused.
        unsafe { Self::from_internal_data(&*value.fields.get()) }
    }

    /// Lookup the fields from an interned struct.
    ///
    /// Note that this is not "leaking" since no dependency edge is required.
    pub fn fields<'db>(&'db self, zalsa: &'db Zalsa, s: C::Struct<'db>) -> &'db C::Fields<'db> {
        assert_ordinary_execution_allowed(OrdinaryAccess::Storage(zalsa), "interned field");
        self.select_fields(zalsa, s)
    }

    pub(crate) fn select_fields<'db>(
        &'db self,
        zalsa: &'db Zalsa,
        s: C::Struct<'db>,
    ) -> &'db C::Fields<'db> {
        self.data(zalsa, AsId::as_id(&s))
    }

    pub fn reset(&mut self, zalsa_mut: &mut Zalsa) {
        _ = zalsa_mut;

        for shard in self.shards.iter_mut() {
            // We can clear the key maps now that we have cancelled all other handles.
            shard.get_mut().key_map.clear();
        }
    }

    /// Returns all data corresponding to the interned struct.
    pub fn entries<'db>(&'db self, zalsa: &'db Zalsa) -> impl Iterator<Item = StructEntry<'db, C>> {
        // SAFETY: `should_lock` is `true`
        unsafe { self.entries_inner(true, zalsa) }
    }

    /// Returns all data corresponding to the interned struct.
    ///
    /// # Safety
    ///
    /// If `should_lock` is `false`, the caller *must* hold the locks for all shards
    /// of the key map.
    unsafe fn entries_inner<'db>(
        &'db self,
        should_lock: bool,
        zalsa: &'db Zalsa,
    ) -> impl Iterator<Item = StructEntry<'db, C>> {
        // TODO: Grab all locks eagerly.
        zalsa.table().slots_of::<Value<C>>().map(move |(_, value)| {
            let id = if should_lock {
                let _shard =
                    // SAFETY: `value.shard` is guaranteed to be in-bounds for `self.shards`.
                    unsafe { self.shards.get_unchecked(value.shard as usize) }.lock();

                // SAFETY: We hold the lock for the shard containing the value.
                unsafe { (*value.lru.metadata.get()).id }
            } else {
                // SAFETY: The caller guarantees we hold the lock for the shard containing the value.
                unsafe { (*value.lru.metadata.get()).id }
            };

            StructEntry {
                value,
                key: self.database_key_index(id),
            }
        })
    }
}

/// Creates the sharded storage outside of the generic [`IngredientImpl::new`] context.
///
/// Keeping this helper non-generic avoids monomorphizing the `OnceLock` and iterator machinery for
/// every interned struct configuration.
fn new_shards() -> Box<[CachePadded<Mutex<IngredientShard>>]> {
    static SHARDS: OnceLock<usize> = OnceLock::new();
    let shards = *SHARDS.get_or_init(|| {
        let num_cpus = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1);

        (num_cpus * 4).next_power_of_two()
    });

    (0..shards).map(|_| Default::default()).collect()
}

/// A stable pointer to an interned value allocated in the database table.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct ValueKey(NonNull<()>);

impl ValueKey {
    fn new<C: Configuration>(value: &Value<C>) -> Self {
        Self(NonNull::from(value).cast())
    }

    /// # Safety
    ///
    /// The database table containing this value must still be alive.
    unsafe fn value<'db, C: Configuration>(&self) -> &'db Value<C> {
        // SAFETY: Guaranteed by the caller. The erased pointer was created from a `Value<C>`.
        unsafe { &*self.0.as_ptr().cast::<Value<C>>() }
    }
}

// SAFETY: Values remain allocated until after the ingredient and its key map are dropped.
// Access to their mutable state is synchronized by the corresponding shard lock.
unsafe impl Send for ValueKey {}
// SAFETY: See the `Send` implementation above.
unsafe impl Sync for ValueKey {}

/// Inserts a value pointer while keeping the hasher and rehashing logic independent of `C`.
fn insert_unique_erased(
    shard: &mut IngredientShard,
    hash: u64,
    value: ValueKey,
    hasher: &dyn Fn(&ValueKey) -> u64,
) {
    shard.key_map.insert_unique(hash, value, hasher);
}

/// An interned struct entry.
pub struct StructEntry<'db, C>
where
    C: Configuration,
{
    #[allow(dead_code)]
    value: &'db Value<C>,
    key: DatabaseKeyIndex,
}

impl<'db, C> StructEntry<'db, C>
where
    C: Configuration,
{
    /// Returns the `DatabaseKeyIndex` for this entry.
    pub fn key(&self) -> DatabaseKeyIndex {
        self.key
    }

    /// Returns the interned struct.
    pub fn as_struct(&self) -> C::Struct<'_> {
        FromId::from_id(self.key.key_index())
    }

    #[cfg(feature = "salsa_unstable")]
    pub fn value(&self) -> &'db Value<C> {
        self.value
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
        self.ingredient_index
    }

    unsafe fn maybe_changed_after(
        &self,
        zalsa: &crate::zalsa::Zalsa,
        _db: crate::database::RawDatabase<'_>,
        input: Id,
        _revision: Revision,
    ) -> VerifyResult {
        // Record the current revision as active.
        let current_revision = zalsa.current_revision();
        if C::REVISIONS != IMMORTAL {
            self.revision_queue.record(current_revision);
        }

        let value = zalsa.table().get::<Value<C>>(input);

        // SAFETY: `value.shard` is guaranteed to be in-bounds for `self.shards`.
        let _shard = unsafe { self.shards.get_unchecked(value.shard as usize) }.lock();

        // SAFETY: We hold the lock for the shard containing the value.
        let metadata = unsafe { &mut *value.lru.metadata.get() };

        // The slot was reused.
        if metadata.id.generation() > input.generation() {
            return VerifyResult::changed();
        }

        // Validate the value for the current revision to avoid reuse.
        metadata.last_interned_at = current_revision;

        zalsa.event(&|| {
            let index = self.database_key_index(input);

            Event::new(EventKind::DidValidateInternedValue {
                key: index,
                revision: current_revision,
            })
        });

        // Any change to an interned value results in a new ID generation.
        VerifyResult::unchanged()
    }

    unsafe fn maybe_changed_after_leaf(
        &self,
        zalsa: &Zalsa,
        db: crate::database::RawDatabase<'_>,
        input: Id,
        revision: Revision,
    ) -> Option<VerifyResult> {
        // SAFETY: The caller supplies the same database required by ordinary validation.
        Some(unsafe { self.maybe_changed_after(zalsa, db, input, revision) })
    }

    fn collect_minimum_serialized_edges(
        &self,
        _zalsa: &Zalsa,
        edge: QueryEdge,
        serialized_edges: &mut FxIndexSet<QueryEdge>,
        _visited_edges: &mut FxHashSet<QueryEdge>,
    ) {
        if C::PERSIST && C::REVISIONS != IMMORTAL {
            // If the interned struct is being persisted, it may be reachable through transitive queries.
            // Additionally, interned struct dependencies are impure in that garbage collection can
            // invalidate a dependency without a base input necessarily being updated. Thus, we must
            // preserve the transitive dependency on the interned struct, if garbage collection is
            // enabled.
            serialized_edges.insert(edge);
        }

        // Otherwise, the dependency is covered by the base inputs.
    }

    fn flatten_cycle_head_dependencies(
        &self,
        _zalsa: &Zalsa,
        id: Id,
        flattened_input_outputs: &mut FxIndexSet<QueryEdge>,
        _seen: &mut FxHashSet<DatabaseKeyIndex>,
    ) {
        flattened_input_outputs.insert(QueryEdge::input(self.database_key_index(id)));
    }

    fn debug_name(&self) -> &'static str {
        C::DEBUG_NAME
    }

    fn jar_kind(&self) -> JarKind {
        JarKind::Struct
    }

    fn memo_table_types(&self) -> &Arc<MemoTableTypes> {
        &self.memo_table_types
    }

    fn memo_table_types_mut(&mut self) -> &mut Arc<MemoTableTypes> {
        &mut self.memo_table_types
    }

    /// Returns memory usage information about any interned values.
    #[cfg(all(not(feature = "shuttle"), feature = "salsa_unstable"))]
    fn memory_usage(&self, db: &dyn crate::Database) -> Option<Vec<crate::database::SlotInfo>> {
        use parking_lot::lock_api::RawMutex;

        for shard in self.shards.iter() {
            // SAFETY: We do not hold any active mutex guards.
            unsafe { shard.raw().lock() };
        }

        // SAFETY: We hold the locks for all shards.
        let entries = unsafe { self.entries_inner(false, db.zalsa()) };

        let memory_usage = entries
            // SAFETY: The memo table belongs to a value that we allocated, so it
            // has the correct type. Additionally, we are holding the locks for all shards.
            .map(|entry| unsafe { entry.value.memory_usage(&self.memo_table_types) })
            .collect();

        for shard in self.shards.iter() {
            // SAFETY: We acquired the locks for all shards.
            unsafe { shard.raw().unlock() };
        }

        Some(memory_usage)
    }

    fn is_persistable(&self) -> bool {
        C::PERSIST
    }

    fn should_serialize(&self, zalsa: &Zalsa) -> bool {
        C::PERSIST && self.entries(zalsa).next().is_some()
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

impl<C> std::fmt::Debug for IngredientImpl<C>
where
    C: Configuration,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("index", &self.ingredient_index)
            .finish()
    }
}

// SAFETY: `Value<C>` is our private type branded over the unique configuration `C`.
unsafe impl<C> Slot for Value<C>
where
    C: Configuration,
{
    #[inline(always)]
    unsafe fn memos(
        this: *const Self,
        _current_revision: Revision,
    ) -> *const crate::table::memo::MemoTable {
        // SAFETY: The fact that we have a pointer to the `Value` means it must
        // have been interned, and thus validated, in the current revision.
        // Caller obligation demands this pointer to be valid.
        unsafe { (*this).memos.get() }
    }

    #[inline(always)]
    fn memos_mut(&mut self) -> &mut crate::table::memo::MemoTable {
        self.memos.get_mut()
    }
}

/// Keep track of revisions in which interned values were read, to determine staleness.
///
/// An interned value is considered stale if it has not been read in the past `REVS`
/// revisions. However, we only consider revisions in which interned values were actually
/// read, as revisions may be created in bursts.
struct RevisionQueue {
    lock: Mutex<()>,
    /// Recent revisions, stored inline for the default configuration.
    revisions: SmallVec<[AtomicRevision; DEFAULT_REVISIONS]>,
}

// `#[salsa::interned(revisions = usize::MAX)]` disables garbage collection.
const IMMORTAL: NonZeroUsize = NonZeroUsize::MAX;

impl RevisionQueue {
    fn new(capacity: NonZeroUsize) -> Self {
        let revisions = if capacity == IMMORTAL {
            SmallVec::new()
        } else {
            (0..capacity.get())
                .map(|_| AtomicRevision::start())
                .collect()
        };

        Self {
            lock: Mutex::new(()),
            revisions,
        }
    }

    /// Record the given revision as active.
    #[inline]
    fn record(&self, revision: Revision) {
        debug_assert!(
            !self.revisions.is_empty(),
            "cannot record revisions when interned garbage collection is disabled"
        );

        // Fast-path: We already recorded this revision.
        if self.revisions[0].load() >= revision {
            return;
        }

        self.record_cold(revision);
    }

    #[cold]
    fn record_cold(&self, revision: Revision) {
        let _lock = self.lock.lock();

        // Another thread may have recorded this revision while we waited for the lock.
        if self.revisions[0].load() >= revision {
            return;
        }

        // Otherwise, update the queue, maintaining sorted order.
        //
        // Note that this should only happen once per revision.
        for i in (1..self.revisions.len()).rev() {
            self.revisions[i].store(self.revisions[i - 1].load());
        }

        self.revisions[0].store(revision);
    }

    /// Returns `true` if the given revision is old enough to be considered stale.
    #[inline]
    fn is_stale(&self, revision: Revision) -> bool {
        let Some(oldest) = self.revisions.last() else {
            return false;
        };
        let oldest = oldest.load();

        // If we have not recorded `REVS` revisions yet, nothing can be stale.
        if oldest == Revision::start() {
            return false;
        }

        revision < oldest
    }

    /// Returns `true` if the configured number of revisions have been recorded as active,
    /// i.e. enough data has been recorded to start garbage collection.
    #[inline]
    fn is_primed(&self) -> bool {
        self.revisions
            .last()
            .is_some_and(|oldest| oldest.load() > Revision::start())
    }
}

/// A trait for types that hash and compare like `O`.
pub trait HashEqLike<O> {
    fn hash<H: Hasher>(&self, h: &mut H);
    fn eq(&self, data: &O) -> bool;
}

/// The `Lookup` trait is a more flexible variant on [`std::borrow::Borrow`]
/// and [`std::borrow::ToOwned`].
///
/// It is implemented by "some type that can be used as the lookup key for `O`".
/// This means that `self` can be hashed and compared for equality with values
/// of type `O` without actually creating an owned value. It `self` needs to be interned,
/// it can be converted into an equivalent value of type `O`.
///
/// The canonical example is `&str: Lookup<String>`. However, this example
/// alone can be handled by [`std::borrow::Borrow`][]. In our case, we may have
/// multiple keys accumulated into a struct, like `ViewStruct: Lookup<(K1, ...)>`,
/// where `struct ViewStruct<L1: Lookup<K1>...>(K1...)`. The `Borrow` trait
/// requires that `&(K1...)` be convertible to `&ViewStruct` which just isn't
/// possible. `Lookup` instead offers direct `hash` and `eq` methods.
pub trait Lookup<O> {
    fn into_owned(self) -> O;
}

impl<T> Lookup<T> for T {
    fn into_owned(self) -> T {
        self
    }
}

impl<T> HashEqLike<T> for T
where
    T: Hash + Eq,
{
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, &mut *h);
    }

    fn eq(&self, data: &T) -> bool {
        self == data
    }
}

impl<T> HashEqLike<T> for &T
where
    T: Hash + Eq,
{
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(*self, &mut *h);
    }

    fn eq(&self, data: &T) -> bool {
        **self == *data
    }
}

impl<T> HashEqLike<&T> for T
where
    T: Hash + Eq,
{
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, &mut *h);
    }

    fn eq(&self, data: &&T) -> bool {
        *self == **data
    }
}

impl<T> Lookup<T> for &T
where
    T: Clone,
{
    fn into_owned(self) -> T {
        Clone::clone(self)
    }
}

impl<'a, T> HashEqLike<&'a T> for Box<T>
where
    T: ?Sized + Hash + Eq,
    Box<T>: From<&'a T>,
{
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, &mut *h)
    }
    fn eq(&self, data: &&T) -> bool {
        **self == **data
    }
}

impl<'a, T> Lookup<Box<T>> for &'a T
where
    T: ?Sized + Hash + Eq,
    Box<T>: From<&'a T>,
{
    fn into_owned(self) -> Box<T> {
        Box::from(self)
    }
}

impl<'a, T> HashEqLike<&'a T> for Arc<T>
where
    T: ?Sized + Hash + Eq,
    Arc<T>: From<&'a T>,
{
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(&**self, &mut *h)
    }
    fn eq(&self, data: &&T) -> bool {
        **self == **data
    }
}

impl<'a, T> Lookup<Arc<T>> for &'a T
where
    T: ?Sized + Hash + Eq,
    Arc<T>: From<&'a T>,
{
    fn into_owned(self) -> Arc<T> {
        Arc::from(self)
    }
}

#[cfg(feature = "triomphe")]
impl<'a, T> HashEqLike<&'a T> for triomphe::Arc<T>
where
    T: ?Sized + Hash + Eq,
    triomphe::Arc<T>: From<&'a T>,
{
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(&**self, &mut *h)
    }
    fn eq(&self, data: &&T) -> bool {
        **self == **data
    }
}

#[cfg(feature = "triomphe")]
impl<'a, T> Lookup<triomphe::Arc<T>> for &'a T
where
    T: ?Sized + Hash + Eq,
    triomphe::Arc<T>: From<&'a T>,
{
    fn into_owned(self) -> triomphe::Arc<T> {
        triomphe::Arc::from(self)
    }
}

impl Lookup<String> for &str {
    fn into_owned(self) -> String {
        self.to_owned()
    }
}

#[cfg(feature = "compact_str")]
impl Lookup<compact_str::CompactString> for &str {
    fn into_owned(self) -> compact_str::CompactString {
        compact_str::CompactString::new(self)
    }
}

#[cfg(feature = "compact_str")]
impl HashEqLike<&str> for compact_str::CompactString {
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, &mut *h)
    }

    fn eq(&self, data: &&str) -> bool {
        self == *data
    }
}

#[cfg(feature = "compact_str")]
impl HashEqLike<Cow<'_, str>> for compact_str::CompactString {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.as_str().hash(h);
    }

    fn eq(&self, data: &Cow<'_, str>) -> bool {
        self.as_str() == data.as_ref()
    }
}

#[cfg(feature = "compact_str")]
impl Lookup<compact_str::CompactString> for Cow<'_, str> {
    fn into_owned(self) -> compact_str::CompactString {
        compact_str::CompactString::new(Cow::into_owned(self))
    }
}

impl HashEqLike<&str> for String {
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, &mut *h)
    }

    fn eq(&self, data: &&str) -> bool {
        self == *data
    }
}

impl<A, T: Hash + Eq + PartialEq<A>> HashEqLike<&[A]> for Vec<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, h);
    }

    fn eq(&self, data: &&[A]) -> bool {
        self.len() == data.len() && data.iter().enumerate().all(|(i, a)| &self[i] == a)
    }
}

impl<A: Hash + Eq + PartialEq<T> + Clone + Lookup<T>, T> Lookup<Vec<T>> for &[A] {
    fn into_owned(self) -> Vec<T> {
        self.iter().map(|a| Lookup::into_owned(a.clone())).collect()
    }
}

impl<const N: usize, A, T: Hash + Eq + PartialEq<A>> HashEqLike<[A; N]> for Vec<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, h);
    }

    fn eq(&self, data: &[A; N]) -> bool {
        self.len() == data.len() && data.iter().enumerate().all(|(i, a)| &self[i] == a)
    }
}

impl<const N: usize, A: Hash + Eq + PartialEq<T> + Clone + Lookup<T>, T> Lookup<Vec<T>> for [A; N] {
    fn into_owned(self) -> Vec<T> {
        self.into_iter()
            .map(|a| Lookup::into_owned(a.clone()))
            .collect()
    }
}

impl HashEqLike<&Path> for PathBuf {
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, h);
    }

    fn eq(&self, data: &&Path) -> bool {
        self == data
    }
}

impl Lookup<PathBuf> for &Path {
    fn into_owned(self) -> PathBuf {
        self.to_owned()
    }
}

impl<T: Hash + Eq + Clone> HashEqLike<Cow<'_, T>> for T {
    fn hash<H: Hasher>(&self, h: &mut H) {
        Hash::hash(self, h);
    }

    fn eq(&self, data: &Cow<'_, T>) -> bool {
        self == data.as_ref()
    }
}

impl<T: Clone> Lookup<T> for Cow<'_, T> {
    fn into_owned(self) -> T {
        Cow::into_owned(self)
    }
}

impl HashEqLike<Cow<'_, str>> for String {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.as_str().hash(h);
    }

    fn eq(&self, data: &Cow<'_, str>) -> bool {
        self.as_str() == data.as_ref()
    }
}

impl Lookup<String> for Cow<'_, str> {
    fn into_owned(self) -> String {
        Cow::into_owned(self)
    }
}

impl HashEqLike<Cow<'_, Path>> for PathBuf {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.as_path().hash(h);
    }

    fn eq(&self, data: &Cow<'_, Path>) -> bool {
        self.as_path() == data.as_ref()
    }
}

impl Lookup<PathBuf> for Cow<'_, Path> {
    fn into_owned(self) -> PathBuf {
        Cow::into_owned(self)
    }
}

impl<T: Hash + Eq + Clone> HashEqLike<Cow<'_, [T]>> for Box<[T]> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.as_ref().hash(h);
    }

    fn eq(&self, data: &Cow<'_, [T]>) -> bool {
        self.as_ref() == data.as_ref()
    }
}

impl<T: Clone> Lookup<Box<[T]>> for Cow<'_, [T]> {
    fn into_owned(self) -> Box<[T]> {
        Cow::into_owned(self).into_boxed_slice()
    }
}

impl<T: Hash + Eq + Clone> HashEqLike<Cow<'_, [T]>> for Vec<T> {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.as_slice().hash(h);
    }

    fn eq(&self, data: &Cow<'_, [T]>) -> bool {
        self.as_slice() == data.as_ref()
    }
}

impl<T: Clone> Lookup<Vec<T>> for Cow<'_, [T]> {
    fn into_owned(self) -> Vec<T> {
        Cow::into_owned(self)
    }
}

#[cfg(feature = "persistence")]
mod persistence {
    use std::cell::UnsafeCell;
    use std::fmt;
    use std::hash::BuildHasher;

    use intrusive_collections::LinkedListLink;
    use serde::ser::{SerializeMap, SerializeStruct};
    use serde::{Deserialize, de};

    use super::{Configuration, EntryMetadata, IngredientImpl, LruEntry, Value};
    use crate::plumbing::Ingredient;
    use crate::table::memo::MemoTable;
    use crate::zalsa::Zalsa;
    use crate::{Durability, Id, Revision};

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
            let Self { zalsa, ingredient } = *self;

            let count = ingredient
                .shards
                .iter()
                .map(|shard| shard.lock().key_map.len())
                .sum();

            let mut map = serializer.serialize_map(Some(count))?;

            for (_, value) in zalsa.table().slots_of::<Value<C>>() {
                // SAFETY: The safety invariant of `Ingredient::serialize` ensures we have exclusive access
                // to the database.
                let id = unsafe { (*value.lru.metadata.get()).id };

                map.serialize_entry(&id.as_bits(), value)?;
            }

            map.end()
        }
    }

    impl<C> serde::Serialize for Value<C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let mut value = serializer.serialize_struct("Value,", 3)?;

            let Value {
                fields,
                lru,
                durability,
                shard: _,
                memos: _,
            } = self;
            let LruEntry { link: _, metadata } = lru;

            // SAFETY: The safety invariant of `Ingredient::serialize` ensures we have exclusive access
            // to the database.
            let fields = unsafe { &*fields.get() };

            // SAFETY: The safety invariant of `Ingredient::serialize` ensures we have exclusive access
            // to the database.
            let durability = unsafe { *durability.get() };

            // SAFETY: The safety invariant of `Ingredient::serialize` ensures we have exclusive access
            // to the database.
            let EntryMetadata {
                last_interned_at,
                id: _,
            } = unsafe { *metadata.get() };

            value.serialize_field("durability", &durability)?;
            value.serialize_field("last_interned_at", &last_interned_at)?;
            value.serialize_field("fields", &SerializeFields::<C>(fields))?;

            value.end()
        }
    }

    struct SerializeFields<'db, C: Configuration>(&'db C::Fields<'static>);

    impl<C> serde::Serialize for SerializeFields<'_, C>
    where
        C: Configuration,
    {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            C::serialize(IngredientImpl::<C>::from_internal_data(self.0), serializer)
        }
    }

    pub struct DeserializeIngredient<'db, C>
    where
        C: Configuration,
    {
        pub zalsa: &'db mut Zalsa,
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

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let DeserializeIngredient { zalsa, ingredient } = self;

            while let Some((id, value)) = access.next_entry::<u64, DeserializeValue<C>>()? {
                let id = Id::from_bits(id);
                let (page_idx, _) = crate::table::split_id(id);

                // Determine the value shard.
                let hash = ingredient.hasher.hash_one(&value.fields.0);
                let shard_index = ingredient.shard(hash);

                // SAFETY: `shard_index` is guaranteed to be in-bounds for `self.shards`.
                let shard = unsafe { &mut *ingredient.shards.get_unchecked(shard_index).lock() };

                let value = Value::<C> {
                    shard: shard_index as u16,
                    lru: LruEntry {
                        link: LinkedListLink::new(),
                        metadata: UnsafeCell::new(EntryMetadata {
                            id,
                            last_interned_at: value.last_interned_at,
                        }),
                    },
                    fields: UnsafeCell::new(value.fields.0),
                    // SAFETY: We only ever access the memos of a value that we allocated through
                    // our `MemoTableTypes`.
                    memos: UnsafeCell::new(unsafe {
                        MemoTable::new(ingredient.memo_table_types())
                    }),
                    durability: UnsafeCell::new(value.durability),
                };

                // Force initialize the relevant page.
                zalsa.table_mut().force_page::<Value<C>>(
                    page_idx,
                    ingredient.ingredient_index(),
                    ingredient.memo_table_types(),
                );

                // Initialize the slot.
                //
                // SAFETY: We have a mutable reference to the database.
                let (allocated_id, value) = unsafe {
                    zalsa
                        .table()
                        .page(page_idx)
                        .allocate(page_idx, |_| value)
                        .unwrap_or_else(|_| panic!("serialized an invalid `Id`: {id:?}"))
                };

                assert_eq!(
                    allocated_id.index(),
                    id.index(),
                    "values are serialized in allocation order"
                );

                // Insert the newly allocated value into our ingredient.
                //
                // SAFETY: We hold this ingredient's shard lock, and `value` is the live, stable
                // value that we just allocated in that shard.
                unsafe { ingredient.insert_value(shard, hash, value) };
            }

            Ok(())
        }
    }

    #[derive(Deserialize)]
    #[serde(rename = "Value")]
    pub struct DeserializeValue<C: Configuration> {
        durability: Durability,
        last_interned_at: Revision,
        #[serde(bound = "C: Configuration")]
        fields: DeserializeFields<C>,
    }

    struct DeserializeFields<C: Configuration>(C::Fields<'static>);

    impl<'de, C> serde::Deserialize<'de> for DeserializeFields<C>
    where
        C: Configuration,
    {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            C::deserialize(deserializer)
                .map(DeserializeFields)
                .map_err(de::Error::custom)
        }
    }
}

#[cfg(all(not(feature = "shuttle"), target_pointer_width = "64"))]
mod _static_assertions {
    use std::mem;

    use super::LruEntry;
    use super::{Configuration, Value};
    use crate::{Id, plumbing};

    const _: [(); mem::size_of::<LruEntry>()] = [(); mem::size_of::<[usize; 4]>()];

    const _: [(); mem::size_of::<Value<DummyConfiguration>>()] = [(); mem::size_of::<[usize; 7]>()];

    struct DummyConfiguration;

    // SAFETY: The fields are `[u8; 1]` for every database lifetime.
    unsafe impl Configuration for DummyConfiguration {
        const LOCATION: crate::ingredient::Location =
            crate::ingredient::Location { file: "", line: 0 };
        const DEBUG_NAME: &'static str = "";
        const PERSIST: bool = false;

        type Fields<'db> = [u8; 1];
        type Struct<'db> = Id;

        fn serialize<S>(_: &Self::Fields<'_>, _: S) -> Result<S::Ok, S::Error>
        where
            S: plumbing::serde::Serializer,
        {
            unimplemented!()
        }

        fn deserialize<'de, D>(_: D) -> Result<Self::Fields<'static>, D::Error>
        where
            D: plumbing::serde::Deserializer<'de>,
        {
            unimplemented!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestConfiguration;

    // SAFETY: The fields are `()` for every database lifetime.
    unsafe impl Configuration for TestConfiguration {
        const LOCATION: crate::ingredient::Location = crate::ingredient::Location {
            file: file!(),
            line: line!(),
        };
        const DEBUG_NAME: &'static str = "TestConfiguration";
        const PERSIST: bool = false;
        const REVISIONS: NonZeroUsize = NonZeroUsize::new(2).unwrap();

        type Fields<'db> = ();
        type Struct<'db> = Id;

        fn serialize<S>(_value: &Self::Fields<'_>, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: plumbing::serde::Serializer,
        {
            unimplemented!()
        }

        fn deserialize<'de, D>(_deserializer: D) -> Result<Self::Fields<'static>, D::Error>
        where
            D: plumbing::serde::Deserializer<'de>,
        {
            unimplemented!()
        }
    }

    #[test]
    fn revision_queue_records_each_revision_once() {
        let queue = RevisionQueue::new(TestConfiguration::REVISIONS);
        let revision = Revision::start().next();

        // Simulate two threads that both passed the fast-path check before taking the lock.
        queue.record_cold(revision);
        queue.record_cold(revision);

        assert_eq!(queue.revisions[0].load(), revision);
        assert_eq!(queue.revisions[1].load(), Revision::start());
    }
}
