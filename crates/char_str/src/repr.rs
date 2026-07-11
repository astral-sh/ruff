use core::{mem, ptr, slice, str};

#[cfg(not(loom))]
use core::sync::atomic::Ordering::Relaxed;
#[cfg(loom)]
use loom::sync::atomic::Ordering::Relaxed;

use crate::ReserveError;

mod heap_buffer;
use heap_buffer::HeapBuffer;

mod inline_buffer;
use inline_buffer::InlineBuffer;

mod last_byte;
use last_byte::LastByte;

mod static_buffer;
use static_buffer::StaticBuffer;

const MAX_INLINE_SIZE: usize = 2 * size_of::<usize>();

#[repr(C)]
#[cfg(target_pointer_width = "64")]
pub(crate) struct Repr(*const (), [u8; 7], LastByte);

#[repr(C)]
#[cfg(target_pointer_width = "32")]
pub(crate) struct Repr(*const (), [u8; 3], LastByte);

const _: () = {
    assert!(size_of::<Repr>() == MAX_INLINE_SIZE);
    assert!(size_of::<Option<Repr>>() == MAX_INLINE_SIZE);
    assert!(align_of::<Repr>() == align_of::<usize>());
    assert!(align_of::<Option<Repr>>() == align_of::<usize>());
};

impl Repr {
    #[inline]
    pub(crate) const fn new() -> Self {
        Self::from_inline(InlineBuffer::empty())
    }

    #[inline]
    pub(crate) fn from_str(text: &str) -> Result<Self, ReserveError> {
        if text.len() <= MAX_INLINE_SIZE {
            // SAFETY: The length was checked above.
            Ok(Self::from_inline(unsafe { InlineBuffer::new(text) }))
        } else {
            HeapBuffer::new(text).map(Self::from_heap)
        }
    }

    #[inline]
    pub(crate) fn from_slices(slices: &[&str]) -> Result<Self, ReserveError> {
        Self::from_iter(slices.iter().copied())
    }

    #[inline]
    pub(crate) fn from_joined_slices<T: AsRef<str>>(
        slices: &[T],
        separator: &str,
    ) -> Result<Self, ReserveError> {
        let slices = slices.iter().enumerate().flat_map(move |(index, text)| {
            [(index > 0).then_some(separator), Some(text.as_ref())]
                .into_iter()
                .flatten()
        });
        Self::from_iter(slices)
    }

