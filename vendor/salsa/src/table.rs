use std::alloc::Layout;
use std::any::{Any, TypeId};
use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::mem::{self, MaybeUninit};
use std::ptr::{self, NonNull};
use std::slice;

use memo::MemoTable;
use rustc_hash::FxHashMap;

use crate::sync::atomic::{AtomicUsize, Ordering};
use crate::sync::{Arc, Mutex};
use crate::table::memo::{MemoTableTypes, MemoTableWithTypes, MemoTableWithTypesMut};
use crate::{Id, IngredientIndex, Revision};

pub(crate) mod memo;

const PAGE_LEN_BITS: usize = 7;
const PAGE_LEN_MASK: usize = PAGE_LEN - 1;
const PAGE_LEN: usize = 1 << PAGE_LEN_BITS;
const MAX_PAGES: usize = Id::MAX_USIZE / PAGE_LEN;

/// A typed [`Page`] view.
pub(crate) struct PageView<'p, T: Slot>(&'p Page, PhantomData<&'p T>);

pub struct Table {
    pages: boxcar::Vec<Page>,
    /// Map from ingredient to non-full pages that are up for grabs
    non_full_pages: Mutex<FxHashMap<IngredientIndex, Vec<PageIndex>>>,
}

/// # Safety
///
/// Implementors of this trait need to make sure that their type is unique with respect to
/// their owning ingredient as the allocation strategy relies on this.
pub unsafe trait Slot: Any + Send + Sync {
    /// Access the [`MemoTable`][] for this slot.
    ///
    /// # Safety condition
    ///
    /// The current revision MUST be the current revision of the database containing this slot.
    unsafe fn memos(slot: *const Self, current_revision: Revision) -> *const MemoTable;

    /// Mutably access the [`MemoTable`] for this slot.
    fn memos_mut(&mut self) -> &mut MemoTable;
}

/// [Slot::memos]
type SlotMemosFnErased = unsafe fn(*const (), current_revision: Revision) -> *const MemoTable;
/// [Slot::memos]
type SlotMemosFn<T> = unsafe fn(*const T, current_revision: Revision) -> *const MemoTable;
/// [Slot::memos_mut]
type SlotMemosMutFnErased = unsafe fn(*mut ()) -> *mut MemoTable;
/// [Slot::memos_mut]
type SlotMemosMutFn<T> = fn(&mut T) -> &mut MemoTable;

struct SlotVTable {
    layout: Layout,
    /// [`Slot`] methods
    memos: SlotMemosFnErased,
    memos_mut: SlotMemosMutFnErased,
    /// The type name of what is stored as entries in data.
    type_name: fn() -> &'static str,
    /// A drop impl to call when the own page drops
    /// SAFETY: The caller is required to supply a valid pointer to a `Box<PageDataEntry<T>>`, and
    /// the correct initialized length and memo types.
    drop_impl: unsafe fn(data: *mut (), initialized: usize, memo_types: &MemoTableTypes),
}

impl SlotVTable {
    const fn of<T: Slot>() -> &'static Self {
        const {
            &Self {
                drop_impl: |data, initialized, memo_types| {
                    // SAFETY: The caller is required to provide a valid data pointer.
                    let data = unsafe { Box::from_raw(data.cast::<PageData<T>>()) };
                    for i in 0..initialized {
                        let item = data[i].get().cast::<T>();
                        // SAFETY: The caller is required to provide a valid initialized length.
                        unsafe {
                            memo_types.attach_memos_mut((*item).memos_mut()).drop();
                            ptr::drop_in_place(item);
                        }
                    }
                },
                layout: Layout::new::<T>(),
                type_name: std::any::type_name::<T>,
                // SAFETY: The signatures are ABI-compatible.
                memos: unsafe { mem::transmute::<SlotMemosFn<T>, SlotMemosFnErased>(T::memos) },
                // SAFETY: The signatures are ABI-compatible.
                memos_mut: unsafe {
                    mem::transmute::<SlotMemosMutFn<T>, SlotMemosMutFnErased>(T::memos_mut)
                },
            }
        }
    }
}

type PageDataEntry<T> = UnsafeCell<MaybeUninit<T>>;
type PageData<T> = [PageDataEntry<T>; PAGE_LEN];

