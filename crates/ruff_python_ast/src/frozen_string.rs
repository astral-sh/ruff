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
const INLINE_CAPACITY: usize = 15;

// Heap tags encode the most significant length byte plus one, reserving zero as a niche. Inline
// tags start above the heap range and encode lengths 0 through 15.
const HEAP_TAG_MAX: u8 = 128;
const INLINE_TAG_BASE: u8 = 193;
const REPR_SIZE: usize = 16;
const TAIL_SIZE: usize = REPR_SIZE - mem::size_of::<*const ()>() - 1;

/// A 16-byte immutable string with 15 bytes of inline storage and shared heap allocations.
#[repr(transparent)]
pub(crate) struct FrozenCompactString(FrozenRepr);

/// A pointer-sized field, byte tail, and niche-bearing final byte.
///
/// Inline values reinterpret the first 15 bytes as string storage. Heap values retain the data
/// pointer returned by `Arc<[u8]>::into_raw`; the tail and final byte encode the slice length.
#[repr(C)]
struct FrozenRepr(*const (), [u8; TAIL_SIZE], NonZeroU8);

#[cfg(target_pointer_width = "64")]
#[repr(C, align(8))]
struct FrozenInline([u8; INLINE_CAPACITY], NonZeroU8);

#[cfg(target_pointer_width = "32")]
#[repr(C, align(4))]
struct FrozenInline([u8; INLINE_CAPACITY], NonZeroU8);

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
        if text.len() <= INLINE_CAPACITY {
            Self::new_inline(text)
        } else {
            Self::new_heap(text)
        }
    }

    #[inline]
    fn concat(left: &str, right: &str) -> Self {
        let len = left
            .len()
            .checked_add(right.len())
            .expect("combined string is too large");
        if len <= INLINE_CAPACITY {
            Self::new_inline_concat(left, right, len)
        } else {
            Self::new_heap_concat(left, right)
        }
    }

    #[inline]
    #[expect(
        unsafe_code,
        reason = "the inline and tagged representations have identical layouts"
    )]
    fn new_inline(text: &str) -> Self {
        let len = u8::try_from(text.len()).expect("inline strings are at most 15 bytes");
        let mut inline = FrozenInline([0; INLINE_CAPACITY], tag(INLINE_TAG_BASE + len));
        inline.0[..text.len()].copy_from_slice(text.as_bytes());

        // SAFETY: `FrozenInline` and `FrozenRepr` have identical size and alignment. The final
        // byte is nonzero, and raw pointers permit any bit pattern for the inline case.
        unsafe { mem::transmute(inline) }
    }

    #[inline]
    #[expect(
        unsafe_code,
        reason = "the inline and tagged representations have identical layouts"
    )]
    fn new_inline_concat(left: &str, right: &str, len: usize) -> Self {
        let len = u8::try_from(len).expect("inline strings are at most 15 bytes");
        let mut inline = FrozenInline([0; INLINE_CAPACITY], tag(INLINE_TAG_BASE + len));
        let (left_bytes, right_bytes) = inline.0[..usize::from(len)].split_at_mut(left.len());
        left_bytes.copy_from_slice(left.as_bytes());
        right_bytes.copy_from_slice(right.as_bytes());

        // SAFETY: `FrozenInline` and `FrozenRepr` have identical size and alignment. The final
        // byte is nonzero, and raw pointers permit any bit pattern for the inline case.
        unsafe { mem::transmute(inline) }
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
        let len_bytes = storage.len().to_le_bytes();
        let high_byte_index = len_bytes.len() - 1;
        let mut tail = [0; TAIL_SIZE];
        tail[..high_byte_index].copy_from_slice(&len_bytes[..high_byte_index]);

        // Rust requires slice allocations to be no larger than `isize::MAX`, so incrementing the
        // high byte produces a nonzero tag in the heap range.
        debug_assert!(len_bytes[high_byte_index] < HEAP_TAG_MAX);
        Self(
            Arc::into_raw(storage).cast::<u8>().cast::<()>(),
            tail,
            tag(len_bytes[high_byte_index] + 1),
        )
    }

    #[inline]
    fn tag(&self) -> u8 {
        self.2.get()
    }

    #[inline]
    fn is_heap_allocated(&self) -> bool {
        self.tag() <= HEAP_TAG_MAX
    }

    #[inline]
    fn heap_len(&self) -> usize {
        debug_assert!(self.is_heap_allocated());

        let mut bytes = 0usize.to_le_bytes();
        let high_byte_index = bytes.len() - 1;
        bytes[..high_byte_index].copy_from_slice(&self.1[..high_byte_index]);
        bytes[high_byte_index] = self.tag() - 1;
        usize::from_le_bytes(bytes)
    }

    #[inline]
    #[expect(
        unsafe_code,
        reason = "the tagged representation stores either inline bytes or a raw Arc slice"
    )]
    fn as_str(&self) -> &str {
        let bytes = if self.is_heap_allocated() {
            // SAFETY: The pointer and encoded length came from an `Arc<[u8]>` that this value owns
            // one strong reference to, and the allocation remains immutable for its lifetime.
            unsafe { &*self.raw_arc_ptr() }
        } else {
            let len = self.tag() as usize - INLINE_TAG_BASE as usize;
            // SAFETY: Inline bytes begin at the same address as the representation, and `len` is
            // in 0..=15 by construction.
            unsafe { slice::from_raw_parts(ptr::from_ref(self).cast::<u8>(), len) }
        };

        // SAFETY: Both representations are created exclusively from valid `str` values and expose
        // no mutation that could violate UTF-8.
        unsafe { str::from_utf8_unchecked(bytes) }
    }

    #[inline]
    fn raw_arc_ptr(&self) -> *const [u8] {
        debug_assert!(self.is_heap_allocated());
        ptr::slice_from_raw_parts(self.0.cast::<u8>(), self.heap_len())
    }
}

