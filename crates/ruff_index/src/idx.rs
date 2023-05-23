use static_assertions::assert_eq_size;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::num::NonZeroU32;
use std::ops::Add;

/// Represents a newtype wrapper used to index into a Vec or a slice.
pub trait Idx: Copy + PartialEq + Eq + Hash + Debug + 'static {
    fn new(value: usize) -> Self;

    fn index(self) -> usize;
}

/// New-type wrapper for an index that can represent up to `u32::MAX - 1` entries.
///
/// The `u32::MAX - 1` is an optimization so that `Option<U32Index>` has the same size as `U32Index`.
///
/// Can store at most `u32::MAX - 1` values
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct U32Index(NonZeroU32);

impl U32Index {
    const MAX: u32 = u32::MAX - 1;

    pub const fn from_usize(value: usize) -> Self {
        assert!(value <= Self::MAX as usize);

        // SAFETY:
        // * The `value < u32::MAX` guarantees that the add doesn't overflow.
        // * The `+ 1` guarantees that the index is not zero
        #[allow(unsafe_code)]
        Self(unsafe { NonZeroU32::new_unchecked((value as u32) + 1) })
    }

    pub const fn from_u32(value: u32) -> Self {
        assert!(value <= Self::MAX);

        // SAFETY:
        // * The `value < u32::MAX` guarantees that the add doesn't overflow.
        // * The `+ 1` guarantees that the index is larger than zero.
        #[allow(unsafe_code)]
        Self(unsafe { NonZeroU32::new_unchecked(value + 1) })
    }

    /// Returns the index as a `u32` value
    #[inline]
    pub const fn as_u32(self) -> u32 {
        self.0.get() - 1
    }

    /// Returns the index as a `u32` value
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.as_u32() as usize
    }

    #[inline]
    pub const fn index(self) -> usize {
        self.as_usize()
    }
}

impl Add<usize> for U32Index {
    type Output = U32Index;

    fn add(self, rhs: usize) -> Self::Output {
        U32Index::from_usize(self.index() + rhs)
    }
}

impl Add for U32Index {
    type Output = U32Index;

    fn add(self, rhs: Self) -> Self::Output {
        U32Index::from_usize(self.index() + rhs.index())
    }
}

impl Debug for U32Index {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.index())
    }
}

impl Idx for U32Index {
    #[inline(always)]
    fn new(value: usize) -> Self {
        U32Index::from_usize(value)
    }

    #[inline(always)]
    fn index(self) -> usize {
        self.index()
    }
}

impl From<usize> for U32Index {
    fn from(value: usize) -> Self {
        U32Index::from_usize(value)
    }
}

impl From<u32> for U32Index {
    fn from(value: u32) -> Self {
        U32Index::from_u32(value)
    }
}

impl From<U32Index> for usize {
    fn from(value: U32Index) -> Self {
        value.as_usize()
    }
}

impl From<U32Index> for u32 {
    fn from(value: U32Index) -> Self {
        value.as_u32()
    }
}

assert_eq_size!(U32Index, Option<U32Index>);

#[cfg(test)]
mod tests {
    use crate::U32Index;

    #[test]
    #[should_panic(expected = "assertion failed: value <= Self::MAX")]
    fn from_u32_panics_for_u32_max() {
        U32Index::from_u32(u32::MAX);
    }

    #[test]
    #[should_panic(expected = "assertion failed: value <= Self::MAX")]
    fn from_usize_panics_for_u32_max() {
        U32Index::from_usize(u32::MAX as usize);
    }

    #[test]
    fn max_value() {
        let max_value = U32Index::from_u32(u32::MAX - 1);

        assert_eq!(max_value.as_u32(), u32::MAX - 1);
    }

    #[test]
    fn max_value_usize() {
        let max_value = U32Index::from_usize((u32::MAX - 1) as usize);

        assert_eq!(max_value.as_u32(), u32::MAX - 1);
    }
}