struct Page {
    /// The ingredient for elements on this page.
    ingredient: IngredientIndex,

    /// Number of elements of `data` that are initialized.
    allocated: AtomicUsize,

    /// The potentially uninitialized data of this page. As we initialize new entries, we increment `allocated`.
    /// This is a box allocated `PageData<SlotType>`
    data: NonNull<()>,

    /// A vtable for the slot type stored in this page.
    slot_vtable: &'static SlotVTable,
    /// The type id of what is stored as entries in data.
    // FIXME: Move this into SlotVTable once const stable
    slot_type_id: TypeId,

    memo_types: Arc<MemoTableTypes>,
}

// SAFETY: `Page` is `Send` as we make sure to only ever store `Slot` types in it which
// requires `Send`.`
unsafe impl Send for Page /* where for<M: Memo> M: Send */ {}
// SAFETY: `Page` is `Sync` as we make sure to only ever store `Slot` types in it which
// requires `Sync`.`
unsafe impl Sync for Page /* where for<M: Memo> M: Sync */ {}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PageIndex(usize);

impl PageIndex {
    #[inline]
    fn new(idx: usize) -> Self {
        debug_assert!(idx < MAX_PAGES);
        Self(idx)
    }

    #[allow(dead_code)]
    pub fn as_usize(&self) -> usize {
        self.0
    }
}

#[derive(Copy, Clone, Debug)]
pub struct SlotIndex(usize);

impl SlotIndex {
    #[inline]
    fn new(idx: usize) -> Self {
        debug_assert!(idx < PAGE_LEN);
        Self(idx)
    }
}

impl Default for Table {
    fn default() -> Self {
        Self {
            pages: boxcar::Vec::new(),
            non_full_pages: Default::default(),
        }
    }
}

impl Table {
    /// Returns the [`IngredientIndex`] for an [`Id`].
    #[inline]
    pub fn ingredient_index(&self, id: Id) -> IngredientIndex {
        let (page_idx, _) = split_id(id);
        self.pages[page_idx.0].ingredient
    }

    /// Get a reference to the data for `id`, which must have been allocated from this table with type `T`.
    ///
    /// # Panics
    ///
    /// If `id` is out of bounds or the does not have the type `T`.
    pub(crate) fn get<T: Slot>(&self, id: Id) -> &T {
        let (page, slot) = split_id(id);
        let page_ref = self.page::<T>(page);
        &page_ref.data()[slot.0]
    }

    /// Get a raw pointer to the data for `id`, which must have been allocated from this table.
    ///
    /// # Panics
    ///
    /// If `id` is out of bounds or the does not have the type `T`.
    ///
    /// # Safety
    ///
    /// See [`Page::get_raw`][].
    pub(crate) fn get_raw<T: Slot>(&self, id: Id) -> *mut T {
        let (page, slot) = split_id(id);
        let page_ref = self.page::<T>(page);
        page_ref.page_data()[slot.0].get().cast::<T>()
    }

    /// Returns the number of pages that have been allocated.
    pub fn page_count(&self) -> usize {
        self.pages.count()
    }

    #[cfg(feature = "salsa_unstable")]
    pub(crate) fn page_capacity(&self) -> usize {
        PAGE_LEN
    }

    #[cfg(feature = "salsa_unstable")]
    pub(crate) fn page_infos(&self) -> FxHashMap<IngredientIndex, crate::database::PageInfo> {
        let mut page_fills: FxHashMap<IngredientIndex, Vec<usize>> = FxHashMap::default();

        for (_, page) in self.pages.iter() {
            let fill = page.allocated.load(Ordering::Acquire);
            if fill > 0 {
                page_fills.entry(page.ingredient).or_default().push(fill);
            }
        }

        page_fills
            .into_iter()
            .filter_map(|(ingredient, fills)| {
                crate::database::PageInfo::from_page_fills(PAGE_LEN, fills)
                    .map(|page_info| (ingredient, page_info))
            })
            .collect()
    }

