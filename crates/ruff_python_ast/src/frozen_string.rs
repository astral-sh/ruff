//! Immutable string storage for [`crate::name::Name`].

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::mem;
use std::num::NonZeroU8;
use std::ptr;
use std::slice;
use std::str;
use std::sync::Arc;

/// The number of UTF-8 bytes stored directly inside a [`FrozenCompactString`].
pub(crate) const INLINE_CAPACITY: usize = 24;

// Short inline strings use an invalid UTF-8 trailing byte to encode lengths 0 through 23. A
// 24-byte UTF-8 string always ends in ASCII or a continuation byte, so its final byte is the tag.
const SHORT_INLINE_TAG_BASE: u8 = 0b1100_0000;
const SHORT_INLINE_TAG_MAX: u8 = 0b1101_0111;
const STATIC_TAG: u8 = 0b1111_1110;
const HEAP_TAG: u8 = 0b1111_1111;
const REPR_SIZE: usize = 24;
const TAIL_SIZE: usize = REPR_SIZE - mem::size_of::<*const ()>() - mem::size_of::<usize>() - 1;

/// A 24-byte immutable string with 24 bytes of inline storage and shared heap allocations.
#[repr(transparent)]
pub(crate) struct FrozenCompactString(FrozenRepr);

/// A data pointer, length, byte tail, and niche-bearing final byte.
///
/// Inline values reinterpret all 24 bytes as string storage. Heap values retain the data pointer
/// returned by `Arc<[u8]>::into_raw`; static values retain a borrowed pointer and length.
#[repr(C)]
struct FrozenRepr(*const (), usize, [u8; TAIL_SIZE], NonZeroU8);

#[cfg(target_pointer_width = "64")]
#[repr(C, align(8))]
struct FrozenInline([u8; INLINE_CAPACITY - 1], NonZeroU8);

#[cfg(target_pointer_width = "32")]
#[repr(C, align(4))]
struct FrozenInline([u8; INLINE_CAPACITY - 1], NonZeroU8);

#[cfg(not(any(target_pointer_width = "32", target_pointer_width = "64")))]
compile_error!("FrozenCompactString only supports 32-bit and 64-bit targets");

const _: [(); REPR_SIZE] = [(); mem::size_of::<FrozenRepr>()];
const _: [(); REPR_SIZE] = [(); mem::size_of::<FrozenInline>()];
const _: [(); mem::align_of::<FrozenRepr>()] = [(); mem::align_of::<FrozenInline>()];
const _: [(); REPR_SIZE] = [(); mem::size_of::<FrozenCompactString>()];
const _: [(); REPR_SIZE] = [(); mem::size_of::<Option<FrozenCompactString>>()];

impl FrozenCompactString {
    #[inline]
    pub(crate) fn new(text: impl AsRef<str>) -> Self {
        Self(FrozenRepr::new(text.as_ref()))
    }

    #[inline]
    pub(crate) const fn new_static(text: &'static str) -> Self {
        Self(FrozenRepr::new_static(text))
    }

    #[inline]
    pub(crate) fn concat(left: &str, right: &str) -> Self {
        Self(FrozenRepr::concat(left, right))
    }

    #[inline]
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    #[inline]
    #[cfg(any(feature = "get-size", test))]
    pub(crate) fn is_heap_allocated(&self) -> bool {
        self.0.is_heap_allocated()
    }
}

impl FrozenRepr {
    #[inline]
    fn new(text: &str) -> Self {
        match text.len() {
            0..INLINE_CAPACITY => Self::new_short_inline(text),
            INLINE_CAPACITY if text.as_bytes()[INLINE_CAPACITY - 1] != 0 => {
                Self::new_full_inline(text)
            }
            _ => Self::new_heap(text),
        }
    }

    #[inline]
    const fn new_static(text: &'static str) -> Self {
        Self(
            text.as_ptr().cast::<()>(),
            text.len(),
            [0; TAIL_SIZE],
            nonzero(STATIC_TAG),
        )
    }

    #[inline]
    fn concat(left: &str, right: &str) -> Self {
        let len = left
            .len()
            .checked_add(right.len())
            .expect("combined string is too large");

        if len <= INLINE_CAPACITY {
            let mut bytes = [0; INLINE_CAPACITY];
            let (left_bytes, right_bytes) = bytes[..len].split_at_mut(left.len());
            left_bytes.copy_from_slice(left.as_bytes());
            right_bytes.copy_from_slice(right.as_bytes());

            if len < INLINE_CAPACITY {
                let len = u8::try_from(len).expect("inline strings are shorter than 24 bytes");
                bytes[INLINE_CAPACITY - 1] = SHORT_INLINE_TAG_BASE + len;
                return Self::from_inline_bytes(bytes);
            }
            if bytes[INLINE_CAPACITY - 1] != 0 {
                return Self::from_inline_bytes(bytes);
            }
        }

        Self::new_heap_concat(left, right)
    }

