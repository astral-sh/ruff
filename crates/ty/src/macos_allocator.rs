//! Select one allocator for the lifetime of the standalone ty process.
#![expect(unsafe_code, reason = "Darwin startup arguments and GlobalAlloc")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::ffi::CStr;
use std::sync::atomic::{AtomicU8, Ordering::Relaxed};

use tikv_jemallocator::Jemalloc;

#[global_allocator]
static GLOBAL: Allocator = Allocator;

struct Allocator;

/// Only opt in for a literal check. Argument files can introduce watch flags.
fn one_shot(mut args: impl Iterator<Item = impl AsRef<[u8]>>) -> bool {
    args.next().is_some_and(|arg| arg.as_ref() == b"check")
        && args.all(|arg| {
            let arg = arg.as_ref();
            !(arg.starts_with(b"@")
                || arg == b"--watch"
                || (arg.starts_with(b"-") && !arg.starts_with(b"--") && arg.contains(&b'W')))
        })
}

/// Read the launch arguments without allocating or reentering the allocator.
#[cold]
fn select() -> bool {
    // SAFETY: Darwin provides argc pointers to NUL-terminated arguments.
    // The standalone executable does not mutate argv. Borrow it only here.
    unsafe {
        let (Some(&argc), Some(&argv)) = (libc::_NSGetArgc().as_ref(), libc::_NSGetArgv().as_ref())
        else {
            return false;
        };
        let argc = usize::try_from(argc).unwrap_or(0);
        if argc < 2 || argv.is_null() {
            return false;
        }
        let args = std::slice::from_raw_parts(argv, argc);
        if args.iter().any(|arg| arg.is_null()) {
            return false;
        }
        one_shot(args[1..].iter().map(|&arg| CStr::from_ptr(arg).to_bytes()))
    }
}

/// Fix the allocator on the first call, which can happen before `main`.
#[inline]
fn uses_jemalloc() -> bool {
    const UNSET: u8 = 0;
    const SYSTEM: u8 = 1;
    const JEMALLOC: u8 = 2;
    static CHOICE: AtomicU8 = AtomicU8::new(UNSET);

    let mut choice = CHOICE.load(Relaxed);
    if choice == UNSET {
        let candidate = if select() { JEMALLOC } else { SYSTEM };
        // Racing first allocations must use the winner. No other data is published.
        choice = match CHOICE.compare_exchange(UNSET, candidate, Relaxed, Relaxed) {
            Ok(_) => candidate,
            Err(winner) => winner,
        };
    }
    choice == JEMALLOC
}

// Keep these methods out of line to avoid duplicating both paths at each allocation site.
//
// SAFETY: The backend is fixed before the first allocation, even before main.
// Every operation forwards the original pointer, layout and size to its owner.
unsafe impl GlobalAlloc for Allocator {
    #[inline(never)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if uses_jemalloc() {
            unsafe { Jemalloc.alloc(layout) }
        } else {
            unsafe { System.alloc(layout) }
        }
    }

    #[inline(never)]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if uses_jemalloc() {
            unsafe { Jemalloc.alloc_zeroed(layout) }
        } else {
            unsafe { System.alloc_zeroed(layout) }
        }
    }

    #[inline(never)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if uses_jemalloc() {
            unsafe { Jemalloc.dealloc(ptr, layout) }
        } else {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    #[inline(never)]
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if uses_jemalloc() {
            unsafe { Jemalloc.realloc(ptr, layout, size) }
        } else {
            unsafe { System.realloc(ptr, layout, size) }
        }
    }
}

#[test]
fn classify_commands() {
    assert!(one_shot(["check", "-v", "src"].into_iter()));
    assert!(!one_shot(["server"].into_iter()));
    assert!(!one_shot(["@args"].into_iter()));
    for flag in ["@args", "--watch", "-W", "-vW"] {
        assert!(!one_shot(["check", flag].into_iter()));
    }
}