    /// Gets a reference to the page which has slots of type `T`
    ///
    /// # Panics
    ///
    /// If `page` is out of bounds or the type `T` is incorrect.
    #[inline]
    pub(crate) fn page<T: Slot>(&self, page: PageIndex) -> PageView<'_, T> {
        self.pages[page.0].assert_type::<T>()
    }

    /// Force initialize the page at the given index.
    ///
    /// If the page at the provided index was created using `push_uninit_page`, it
    /// will be initialized using the provided ingredient data.
    ///
    /// Otherwise, the page will be allocated.
    ///
    /// # Panics
    ///
    /// If `page` is out of bounds or the type `T` is incorrect.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn force_page<T: Slot>(
        &mut self,
        page_idx: PageIndex,
        ingredient: IngredientIndex,
        memo_types: &Arc<MemoTableTypes>,
    ) {
        let page = self.pages.get_mut(page_idx.0);

        match page {
            Some(page) => {
                // Initialize the page if was created using `push_uninit_page`.
                if page.slot_type_id == TypeId::of::<DummySlot>() {
                    *page = Page::new::<T>(ingredient, memo_types.clone());
                }

                // Ensure the page has the correct type.
                page.assert_type::<T>();
            }

            None => {
                // Create dummy pages until we reach the page we want.
                while self.page_count() < page_idx.as_usize() {
                    // We make sure not to claim any intermediary pages for ourselves, as they may
                    // be required by a different ingredient when it is deserialized.
                    self.push_uninit_page();
                }

                let allocated_idx = self.push_page::<T>(ingredient, memo_types.clone());
                assert_eq!(
                    allocated_idx, page_idx,
                    "allocated index does not match requested index"
                );
            }
        };
    }

    /// Allocate a new page for the given ingredient and with slots of type `T`
    #[inline]
    pub(crate) fn push_page<T: Slot>(
        &self,
        ingredient: IngredientIndex,
        memo_types: Arc<MemoTableTypes>,
    ) -> PageIndex {
        PageIndex::new(self.pages.push(Page::new::<T>(ingredient, memo_types)))
    }

    /// Allocate an uninitialized page.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn push_uninit_page(&self) -> PageIndex {
        // Note that `DummySlot` is a ZST, so the memory wasted by any pages of ingredients
        // that were not serialized should be negligible.
        PageIndex::new(self.pages.push(Page::new::<DummySlot>(
            IngredientIndex::new(0),
            Arc::new(MemoTableTypes::default()),
        )))
    }

    /// Get the memo table associated with `id` for the concrete type `T`.
    ///
    /// # Safety
    ///
    /// The parameter `current_revision` must be the current revision of the database
    /// owning this table.
    ///
    /// # Panics
    ///
    /// If `page` is out of bounds or the type `T` is incorrect.
    pub unsafe fn memos<T: Slot>(
        &self,
        id: Id,
        current_revision: Revision,
    ) -> MemoTableWithTypes<'_> {
        let (page, slot) = split_id(id);
        let page = self.pages[page.0].assert_type::<T>();
        let slot = &page.data()[slot.0];

        // SAFETY: The caller is required to pass the `current_revision`.
        let memos = unsafe { &*T::memos(slot, current_revision) };

        // SAFETY: The `Page` keeps the correct memo types.
        unsafe { page.0.memo_types.attach_memos(memos) }
    }

    /// Get the memo table associated with `id`.
    ///
    /// Unlike `Table::memos`, this does not require a concrete type, and instead uses dynamic
    /// dispatch.
    ///
    /// # Safety
    ///
    /// The parameter `current_revision` must be the current revision of the owner of database
    /// owning this table.
    pub unsafe fn dyn_memos(&self, id: Id, current_revision: Revision) -> MemoTableWithTypes<'_> {
        let (page, slot) = split_id(id);
        let page = &self.pages[page.0];
        // SAFETY: We supply a proper slot pointer and the caller is required to pass the `current_revision`.
        let memos = unsafe { &*(page.slot_vtable.memos)(page.get(slot), current_revision) };
        // SAFETY: The `Page` keeps the correct memo types.
        unsafe { page.memo_types.attach_memos(memos) }
    }

    /// Get the memo table associated with `id`
    pub(crate) fn memos_mut(&mut self, id: Id) -> MemoTableWithTypesMut<'_> {
        let (page, slot) = split_id(id);
        let page_index = page.0;
        let page = self
            .pages
            .get_mut(page_index)
            .unwrap_or_else(|| panic!("index `{page_index}` is uninitialized"));
        // SAFETY: We supply a proper slot pointer and the caller is required to pass the `current_revision`.
        let memos = unsafe { &mut *(page.slot_vtable.memos_mut)(page.get(slot)) };
        // SAFETY: The `Page` keeps the correct memo types.
        unsafe { page.memo_types.attach_memos_mut(memos) }
    }

    pub(crate) fn slots_of<T: Slot>(&self) -> impl Iterator<Item = (Id, &T)> + '_ {
        ErasedSlots {
            pages: self.pages.iter(),
            current_page: None,
            slot_type_id: TypeId::of::<T>(),
        }
        .map(|(id, slot)| {
            // SAFETY: `ErasedSlots` only visits pages whose `TypeId` matches `T` and only returns
            // pointers to initialized slots.
            (id, unsafe { &*slot.cast::<T>() })
        })
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn fetch_or_push_page<T: Slot>(
        &self,
        ingredient: IngredientIndex,
        memo_types: impl FnOnce() -> Arc<MemoTableTypes>,
    ) -> PageIndex {
        if let Some(page) = self.take_non_full_page(ingredient) {
            return page;
        }

        self.push_page::<T>(ingredient, memo_types())
    }

    fn take_non_full_page(&self, ingredient: IngredientIndex) -> Option<PageIndex> {
        self.non_full_pages
            .lock()
            .get_mut(&ingredient)
            .and_then(Vec::pop)
    }

    pub(crate) fn record_unfilled_page(&self, ingredient: IngredientIndex, page: PageIndex) {
        self.non_full_pages
            .lock()
            .entry(ingredient)
            .or_default()
            .push(page);
    }
}