    #[inline]
    fn new_short_inline(text: &str) -> Self {
        debug_assert!(text.len() < INLINE_CAPACITY);
        let mut bytes = [0; INLINE_CAPACITY];
        bytes[..text.len()].copy_from_slice(text.as_bytes());
        let len = u8::try_from(text.len()).expect("inline strings are shorter than 24 bytes");
        bytes[INLINE_CAPACITY - 1] = SHORT_INLINE_TAG_BASE + len;
        Self::from_inline_bytes(bytes)
    }

    #[inline]
    fn new_full_inline(text: &str) -> Self {
        debug_assert_eq!(text.len(), INLINE_CAPACITY);
        debug_assert_ne!(text.as_bytes()[INLINE_CAPACITY - 1], 0);
        let mut bytes = [0; INLINE_CAPACITY];
        bytes.copy_from_slice(text.as_bytes());
        Self::from_inline_bytes(bytes)
    }

    #[inline]
    #[expect(
        unsafe_code,
        reason = "the inline and tagged representations have identical layouts"
    )]
    fn from_inline_bytes(bytes: [u8; INLINE_CAPACITY]) -> Self {
        debug_assert_ne!(bytes[INLINE_CAPACITY - 1], 0);

        // SAFETY: The byte array and `FrozenRepr` have identical sizes. The final byte is nonzero,
        // and raw pointers permit any bit pattern for inline values.
        unsafe { mem::transmute(bytes) }
    }

    #[cold]
    fn new_heap(text: &str) -> Self {
        Self::from_heap_storage(Arc::from(text.as_bytes()))
    }

    #[cold]
    #[expect(
        unsafe_code,
        reason = "the new Arc allocation is initialized from two valid string slices"
    )]
    fn new_heap_concat(left: &str, right: &str) -> Self {
        let len = left
            .len()
            .checked_add(right.len())
            .expect("combined string is too large");
        let mut storage = Arc::<[u8]>::new_uninit_slice(len);
        let destination = Arc::get_mut(&mut storage)
            .expect("a newly allocated Arc has no other strong or weak references")
            .as_mut_ptr()
            .cast::<u8>();

        // SAFETY: `destination` points to `len` uninitialized bytes in a new allocation. The two
        // source slices contain exactly `len` initialized bytes and cannot overlap that allocation.
        unsafe {
            ptr::copy_nonoverlapping(left.as_ptr(), destination, left.len());
            ptr::copy_nonoverlapping(right.as_ptr(), destination.add(left.len()), right.len());
        }

        // SAFETY: The two copies above initialized every byte in the allocation.
        Self::from_heap_storage(unsafe { storage.assume_init() })
    }

    #[inline]
    fn from_heap_storage(storage: Arc<[u8]>) -> Self {
        let len = storage.len();
        Self(
            Arc::into_raw(storage).cast::<u8>().cast::<()>(),
            len,
            [0; TAIL_SIZE],
            nonzero(HEAP_TAG),
        )
    }

    #[inline]
    fn tag(&self) -> u8 {
        self.3.get()
    }

    #[inline]
    fn is_heap_allocated(&self) -> bool {
        self.tag() == HEAP_TAG
    }

    #[inline]
    fn is_static(&self) -> bool {
        self.tag() == STATIC_TAG
    }

    #[inline]
    fn inline_len(&self) -> usize {
        debug_assert!(!self.is_heap_allocated() && !self.is_static());
        if self.tag() < SHORT_INLINE_TAG_BASE {
            INLINE_CAPACITY
        } else {
            debug_assert!(self.tag() <= SHORT_INLINE_TAG_MAX);
            usize::from(self.tag() - SHORT_INLINE_TAG_BASE)
        }
    }

    #[inline]
    #[expect(
        unsafe_code,
        reason = "the tagged representation stores inline bytes, a raw Arc slice, or static data"
    )]
    fn as_str(&self) -> &str {
        let bytes = if self.is_heap_allocated() {
            // SAFETY: The pointer and length came from an `Arc<[u8]>` that this value owns one
            // strong reference to, and the allocation remains immutable for its lifetime.
            unsafe { &*self.raw_arc_ptr() }
        } else if self.is_static() {
            // SAFETY: The pointer and length came from a valid `&'static str`.
            unsafe { slice::from_raw_parts(self.0.cast::<u8>(), self.1) }
        } else {
            // SAFETY: Inline bytes begin at the same address as the representation, and
            // `inline_len` is in 0..=24 by construction.
            unsafe { slice::from_raw_parts(ptr::from_ref(self).cast::<u8>(), self.inline_len()) }
        };

        // SAFETY: Every representation is created exclusively from valid `str` values and exposes
        // no mutation that could violate UTF-8.
        unsafe { str::from_utf8_unchecked(bytes) }
    }

    #[inline]
    fn raw_arc_ptr(&self) -> *const [u8] {
        debug_assert!(self.is_heap_allocated());
        ptr::slice_from_raw_parts(self.0.cast::<u8>(), self.1)
    }
}

#[expect(
    unsafe_code,
    reason = "all representation tags are statically constrained to nonzero values"
)]
const fn nonzero(byte: u8) -> NonZeroU8 {
    debug_assert!(byte != 0);

    // SAFETY: Callers only pass the nonzero static and heap tag constants.
    unsafe { NonZeroU8::new_unchecked(byte) }
}

