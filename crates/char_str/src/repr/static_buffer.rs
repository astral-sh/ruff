use core::ptr::NonNull;

use crate::ReserveError;

use super::{LastByte, MAX_INLINE_SIZE};

#[repr(C)]
pub(super) struct StaticBuffer {
    ptr: NonNull<u8>,
    len: usize,
}

const _: () = {
    assert!(size_of::<StaticBuffer>() == MAX_INLINE_SIZE);
    assert!(align_of::<StaticBuffer>() == align_of::<usize>());
};

impl StaticBuffer {
    const USIZE_SIZE: usize = size_of::<usize>();
    const MAX_LENGTH: usize = {
        let mut bytes = [u8::MAX; Self::USIZE_SIZE];
        bytes[Self::USIZE_SIZE - 1] = 0;
        usize::from_le_bytes(bytes)
    };
    const TAG: usize = {
        let mut bytes = [0; Self::USIZE_SIZE];
        bytes[Self::USIZE_SIZE - 1] = LastByte::StaticMarker as u8;
        usize::from_ne_bytes(bytes)
    };

    pub(super) const fn new(text: &'static str) -> Result<Self, ReserveError> {
        if text.len() > Self::MAX_LENGTH {
            return Err(ReserveError);
        }

        // SAFETY: A string slice always has a non-null data pointer, including when empty.
        let ptr = unsafe { NonNull::new_unchecked(text.as_ptr().cast_mut()) };
        let len = text.len().to_le() | Self::TAG;
        Ok(Self { ptr, len })
    }

    #[cfg(target_pointer_width = "32")]
    pub(super) const fn len(&self) -> usize {
        usize::from_le_bytes((self.len ^ Self::TAG).to_ne_bytes())
    }
}