struct ErasedSlots<'db> {
    pages: boxcar::vec::Iter<'db, Page>,
    current_page: Option<(PageIndex, &'db Page, std::ops::Range<usize>)>,
    slot_type_id: TypeId,
}

impl Iterator for ErasedSlots<'_> {
    type Item = (Id, *mut ());

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((page_index, page, slots)) = &mut self.current_page {
                if let Some(slot_index) = slots.next() {
                    let slot_index = SlotIndex::new(slot_index);
                    let id = make_id(*page_index, slot_index);

                    // SAFETY: `slot_index` is below the initialized length captured when the page
                    // became current, so the resulting pointer is within the page allocation.
                    let slot = unsafe {
                        page.data
                            .as_ptr()
                            .byte_add(slot_index.0 * page.slot_vtable.layout.size())
                    };

                    return Some((id, slot));
                }
            }

            self.current_page = self.pages.find_map(|(page_index, page)| {
                (page.slot_type_id == self.slot_type_id).then(|| {
                    (
                        PageIndex::new(page_index),
                        page,
                        0..page.allocated.load(Ordering::Acquire),
                    )
                })
            });
            self.current_page.as_ref()?;
        }
    }
}

impl<'db, T: Slot> PageView<'db, T> {
    #[inline]
    fn page_data(&self) -> &'db [PageDataEntry<T>] {
        let len = self.0.allocated.load(Ordering::Acquire);
        // SAFETY: `len` is the initialized length of the page
        unsafe { slice::from_raw_parts(self.0.data.cast::<PageDataEntry<T>>().as_ptr(), len) }
    }

    #[inline]
    fn data(&self) -> &'db [T] {
        let len = self.0.allocated.load(Ordering::Acquire);
        // SAFETY: `len` is the initialized length of the page
        unsafe { slice::from_raw_parts(self.0.data.cast::<T>().as_ptr(), len) }
    }

    /// Allocate a value in this page.
    ///
    /// # Safety
    ///
    /// The caller must be the unique writer to this page, i.e. `allocate` cannot be called
    /// concurrently by multiple threads. Concurrent readers however, are fine.
    #[inline]
    pub(crate) unsafe fn allocate<V>(&self, page: PageIndex, value: V) -> Result<(Id, &'db T), V>
    where
        V: FnOnce(Id) -> T,
    {
        let index = self.0.allocated.load(Ordering::Acquire);
        if index >= PAGE_LEN {
            return Err(value);
        }

        // Initialize entry `index`
        let id = make_id(page, SlotIndex::new(index));
        let data = self.0.data.cast::<PageDataEntry<T>>();

        // SAFETY: `index` is also guaranteed to be in bounds as per the check above.
        let entry = unsafe { &*data.as_ptr().add(index) };

        // SAFETY: The caller guarantees we are the unique writer, and readers will not attempt to
        // access this index until we have updated the length.
        unsafe { (*entry.get()).write(value(id)) };

        // SAFETY: We just initialized the value above.
        let value = unsafe { (*entry.get()).assume_init_ref() };

        // Update the length now that we have initialized the value.
        self.0.allocated.store(index + 1, Ordering::Release);

        Ok((id, value))
    }
}

