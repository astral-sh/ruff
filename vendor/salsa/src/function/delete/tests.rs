#![cfg(not(feature = "shuttle"))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::{DeletedEntries, PreparedRetirement, RetirementEntry};
use crate::cycle::CycleRecoveryStrategy;
use crate::function::memo::{Memo, PreparedMemo};
use crate::function::{Configuration, IngredientImpl, NoopEviction};
use crate::ingredient::Location;
use crate::plumbing;
use crate::plumbing::AsId;
use crate::zalsa::{Zalsa, ZalsaDatabase};
use crate::zalsa_local::{OriginAndExtra, QueryRevisions};
use crate::{Cycle, Database, DatabaseImpl, Durability, Id, Revision};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Allocations {
    alloc: usize,
    zeroed: usize,
    realloc: usize,
    dealloc: usize,
    pub(crate) requested_bytes: usize,
}

impl Allocations {
    pub(crate) fn allocation_requests(self) -> usize {
        self.alloc
            .saturating_add(self.zeroed)
            .saturating_add(self.realloc)
    }
}

thread_local! {
    static ALLOCATIONS: Cell<Option<Allocations>> = const { Cell::new(None) };
}

struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn record(update: impl FnOnce(&mut Allocations)) {
    let _ = ALLOCATIONS.try_with(|slot| {
        if let Some(mut counts) = slot.get() {
            update(&mut counts);
            slot.set(Some(counts));
        }
    });
}