    fn from_iter<'a>(slices: impl Iterator<Item = &'a str> + Clone) -> Result<Self, ReserveError> {
        let text_len = slices.clone().try_fold(0usize, |len, text| {
            len.checked_add(text.len()).ok_or(ReserveError)
        })?;

        if text_len <= MAX_INLINE_SIZE {
            // SAFETY: `text_len` is the checked sum of every slice length and fits inline.
            Ok(Self::from_inline(unsafe {
                InlineBuffer::from_iter(slices, text_len)
            }))
        } else {
            // SAFETY: `text_len` is the checked sum of every slice length.
            unsafe { HeapBuffer::from_iter(slices, text_len) }.map(Self::from_heap)
        }
    }

    #[inline]
    pub(crate) fn from_char(ch: char) -> Self {
        let mut buffer = [0; 4];
        let text = ch.encode_utf8(&mut buffer);
        // SAFETY: Every encoded `char` is at most four bytes and therefore fits inline.
        Self::from_inline(unsafe { InlineBuffer::new(text) })
    }

    #[inline]
    pub(crate) const fn from_static_str(text: &'static str) -> Result<Self, ReserveError> {
        if text.len() <= MAX_INLINE_SIZE {
            // SAFETY: The length was checked above.
            Ok(Self::from_inline(unsafe { InlineBuffer::new(text) }))
        } else {
            match StaticBuffer::new(text) {
                Ok(buffer) => Ok(Self::from_static(buffer)),
                Err(error) => Err(error),
            }
        }
    }

    #[cfg(target_pointer_width = "64")]
    #[inline]
    pub(crate) const fn len(&self) -> usize {
        let last_byte = self.last_byte();
        let inline_len = {
            let len = (last_byte as usize).wrapping_sub(LastByte::MASK_1100_0000 as usize);
            if MAX_INLINE_SIZE < len {
                MAX_INLINE_SIZE
            } else {
                len
            }
        };

        // SAFETY: `Repr` has the same size and alignment as `[usize; 2]`.
        let tail = unsafe {
            let ptr = ptr::from_ref(self).cast::<usize>().add(1);
            usize::from_le(*ptr)
        };
        let tagged_len = tail & (usize::MAX >> 8);

        if last_byte < LastByte::ExactHeapMarker as u8 {
            inline_len
        } else {
            tagged_len
        }
    }

    #[cfg(target_pointer_width = "32")]
    #[inline]
    pub(crate) const fn len(&self) -> usize {
        if self.is_heap_buffer() {
            // SAFETY: The representation tag identifies a heap buffer.
            unsafe { self.as_heap_buffer() }.len()
        } else if self.is_static_buffer() {
            // SAFETY: The representation tag identifies a static buffer.
            unsafe { self.as_static_buffer() }.len()
        } else {
            let len = (self.last_byte() as usize).wrapping_sub(LastByte::MASK_1100_0000 as usize);
            if MAX_INLINE_SIZE < len {
                MAX_INLINE_SIZE
            } else {
                len
            }
        }
    }

    #[inline]
    pub(crate) const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub(crate) const fn as_str(&self) -> &str {
        // SAFETY: Every constructor requires or produces valid UTF-8.
        unsafe { str::from_utf8_unchecked(self.as_bytes()) }
    }

    #[inline]
    pub(crate) const fn as_bytes(&self) -> &[u8] {
        let ptr = if self.is_heap_buffer() || self.is_static_buffer() {
            self.0.cast::<u8>()
        } else {
            ptr::from_ref(self).cast::<u8>()
        };

        // SAFETY: `ptr` points to the representation's initialized string bytes, which remain live
        // for at least as long as `self`.
        unsafe { slice::from_raw_parts(ptr, self.len()) }
    }

    #[inline]
    pub(crate) const fn is_heap_buffer(&self) -> bool {
        self.last_byte() == LastByte::ExactHeapMarker as u8
    }

    #[inline]
    pub(crate) const fn heap_allocation_size(&self) -> usize {
        if self.is_heap_buffer() {
            // SAFETY: The representation tag identifies a heap buffer.
            unsafe { self.as_heap_buffer() }.allocation_size()
        } else {
            0
        }
    }

    #[inline]
    const fn is_static_buffer(&self) -> bool {
        self.last_byte() == LastByte::StaticMarker as u8
    }

    #[inline]
    const fn from_inline(buffer: InlineBuffer) -> Self {
        // SAFETY: All representation variants have identical size and alignment.
        unsafe { mem::transmute(buffer) }
    }

    #[inline]
    const fn from_heap(buffer: HeapBuffer) -> Self {
        // SAFETY: All representation variants have identical size and alignment.
        unsafe { mem::transmute(buffer) }
    }

    #[inline]
    const fn from_static(buffer: StaticBuffer) -> Self {
        // SAFETY: All representation variants have identical size and alignment.
        unsafe { mem::transmute(buffer) }
    }

    #[inline]
    const fn last_byte(&self) -> u8 {
        self.2 as u8
    }

    #[inline]
    const unsafe fn as_heap_buffer(&self) -> &HeapBuffer {
        // SAFETY: The caller guarantees that the active representation is a heap buffer.
        unsafe { &*ptr::from_ref(self).cast::<HeapBuffer>() }
    }

    #[inline]
    #[cfg(target_pointer_width = "32")]
    const unsafe fn as_static_buffer(&self) -> &StaticBuffer {
        // SAFETY: The caller guarantees that the active representation is a static buffer.
        unsafe { &*ptr::from_ref(self).cast::<StaticBuffer>() }
    }

    #[inline]
    pub(crate) fn release(&self) {
        if self.is_heap_buffer() {
            // SAFETY: This `Repr` owns one live counted reference and will not be accessed after
            // its containing `CharStr` is dropped or overwritten.
            unsafe { self.as_heap_buffer().release() };
        }
    }
}

impl Clone for Repr {
    #[inline]
    fn clone(&self) -> Self {
        if self.is_heap_buffer() {
            // SAFETY: The representation tag identifies a live heap buffer.
            let heap = unsafe { self.as_heap_buffer() };
            let previous = heap.reference_count().fetch_add(1, Relaxed);

            if previous > isize::MAX as usize {
                // Balance the increment before aborting the clone. The raw copy is valid because
                // the increment above created its counted reference.
                unsafe { ptr::read(self) }.release();
                panic!("reference count overflow");
            }
        }

        // SAFETY: Heap storage gained a counted reference above. Inline and static storage can be
        // copied directly.
        unsafe { ptr::read(self) }
    }

    #[inline]
    fn clone_from(&mut self, source: &Self) {
        let replacement = source.clone();
        self.release();
        *self = replacement;
    }
}