impl Page {
    #[inline]
    fn new<T: Slot>(ingredient: IngredientIndex, memo_types: Arc<MemoTableTypes>) -> Self {
        #[cfg(not(feature = "shuttle"))]
        let data: Box<PageData<T>> =
            Box::new([const { UnsafeCell::new(MaybeUninit::uninit()) }; PAGE_LEN]);

        #[cfg(feature = "shuttle")]
        let data = {
            // Avoid stack overflows when using larger shuttle types.
            let data = (0..PAGE_LEN)
                .map(|_| UnsafeCell::new(MaybeUninit::uninit()))
                .collect::<Box<[PageDataEntry<T>]>>();

            let data: *mut [PageDataEntry<T>] = Box::into_raw(data);

            // SAFETY: `*mut PageDataEntry<T>` and `*mut [PageDataEntry<T>; N]` have the same layout.
            unsafe { Box::from_raw(data.cast::<PageDataEntry<T>>().cast::<PageData<T>>()) }
        };

        Self {
            ingredient,
            memo_types,
            slot_vtable: SlotVTable::of::<T>(),
            slot_type_id: TypeId::of::<T>(),
            allocated: AtomicUsize::new(0),
            data: NonNull::from(Box::leak(data)).cast::<()>(),
        }
    }

    /// Retrieves the pointer for the given slot.
    ///
    /// # Panics
    ///
    /// If slot is out of bounds
    fn get(&self, slot: SlotIndex) -> *mut () {
        let len = self.allocated.load(Ordering::Acquire);
        assert!(
            slot.0 < len,
            "out of bounds access `{slot:?}` (maximum slot `{len}`)"
        );

        // SAFETY: We have checked that the resulting pointer will be within bounds.
        unsafe {
            self.data
                .as_ptr()
                .byte_add(slot.0 * self.slot_vtable.layout.size())
        }
    }

    #[inline]
    fn assert_type<T: Slot>(&self) -> PageView<'_, T> {
        if self.slot_type_id != TypeId::of::<T>() {
            type_assert_failed::<T>(self);
        }

        PageView(self, PhantomData)
    }
}

/// This function is explicitly outlined to avoid debug machinery in the hot-path.
#[cold]
#[inline(never)]
fn type_assert_failed<T: 'static>(page: &Page) -> ! {
    panic!(
        "page has slot type `{:?}` but `{:?}` was expected",
        (page.slot_vtable.type_name)(),
        std::any::type_name::<T>(),
    )
}

impl Drop for Page {
    fn drop(&mut self) {
        let len = *self.allocated.get_mut();
        // SAFETY: We supply the data pointer and the initialized length
        unsafe { (self.slot_vtable.drop_impl)(self.data.as_ptr(), len, &self.memo_types) };
    }
}

/// A placeholder type representing the slots of an uninitialized `Page`.
struct DummySlot;

// SAFETY: The `DummySlot type is private.
unsafe impl Slot for DummySlot {
    unsafe fn memos(_: *const Self, _: Revision) -> *const MemoTable {
        unreachable!()
    }

    fn memos_mut(&mut self) -> &mut MemoTable {
        unreachable!()
    }
}

fn make_id(page: PageIndex, slot: SlotIndex) -> Id {
    let page = page.0 as u32;
    let slot = slot.0 as u32;
    // SAFETY: `slot` is guaranteed to be small enough that the resulting Id won't be bigger than `Id::MAX_U32`
    unsafe { Id::from_index((page << PAGE_LEN_BITS) | slot) }
}

#[inline]
pub fn split_id(id: Id) -> (PageIndex, SlotIndex) {
    let index = id.index() as usize;
    let slot = index & PAGE_LEN_MASK;
    let page = index >> PAGE_LEN_BITS;
    (PageIndex::new(page), SlotIndex::new(slot))
}