// SAFETY: Every operation forwards the caller's allocation contract unchanged to System.
// The TLS observer only updates initialized scalars and never allocates or invokes callbacks.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(|counts| {
            counts.alloc = counts.alloc.saturating_add(1);
            counts.requested_bytes = counts.requested_bytes.saturating_add(layout.size());
        });
        // SAFETY: The caller supplied the GlobalAlloc allocation contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(|counts| {
            counts.zeroed = counts.zeroed.saturating_add(1);
            counts.requested_bytes = counts.requested_bytes.saturating_add(layout.size());
        });
        // SAFETY: The caller supplied the GlobalAlloc allocation contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(|counts| {
            counts.realloc = counts.realloc.saturating_add(1);
            counts.requested_bytes = counts.requested_bytes.saturating_add(size);
        });
        // SAFETY: The caller supplied the live pointer, layout, and new size.
        unsafe { System.realloc(ptr, layout, size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record(|counts| counts.dealloc = counts.dealloc.saturating_add(1));
        // SAFETY: The caller supplied the live pointer and its allocation layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

struct CountGuard;

impl Drop for CountGuard {
    fn drop(&mut self) {
        ALLOCATIONS.with(|counts| counts.set(None));
    }
}

pub(crate) fn allocations<T>(action: impl FnOnce() -> T) -> (T, Allocations) {
    ALLOCATIONS.with(|counts| assert!(counts.get().is_none(), "nested allocation measurement"));
    let guard = CountGuard;
    ALLOCATIONS.with(|counts| counts.set(Some(Allocations::default())));
    let result = action();
    let counts = ALLOCATIONS
        .with(|counts| counts.replace(None))
        .expect("active measurement");
    drop(guard);
    (result, counts)
}

#[test]
fn allocation_observer_counts_whole_replacement_requests() {
    let mut values = Vec::<usize>::new();
    let ((), initial) = allocations(|| values.reserve_exact(4));
    assert_eq!(initial.requested_bytes, 4 * size_of::<usize>());
    let ((), replacement) = allocations(|| values.reserve_exact(8));
    assert_eq!(replacement.requested_bytes, 8 * size_of::<usize>());
    assert_eq!(replacement.allocation_requests(), 1);
}

#[crate::input]
struct Key {
    value: u32,
}

#[derive(Debug)]
struct Value {
    number: usize,
    drops: Arc<AtomicUsize>,
}

impl Drop for Value {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

#[crate::tracked(returns(ref), no_eq)]
fn stored(_db: &dyn Database, _key: Key) -> Value {
    panic!("installation controls supply the prepared output directly")
}

struct Config;

// SAFETY: Output is the same owned, static type for every database lifetime.
unsafe impl Configuration for Config {
    const DEBUG_NAME: &'static str = "retirement_test";
    const LOCATION: Location = Location {
        file: file!(),
        line: line!(),
    };
    const PERSIST: bool = false;
    const CYCLE_STRATEGY: CycleRecoveryStrategy = CycleRecoveryStrategy::Panic;

    type DbView = dyn Database;
    type SalsaStruct<'db> = Key;
    type Input<'db> = ();
    type Output<'db> = Value;
    type Eviction = NoopEviction;

    fn values_equal<'db>(old: &Self::Output<'db>, new: &Self::Output<'db>) -> bool {
        old.number == new.number
    }

    fn id_to_input(_: &Zalsa, _: Id) -> Self::Input<'_> {}

    fn execute<'db>(_: &'db Self::DbView, _: Self::Input<'db>) -> Self::Output<'db> {
        panic!("retirement storage controls do not execute queries")
    }

    fn cycle_initial<'db>(_: &'db Self::DbView, _: Id, _: Self::Input<'db>) -> Self::Output<'db> {
        panic!("retirement storage controls do not execute cycles")
    }

    fn recover_from_cycle<'db>(
        _: &'db Self::DbView,
        _: &Cycle,
        _: &Self::Output<'db>,
        value: Self::Output<'db>,
        _: Self::Input<'db>,
    ) -> Self::Output<'db> {
        value
    }

    fn serialize<S>(_: &Self::Output<'_>, _: S) -> Result<S::Ok, S::Error>
    where
        S: plumbing::serde::Serializer,
    {
        panic!("retirement storage controls do not persist")
    }

    fn deserialize<'de, D>(_: D) -> Result<Self::Output<'static>, D::Error>
    where
        D: plumbing::serde::Deserializer<'de>,
    {
        panic!("retirement storage controls do not persist")
    }
}

fn memo(number: usize, drops: &Arc<AtomicUsize>) -> NonNull<Memo<Config>> {
    NonNull::from(Box::leak(Box::new(Memo::new(
        Some(Value {
            number,
            drops: drops.clone(),
        }),
        Revision::start(),
        revisions(),
    ))))
}

fn revisions() -> QueryRevisions {
    QueryRevisions {
        changed_at: Revision::start(),
        durability: Durability::LOW,
        origin_and_extra: OriginAndExtra::derived(std::iter::empty(), Default::default()),
        #[cfg(feature = "accumulator")]
        accumulated_inputs: Default::default(),
        verified_final: AtomicBool::new(true),
    }
}

#[test]
fn prepared_retirement_fill_does_not_allocate() {
    for prefill in [0, 28, 32, 88, 96, 208] {
        let mut deleted = DeletedEntries::<Config>::default();
        let drops = Arc::new(AtomicUsize::new(0));
        for number in 0..prefill {
            // SAFETY: The new allocation is uniquely transferred to this retirement list.
            unsafe { deleted.push(memo(number, &drops)) };
        }
        let assigned = memo(prefill, &drops);
        let (prepared, preparation) = allocations(|| deleted.prepare());
        assert_eq!(deleted.memos.iter().count(), prefill + 1);
        // SAFETY: The new allocation is uniquely transferred to the prepared entry.
        let ((), fill) = allocations(|| unsafe { prepared.finish(Some(assigned)) });
        assert_eq!(fill, Allocations::default(), "prefill {prefill}");
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        // SAFETY: The filled entry retains this allocation until the list is cleared below.
        assert_eq!(
            unsafe { assigned.as_ref() }.value().unwrap().number,
            prefill
        );
        eprintln!("retirement prefill={prefill} prepare={preparation:?} fill={fill:?}");
        deleted.clear();
        assert_eq!(drops.load(Ordering::Relaxed), prefill + 1);
    }
}

#[test]
fn prepared_retirement_slots_preserve_unique_reclamation() {
    let mut deleted = DeletedEntries::<Config>::default();
    drop(deleted.prepare());
    drop(deleted.prepare());
    assert_eq!(deleted.memos.iter().count(), 2);
    let first = std::ptr::from_ref(&deleted.memos[0]);
    let second = std::ptr::from_ref(&deleted.memos[1]);
    assert_ne!(first, second);
    let abandoned_payload = deleted.memos.iter().count() * size_of::<RetirementEntry<Config>>();
    assert_eq!(abandoned_payload, 2 * size_of::<NonNull<Memo<Config>>>());

    // SAFETY: No memo allocation is transferred by an empty replacement.
    unsafe { deleted.prepare().finish(None) };
    let assigned_drops = Arc::new(AtomicUsize::new(0));
    let ordinary_drops = Arc::new(AtomicUsize::new(0));
    let assigned = memo(17, &assigned_drops);
    let ordinary = memo(29, &ordinary_drops);
    // SAFETY: Both allocations remain owned by the locals until their unique transfers below,
    // then by deleted; neither reference is used after clear.
    let assigned_ref = unsafe { assigned.as_ref() };
    let ordinary_ref = unsafe { ordinary.as_ref() };
    let prepared = deleted.prepare();
    let entry = std::ptr::from_ref(&deleted.memos[3]);
    for _ in 0..220 {
        drop(deleted.prepare());
    }
    // SAFETY: Each live allocation is transferred once to a distinct retirement entry.
    unsafe {
        deleted.push(ordinary);
        prepared.finish(Some(assigned));
    }
    assert_eq!(deleted.memos.iter().count(), 225);
    assert_eq!(std::ptr::from_ref(&deleted.memos[0]), first);
    assert_eq!(std::ptr::from_ref(&deleted.memos[1]), second);
    assert_eq!(std::ptr::from_ref(&deleted.memos[3]), entry);
    assert!(deleted.memos[0].memo.load(Ordering::Relaxed).is_null());
    assert!(deleted.memos[1].memo.load(Ordering::Relaxed).is_null());
    assert!(deleted.memos[2].memo.load(Ordering::Relaxed).is_null());
    assert_eq!(
        deleted.memos[3].memo.load(Ordering::Relaxed),
        assigned.as_ptr()
    );
    assert_eq!(
        deleted.memos[224].memo.load(Ordering::Relaxed),
        ordinary.as_ptr()
    );
    assert_eq!(assigned_ref.value().unwrap().number, 17);
    assert_eq!(ordinary_ref.value().unwrap().number, 29);
    assert_eq!(assigned_drops.load(Ordering::Relaxed), 0);
    assert_eq!(ordinary_drops.load(Ordering::Relaxed), 0);
    deleted.clear();
    assert_eq!(deleted.memos.iter().count(), 0);
    assert_eq!(assigned_drops.load(Ordering::Relaxed), 1);
    assert_eq!(ordinary_drops.load(Ordering::Relaxed), 1);
    deleted.clear();
    drop(deleted);
    assert_eq!(assigned_drops.load(Ordering::Relaxed), 1);
    assert_eq!(ordinary_drops.load(Ordering::Relaxed), 1);
}

#[test]
fn retirement_entry_preserves_pointer_layout_and_owned_auto_traits() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Memo<Config>>();
    send_sync::<RetirementEntry<Config>>();
    send_sync::<DeletedEntries<Config>>();
    send_sync::<PreparedRetirement<'static, Config>>();
    assert_eq!(
        size_of::<RetirementEntry<Config>>(),
        size_of::<NonNull<Memo<Config>>>()
    );
    assert_eq!(
        align_of::<RetirementEntry<Config>>(),
        align_of::<NonNull<Memo<Config>>>()
    );
    eprintln!(
        "retirement layout entry={} token={} list={}",
        size_of::<RetirementEntry<Config>>(),
        size_of::<PreparedRetirement<'_, Config>>(),
        size_of::<DeletedEntries<Config>>()
    );
}

