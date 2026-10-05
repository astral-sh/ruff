use crate::sync::atomic::{AtomicU32, Ordering};
use std::marker::PhantomData;
use std::num::NonZeroU32;

/// A type to generate nonces. Store it in a static and each nonce it produces will be unique from other nonces.
/// The type parameter `T` just serves to distinguish different kinds of nonces.
pub(crate) struct NonceGenerator<T> {
    value: AtomicU32,
    phantom: PhantomData<T>,
}

/// A "nonce" is a value that gets created exactly once.
/// We use it to mark the database storage so we can be sure we're seeing the same database.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Nonce<T>(NonZeroU32, PhantomData<T>);

impl<T> NonceGenerator<T> {
    pub(crate) const fn new() -> Self {
        Self {
            // start at 1 so we can detect rollover more easily
            value: AtomicU32::new(1),
            phantom: PhantomData,
        }
    }

    pub(crate) fn nonce(&self) -> Nonce<T> {
        let value = self.value.fetch_add(1, Ordering::Relaxed);

        assert!(value != 0, "nonce rolled over");

        Nonce(NonZeroU32::new(value).unwrap(), self.phantom)
    }
}

impl<T> Nonce<T> {
    pub(crate) fn into_u32(self) -> NonZeroU32 {
        self.0
    }

    pub(crate) fn from_u32(u32: NonZeroU32) -> Self {
        Self(u32, PhantomData)
    }
}
