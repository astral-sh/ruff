// RUSTFLAGS="--cfg loom" cargo test -p char_str --test loom --release --features loom -- --test-threads=1
#![cfg(loom)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering::Relaxed};

use char_str::CharStr;
use loom::thread;

// Avoid matching the sizes of Loom's internal allocations.
const TEXT_LEN: usize = 4099;
const HEAP_ALLOCATION_SIZE: usize = size_of::<loom::sync::atomic::AtomicUsize>() + TEXT_LEN;

struct TrackingAllocator;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

static TRACK_ALLOCATIONS: AtomicBool = AtomicBool::new(false);
static SAW_HEAP_ALLOCATION: AtomicBool = AtomicBool::new(false);
static MATCHING_ALLOCATIONS: AtomicIsize = AtomicIsize::new(0);

#[expect(
    unsafe_code,
    reason = "the global allocator forwards valid layouts while tracking the heap buffer"
)]
unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The layout is forwarded unchanged to the system allocator.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null()
            && TRACK_ALLOCATIONS.load(Relaxed)
            && layout.size() == HEAP_ALLOCATION_SIZE
        {
            SAW_HEAP_ALLOCATION.store(true, Relaxed);
            MATCHING_ALLOCATIONS.fetch_add(1, Relaxed);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if TRACK_ALLOCATIONS.load(Relaxed) && layout.size() == HEAP_ALLOCATION_SIZE {
            MATCHING_ALLOCATIONS.fetch_sub(1, Relaxed);
        }
        // SAFETY: The pointer and layout are forwarded unchanged to the system allocator.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[test]
fn concurrent_clone_and_drop() {
    loom::model(|| {
        let text = "a".repeat(TEXT_LEN);
        SAW_HEAP_ALLOCATION.store(false, Relaxed);
        MATCHING_ALLOCATIONS.store(0, Relaxed);
        TRACK_ALLOCATIONS.store(true, Relaxed);
        {
            let original = CharStr::from(text.as_str());
            let shared = original.clone();

            let thread = thread::spawn(move || {
                let clone = shared.clone();
                assert_eq!(clone.len(), TEXT_LEN);
                assert!(clone.bytes().all(|byte| byte == b'a'));
            });

            let clone = original.clone();
            assert_eq!(clone.len(), TEXT_LEN);
            assert!(clone.bytes().all(|byte| byte == b'a'));
            thread.join().unwrap();
        }

        TRACK_ALLOCATIONS.store(false, Relaxed);
        assert!(SAW_HEAP_ALLOCATION.load(Relaxed));
        assert_eq!(MATCHING_ALLOCATIONS.load(Relaxed), 0);
    });
}
