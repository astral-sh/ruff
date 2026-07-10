use core::{alloc::Layout, ptr, ptr::NonNull};
use std::alloc::{alloc, dealloc};

#[cfg(not(loom))]
use core::sync::atomic::{
    AtomicUsize,
    Ordering::{Acquire, Release},
    fence,
};
#[cfg(loom)]
use loom::sync::atomic::{
    AtomicUsize,
    Ordering::{Acquire, Release},
    fence,
};

use crate::ReserveError;

use super::{LastByte, MAX_INLINE_SIZE};

#[repr(C)]
pub(super) struct HeapBuffer {
    // On 32-bit architectures, allocations too large to encode in `len` prepend a `usize`
    // containing the full length:
    //
    // | Optional length | Header | Data (array of `u8`) |
    //                              ^ ptr
    ptr: NonNull<u8>,
    len: TextLen,
}

struct Header {
    count: AtomicUsize,
}

const _: () = {
    assert!(size_of::<HeapBuffer>() == MAX_INLINE_SIZE);
    assert!(align_of::<HeapBuffer>() == align_of::<usize>());
    assert!(align_of::<Header>() == align_of::<usize>());
};

impl HeapBuffer {
    pub(super) fn new(text: &str) -> Result<Self, ReserveError> {
        let text_len = text.len();
        let len = TextLen::new(text_len)?;
        let ptr = Self::allocate(text_len, len.is_heap())?;

        if len.is_heap() {
            // SAFETY: `allocate` reserved the leading word used for the full length.
            unsafe {
                ptr::write(
                    ptr.sub(Self::header_offset())
                        .sub(size_of::<usize>())
                        .as_ptr()
                        .cast(),
                    text_len,
                );
            }
        }

        // SAFETY: The new allocation is valid for `text_len` data bytes and cannot overlap `text`.
        unsafe { ptr::copy_nonoverlapping(text.as_ptr(), ptr.as_ptr(), text_len) };
        Ok(Self { ptr, len })
    }

    /// # Safety
    ///
    /// `text_len` must equal the sum of the lengths of all `slices`.
    pub(super) unsafe fn from_slices(
        slices: &[&str],
        text_len: usize,
    ) -> Result<Self, ReserveError> {
        let len = TextLen::new(text_len)?;
        let ptr = Self::allocate(text_len, len.is_heap())?;

        if len.is_heap() {
            // SAFETY: `allocate` reserved the leading word used for the full length.
            unsafe {
                ptr::write(
                    ptr.sub(Self::header_offset())
                        .sub(size_of::<usize>())
                        .as_ptr()
                        .cast(),
                    text_len,
                );
            }
        }

        let mut offset = 0;
        for text in slices {
            // SAFETY: The caller guarantees that the slices total `text_len`. Each slice is valid
            // for its own length and cannot overlap the new allocation.
            unsafe {
                ptr::copy_nonoverlapping(text.as_ptr(), ptr.add(offset).as_ptr(), text.len());
            }
            offset += text.len();
        }
        debug_assert_eq!(offset, text_len);

        Ok(Self { ptr, len })
    }

    pub(super) const fn len(&self) -> usize {
        if self.len.is_heap() {
            // SAFETY: Heap-stored lengths live immediately before the allocation header.
            unsafe {
                ptr::read(
                    self.ptr
                        .sub(Self::header_offset())
                        .sub(size_of::<usize>())
                        .as_ptr()
                        .cast(),
                )
            }
        } else {
            self.len.as_usize()
        }
    }

    pub(super) fn reference_count(&self) -> &AtomicUsize {
        // SAFETY: Every live heap buffer has an initialized header immediately before its data.
        let header = unsafe { self.ptr.cast::<Header>().sub(1).as_ref() };
        &header.count
    }

    /// Releases one live reference, deallocating the buffer when it was the last reference.
    ///
    /// # Safety
    ///
    /// The caller must own one counted reference and may not access it after this call.
    pub(super) unsafe fn release(&self) {
        if self.reference_count().fetch_sub(1, Release) == 1 {
            fence(Acquire);
            // SAFETY: The reference count reached zero, so no live references remain.
            unsafe { self.dealloc() };
        }
    }

