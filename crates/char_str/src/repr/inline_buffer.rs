use core::ptr;

use super::{LastByte, MAX_INLINE_SIZE};

#[cfg(target_pointer_width = "64")]
#[repr(C, align(8))]
pub(super) struct InlineBuffer([u8; MAX_INLINE_SIZE]);

#[cfg(target_pointer_width = "32")]
#[repr(C, align(4))]
pub(super) struct InlineBuffer([u8; MAX_INLINE_SIZE]);

const _: () = {
    assert!(size_of::<InlineBuffer>() == MAX_INLINE_SIZE);
    assert!(align_of::<InlineBuffer>() == align_of::<usize>());
};

impl InlineBuffer {
    /// # Safety
    ///
    /// `text` must be no longer than [`MAX_INLINE_SIZE`].
    pub(super) const unsafe fn new(text: &str) -> Self {
        debug_assert!(text.len() <= MAX_INLINE_SIZE);

        let mut buffer = [0; MAX_INLINE_SIZE];
        buffer[MAX_INLINE_SIZE - 1] = text.len().to_le_bytes()[0] | LastByte::MASK_1100_0000;

        // SAFETY: The caller guarantees that `text` fits in `buffer`, and the two do not overlap.
        unsafe { ptr::copy_nonoverlapping(text.as_ptr(), buffer.as_mut_ptr(), text.len()) };

        Self(buffer)
    }

    /// # Safety
    ///
    /// `text_len` must be the sum of all slice lengths and no greater than [`MAX_INLINE_SIZE`].
    pub(super) unsafe fn from_slices(slices: &[&str], text_len: usize) -> Self {
        debug_assert!(text_len <= MAX_INLINE_SIZE);

        let mut buffer = [0; MAX_INLINE_SIZE];
        buffer[MAX_INLINE_SIZE - 1] = text_len.to_le_bytes()[0] | LastByte::MASK_1100_0000;

        let mut offset = 0;
        for text in slices {
            // SAFETY: The caller guarantees that all slices together fit in `buffer`. They cannot
            // overlap the new stack buffer.
            unsafe {
                ptr::copy_nonoverlapping(
                    text.as_ptr(),
                    buffer.as_mut_ptr().add(offset),
                    text.len(),
                );
            }
            offset += text.len();
        }
        debug_assert_eq!(offset, text_len);

        Self(buffer)
    }

    pub(super) const fn empty() -> Self {
        let mut buffer = [0; MAX_INLINE_SIZE];
        buffer[MAX_INLINE_SIZE - 1] = LastByte::Length00 as u8;
        Self(buffer)
    }
}