fn install<C>(
    db: &dyn Database,
    key: Key,
    ingredient: &IngredientImpl<C>,
    old_drops: &Arc<AtomicUsize>,
    new_drops: &Arc<AtomicUsize>,
) where
    C: for<'db> Configuration<SalsaStruct<'db> = Key, Output<'db> = Value>,
{
    let index = ingredient.memo_ingredient_index(db.zalsa(), key.as_id());
    let first = PreparedMemo::new(
        Value {
            number: 41,
            drops: old_drops.clone(),
        },
        Revision::start(),
        revisions(),
    );
    let first_slot = ingredient
        .prepare_memo_slot(db.zalsa(), key.as_id(), index)
        .expect("registered input memo slot");
    let initial_retirement = ingredient.deleted_entries.prepare();
    let old = ingredient.install_prepared_memo(first_slot, first, Some(initial_retirement));
    assert_eq!(ingredient.deleted_entries.memos.iter().count(), 1);
    assert!(
        ingredient.deleted_entries.memos[0]
            .memo
            .load(Ordering::Relaxed)
            .is_null()
    );
    for _ in 1..28 {
        drop(ingredient.deleted_entries.prepare());
    }
    assert_eq!(ingredient.deleted_entries.memos.iter().count(), 28);
    assert_eq!(
        ingredient.memo_preparation_bytes(db.zalsa(), key.as_id()),
        Some(size_of::<Memo<C>>() + DeletedEntries::<C>::entry_size()),
        "a warm table still quotes one retirement payload for each preparation"
    );
    let prepared = PreparedMemo::new(
        Value {
            number: 73,
            drops: new_drops.clone(),
        },
        Revision::start(),
        revisions(),
    );
    let slot = ingredient
        .prepare_memo_slot(db.zalsa(), key.as_id(), index)
        .expect("registered input memo slot");
    let retirement = ingredient.deleted_entries.prepare();
    assert_eq!(ingredient.deleted_entries.memos.iter().count(), 29);
    let (accepted, counts) =
        allocations(|| ingredient.install_prepared_memo(slot, prepared, Some(retirement)));
    assert_eq!(counts, Allocations::default());
    let selected = ingredient
        .get_memo_from_table_for(db.zalsa(), key.as_id(), index)
        .expect("new allocation selected");
    assert!(std::ptr::eq(selected, accepted));
    assert!(!std::ptr::eq(old, accepted));
    assert_eq!(old.value().unwrap().number, 41);
    assert_eq!(accepted.value().unwrap().number, 73);
    assert_eq!(
        ingredient.deleted_entries.memos[28]
            .memo
            .load(Ordering::Relaxed),
        std::ptr::from_ref(old).cast_mut()
    );
    assert_eq!(old_drops.load(Ordering::Relaxed), 0);
    assert_eq!(new_drops.load(Ordering::Relaxed), 0);
    eprintln!(
        "prepared_installation prefill=28 interval=swap_and_retirement_fill allocations={counts:?}"
    );
}

#[test]
fn prepared_installation_preserves_old_borrow_without_allocating() {
    let db = DatabaseImpl::default();
    let key = Key::new(&db, 0);
    let old_drops = Arc::new(AtomicUsize::new(0));
    let new_drops = Arc::new(AtomicUsize::new(0));
    install(
        &db,
        key,
        stored::fn_ingredient_(&db, db.zalsa()),
        &old_drops,
        &new_drops,
    );
    drop(db);
    assert_eq!(old_drops.load(Ordering::Relaxed), 1);
    assert_eq!(new_drops.load(Ordering::Relaxed), 1);
}