    fn allocate(text_len: usize, len_on_heap: bool) -> Result<NonNull<u8>, ReserveError> {
        let layout = Self::layout(text_len, len_on_heap)?;

        // SAFETY: Long strings always produce a nonzero layout.
        let mut allocation = unsafe { alloc(layout) };
        if allocation.is_null() {
            return Err(ReserveError);
        }

        if len_on_heap {
            // SAFETY: The layout includes a leading word for the full length.
            unsafe { allocation = allocation.add(size_of::<usize>()) };
        }

        // SAFETY: The allocation includes an aligned header followed by `text_len` data bytes.
        unsafe {
            ptr::write(
                allocation.cast(),
                Header {
                    count: AtomicUsize::new(1),
                },
            );
            Ok(NonNull::new_unchecked(
                allocation.add(Self::header_offset()),
            ))
        }
    }

    fn layout(text_len: usize, len_on_heap: bool) -> Result<Layout, ReserveError> {
        let prefix = if len_on_heap { size_of::<usize>() } else { 0 };
        let allocation_size = prefix
            .checked_add(Self::header_offset())
            .and_then(|size| size.checked_add(text_len))
            .ok_or(ReserveError)?;
        Layout::from_size_align(allocation_size, align_of::<usize>()).map_err(|_| ReserveError)
    }

    /// # Safety
    ///
    /// The allocation must have been created by [`HeapBuffer::allocate`] with the same immutable
    /// length, and no references to it may remain.
    unsafe fn dealloc(&self) {
        let text_len = self.len();
        let len_on_heap = self.len.is_heap();
        let prefix = if len_on_heap { size_of::<usize>() } else { 0 };
        let allocation_size = prefix
            .wrapping_add(Self::header_offset())
            .wrapping_add(text_len);

        // SAFETY: The allocation's original checked layout had these exact immutable dimensions.
        let layout =
            unsafe { Layout::from_size_align_unchecked(allocation_size, align_of::<usize>()) };
        let allocation = unsafe {
            let header = self.ptr.as_ptr().sub(Self::header_offset());
            if len_on_heap {
                header.sub(size_of::<usize>())
            } else {
                header
            }
        };

        // SAFETY: This is the original allocation pointer and layout, and the count is zero.
        unsafe { dealloc(allocation, layout) };
    }

    const fn header_offset() -> usize {
        size_of::<Header>()
    }
}

/// A length packed into all but the last byte of a word. On 32-bit targets, the all-ones packed
/// value indicates that the full length is stored in the allocation.
#[derive(Clone, Copy)]
struct TextLen(usize);

impl TextLen {
    const USIZE_SIZE: usize = size_of::<usize>();
    const MAX_LEN: usize = {
        let mut bytes = [u8::MAX; Self::USIZE_SIZE];
        bytes[Self::USIZE_SIZE - 1] = 0;
        usize::from_le_bytes(bytes)
            - if cfg!(target_pointer_width = "32") {
                1
            } else {
                0
            }
    };
    const TAG: usize = {
        let mut bytes = [0; Self::USIZE_SIZE];
        bytes[Self::USIZE_SIZE - 1] = LastByte::ExactHeapMarker as u8;
        usize::from_ne_bytes(bytes)
    };
    #[cfg(target_pointer_width = "32")]
    const ON_HEAP: usize = {
        let mut bytes = [u8::MAX; Self::USIZE_SIZE];
        bytes[Self::USIZE_SIZE - 1] = LastByte::ExactHeapMarker as u8;
        usize::from_ne_bytes(bytes)
    };

    #[cfg_attr(
        target_pointer_width = "32",
        expect(
            clippy::unnecessary_wraps,
            reason = "keeps length construction target-independent"
        )
    )]
    const fn new(size: usize) -> Result<Self, ReserveError> {
        if size > Self::MAX_LEN {
            #[cfg(target_pointer_width = "64")]
            return Err(ReserveError);
            #[cfg(target_pointer_width = "32")]
            return Ok(Self(Self::ON_HEAP));
        }
        Ok(Self(size.to_le() | Self::TAG))
    }

    #[cfg(target_pointer_width = "64")]
    #[inline]
    #[expect(clippy::unused_self, reason = "keeps length access target-independent")]
    const fn is_heap(self) -> bool {
        false
    }

    #[cfg(target_pointer_width = "32")]
    #[inline]
    const fn is_heap(self) -> bool {
        self.0 == Self::ON_HEAP
    }

    const fn as_usize(self) -> usize {
        usize::from_le_bytes((self.0 ^ Self::TAG).to_ne_bytes())
    }
}