#[inline]
#[expect(
    unsafe_code,
    reason = "all representation tags are statically constrained to nonzero ranges"
)]
fn tag(byte: u8) -> NonZeroU8 {
    debug_assert!(byte <= HEAP_TAG_MAX || (INLINE_TAG_BASE..=INLINE_TAG_BASE + 15).contains(&byte));

    // SAFETY: Callers only pass 1..=128 or 193..=208.
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

        Self(self.0, self.1, self.2)
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

// SAFETY: Inline values contain owned bytes. Heap values hold a strong reference to an immutable
// `Arc<[u8]>`, which is Send and Sync.
#[expect(unsafe_code, reason = "the raw pointer owns immutable Arc storage")]
unsafe impl Send for FrozenRepr {}
// SAFETY: See the `Send` implementation above; no API exposes mutation of shared storage.
#[expect(unsafe_code, reason = "the raw pointer owns immutable Arc storage")]
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

impl From<String> for FrozenCompactString {
    #[inline]
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::{FrozenCompactString, INLINE_CAPACITY};

    #[test]
    fn representation_is_16_bytes() {
        assert_eq!(size_of::<FrozenCompactString>(), 16);
        assert_eq!(size_of::<Option<FrozenCompactString>>(), 16);
    }

    #[test]
    fn inline_and_heap_strings_roundtrip() {
        for len in 0..=INLINE_CAPACITY {
            let text = "x".repeat(len);
            let frozen = FrozenCompactString::new(&text);
            assert_eq!(frozen.as_str(), text);
            assert!(!frozen.is_heap_allocated());
        }

        for text in [
            "x".repeat(INLINE_CAPACITY + 1),
            String::from("Python 🐍 and Rust 🦀"),
        ] {
            let frozen = FrozenCompactString::new(&text);
            assert_eq!(frozen.as_str(), text);
            assert!(frozen.is_heap_allocated());
        }
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
            ("abcdefg", "hijklmno", false),
            ("abcdefgh", "ijklmnop", true),
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
