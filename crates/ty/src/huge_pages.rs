//! Request transparent huge pages for large jemalloc extents in one-shot checks.
//!
//! Backing the heap with larger pages can reduce page faults and address translation costs.
//! It can also increase resident memory for sparsely used allocations. Limit the hint to
//! large extents in short-lived processes where the type checker builds up a large working set.

use std::ffi::{CStr, CString, c_char, c_uint, c_void};
use std::mem::MaybeUninit;
use std::ptr;
use std::sync::OnceLock;
use tikv_jemalloc_sys::{extent_hooks_t, mallctl};

struct Hooks {
    original: &'static extent_hooks_t,
    replacement: extent_hooks_t,
}

static HOOKS: OnceLock<Hooks> = OnceLock::new();

/// Configure the automatic arenas before the check workers start. Jemalloc
/// chooses its metadata hooks when an arena is first initialized.
#[expect(unsafe_code)]
pub(super) fn enable(workers: usize) {
    // Custom hooks can interfere with non-default settings such as guard
    // pages. Leave explicitly configured allocators alone.
    // SAFETY: jemalloc's per-binary configuration is initialized before main and
    // remains unchanged. Check both prefixed and unprefixed configuration sources.
    if unsafe { tikv_jemalloc_sys::malloc_conf.is_some() }
        || std::env::var_os("_RJEM_MALLOC_CONF").is_some()
        || std::env::var_os("MALLOC_CONF").is_some()
        || ["/etc/_rjem_malloc.conf", "/etc/malloc.conf"]
            .iter()
            .any(|path| {
                !std::fs::symlink_metadata(path)
                    .is_err_and(|err| err.kind() == std::io::ErrorKind::NotFound)
            })
        || !std::fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled")
            .is_ok_and(|setting| setting.contains("[madvise]"))
    {
        return;
    }

    // SAFETY: jemalloc reports the compiled-in configuration as a C string.
    let Some(config) = (unsafe { read::<*const c_char>(c"config.malloc_conf") }) else {
        return;
    };
    if config.is_null() || !unsafe { CStr::from_ptr(config) }.is_empty() {
        return;
    }

    // SAFETY: jemalloc returns a pointer to a static C string for `opt.thp`.
    let Some(policy) = (unsafe { read::<*const c_char>(c"opt.thp") }) else {
        return;
    };
    // Respect explicit huge page settings and avoid interfering with jemalloc's
    // optional huge page allocator, which has its own extent management.
    // SAFETY: `policy` is a valid C string and `opt.hpa` has type `bool`.
    if policy.is_null()
        || unsafe {
            CStr::from_ptr(policy) != c"default" || read::<bool>(c"opt.hpa") != Some(false)
        }
    {
        return;
    }

    // The main thread uses arena zero. By default jemalloc assigns new
    // threads to unused automatic arenas. Initializing them here also covers
    // metadata allocations, without changing how oversized requests are routed.
    // SAFETY: `opt.narenas` has type `unsigned`.
    let Some(arenas) = (unsafe { read::<c_uint>(c"opt.narenas") }) else {
        return;
    };
    for arena in (1..arenas).take(workers) {
        let Ok(key) = CString::new(format!("arena.{arena}.extent_hooks")) else {
            return;
        };
        // SAFETY: This binary uses the default hooks, which have static lifetime
        // and ignore the hooks pointer passed to their callbacks.
        let Some(original) =
            (unsafe { read::<*const extent_hooks_t>(&key).and_then(|hooks| hooks.as_ref()) })
        else {
            return;
        };
        let hooks = HOOKS.get_or_init(|| Hooks {
            original,
            replacement: extent_hooks_t {
                alloc: Some(allocate),
                ..*original
            },
        });
        if ptr::eq(original, hooks.original) {
            let mut replacement = &hooks.replacement;
            // SAFETY: This option expects a pointer to the new hooks. jemalloc
            // only reads the hook table and it lives for the rest of the process.
            unsafe {
                mallctl(
                    key.as_ptr(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::from_mut(&mut replacement).cast(),
                    size_of::<&extent_hooks_t>(),
                );
            }
        }
    }
}

/// The caller must use the documented C type for the given key.
#[expect(unsafe_code)]
unsafe fn read<T: Copy>(key: &CStr) -> Option<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let mut len = size_of::<T>();
    // SAFETY: The caller supplied the correct type. A successful read
    // initializes the provided output buffer.
    let result = unsafe {
        mallctl(
            key.as_ptr(),
            value.as_mut_ptr().cast(),
            &raw mut len,
            ptr::null_mut(),
            0,
        )
    };
    (result == 0 && len == size_of::<T>()).then(|| unsafe { value.assume_init() })
}

#[expect(unsafe_code)]
unsafe extern "C" fn allocate(
    _: *mut extent_hooks_t,
    addr: *mut c_void,
    size: usize,
    alignment: usize,
    zero: *mut bool,
    commit: *mut bool,
    arena: c_uint,
) -> *mut c_void {
    const HUGE_PAGE: usize = 2 * 1024 * 1024;

    // The allocator can invoke this while holding arena locks. Never allocate
    // or wait here. HOOKS is initialized before installing the callback.
    let Some(hooks) = HOOKS.get() else {
        return ptr::null_mut();
    };
    let Some(original) = hooks.original.alloc else {
        return ptr::null_mut();
    };
    // SAFETY: Pass through jemalloc's arguments to its default allocator.
    let ptr = unsafe {
        original(
            ptr::from_ref(hooks.original).cast_mut(),
            addr,
            size,
            alignment,
            zero,
            commit,
            arena,
        )
    };
    // Leave the small initial growth extents unhinted. Metadata extents
    // already request huge-page alignment and can benefit after the first one.
    let large = size >= 4 * HUGE_PAGE || (size >= 2 * HUGE_PAGE && alignment >= HUGE_PAGE);
    if !ptr.is_null() && large {
        // SAFETY: The allocator just mapped this memory. The hint doesn't
        // change the contents or protection of any pages.
        unsafe { libc::madvise(ptr, size, libc::MADV_HUGEPAGE) };
    }
    ptr
}
