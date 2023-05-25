use std::hash::Hash;

/// Represents a newtype wrapper used to index into a Vec or a slice.
///
/// You can use the [`newtype_index`](crate::newtype_index) macro to define your own index.
pub trait Idx: Copy + PartialEq + Eq + Hash + std::fmt::Debug + 'static {
    fn new(value: usize) -> Self;

    fn index(self) -> usize;
}

#[cfg(test)]
mod tests {

    use crate::newtype_index;
    use static_assertions::{assert_eq_size, assert_impl_all};

    // Allows the macro invocation below to work
    use crate as ruff_index;

    #[newtype_index]
    #[derive(PartialOrd, Ord)]
    struct MyIndex;

    assert_impl_all!(MyIndex: Ord, PartialOrd);
    assert_eq_size!(MyIndex, Option<MyIndex>);

    #[test]
    #[should_panic(expected = "assertion failed: value <= Self::MAX")]
    fn from_u32_panics_for_u32_max() {
        MyIndex::from_u32(u32::MAX);
    }

    #[test]
    #[should_panic(expected = "assertion failed: value <= Self::MAX")]
    fn from_usize_panics_for_u32_max() {
        MyIndex::from_usize(u32::MAX as usize);
    }

    #[test]
    fn max_value() {
        let max_value = MyIndex::from_u32(u32::MAX - 1);

        assert_eq!(max_value.as_u32(), u32::MAX - 1);
    }

    #[test]
    fn max_value_usize() {
        let max_value = MyIndex::from_usize((u32::MAX - 1) as usize);

        assert_eq!(max_value.as_u32(), u32::MAX - 1);
    }

    #[test]
    fn debug() {
        let output = format!("{:?}", MyIndex::from(10u32));

        assert_eq!(output, "MyIndex(10)");
    }
}