impl Clone for FrozenRepr {
    #[inline]
    #[expect(
        unsafe_code,
        reason = "cloning shared storage increments the strong count of its raw Arc pointer"
    )]
    fn clone(&self) -> Self {
        if self.is_heap_allocated() {
            // SAFETY: `raw_arc_ptr` reconstructs the pointer returned by `Arc::into_raw`, and this
            // value owns a live strong reference for the duration of this call.
            unsafe { Arc::increment_strong_count(self.raw_arc_ptr()) };
        }

        Self(self.0, self.1, self.2, self.3)
    }
}

impl Drop for FrozenRepr {
    #[inline]
    #[expect(
        unsafe_code,
        reason = "dropping shared storage releases the strong count of its raw Arc pointer"
    )]
    fn drop(&mut self) {
        if self.is_heap_allocated() {
            // SAFETY: Each heap representation owns exactly one strong reference corresponding to
            // this raw pointer. Reconstructing and dropping the Arc releases that reference.
            unsafe { drop(Arc::from_raw(self.raw_arc_ptr())) };
        }
    }
}

// SAFETY: Inline values contain owned bytes. Heap values hold a strong reference to immutable
// `Arc<[u8]>` storage, and static values point to immutable data.
#[expect(
    unsafe_code,
    reason = "the raw pointer owns or borrows immutable storage"
)]
unsafe impl Send for FrozenRepr {}
// SAFETY: See the `Send` implementation above; no API exposes mutation of shared storage.
#[expect(
    unsafe_code,
    reason = "the raw pointer owns or borrows immutable storage"
)]
unsafe impl Sync for FrozenRepr {}

impl Clone for FrozenCompactString {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Default for FrozenCompactString {
    fn default() -> Self {
        Self::new("")
    }
}

impl Hash for FrozenCompactString {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialEq for FrozenCompactString {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for FrozenCompactString {}

impl PartialOrd for FrozenCompactString {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FrozenCompactString {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.as_str().cmp(other.as_str())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::{FrozenCompactString, INLINE_CAPACITY};

    const STATIC: FrozenCompactString =
        FrozenCompactString::new_static("a static string longer than inline storage");

    #[test]
    fn representation_is_24_bytes() {
        assert_eq!(size_of::<FrozenCompactString>(), 24);
        assert_eq!(size_of::<Option<FrozenCompactString>>(), 24);
    }

    #[test]
    fn inline_heap_and_static_strings_roundtrip() {
        for len in 0..=INLINE_CAPACITY {
            let text = "x".repeat(len);
            let frozen = FrozenCompactString::new(&text);
            assert_eq!(frozen.as_str(), text);
            assert!(!frozen.is_heap_allocated());
        }

        for text in [
            "x".repeat(INLINE_CAPACITY + 1),
            format!("{}\0", "x".repeat(INLINE_CAPACITY - 1)),
            String::from("Python 🐍 and Rust 🦀"),
        ] {
            let frozen = FrozenCompactString::new(&text);
            assert_eq!(frozen.as_str(), text);
            assert!(frozen.is_heap_allocated());
        }

        assert_eq!(
            STATIC.as_str(),
            "a static string longer than inline storage"
        );
        assert!(!STATIC.is_heap_allocated());
    }

    #[test]
    fn heap_clones_share_storage() {
        let frozen = FrozenCompactString::new("a string too long for inline storage");
        let clone = frozen.clone();
        assert_eq!(frozen.as_str().as_ptr(), clone.as_str().as_ptr());

        drop(frozen);
        assert_eq!(clone.as_str(), "a string too long for inline storage");
    }

    #[test]
    fn concatenated_strings_roundtrip() {
        for (left, right, is_heap_allocated) in [
            ("", "short", false),
            ("abcdefghijk", "lmnopqrstuvwx", false),
            ("abcdefghijkl", "mnopqrstuvwx", false),
            ("abcdefghijkl", "mnopqrstuvwxy", true),
            ("Python 🐍", " and Rust 🦀", true),
        ] {
            let expected = format!("{left}{right}");
            let frozen = FrozenCompactString::concat(left, right);
            assert_eq!(frozen.as_str(), expected);
            assert_eq!(frozen.is_heap_allocated(), is_heap_allocated);

            let clone = frozen.clone();
            assert_eq!(clone.as_str(), expected);
            if is_heap_allocated {
                assert_eq!(frozen.as_str().as_ptr(), clone.as_str().as_ptr());
            }

            drop(frozen);
            assert_eq!(clone.as_str(), expected);
        }
    }

    #[test]
    fn equality_hash_and_order_match_str() {
        let alpha = FrozenCompactString::new("alpha");
        let beta = FrozenCompactString::new("beta value too long to store inline");
        assert!(alpha < beta);
        assert!(alpha == FrozenCompactString::new("alpha"));

        let mut expected = DefaultHasher::new();
        "alpha".hash(&mut expected);
        let mut actual = DefaultHasher::new();
        alpha.hash(&mut actual);
        assert_eq!(actual.finish(), expected.finish());
    }
}
