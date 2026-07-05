//! Immutable string storage for [`crate::name::Name`].

use arcstr::ArcStr;

/// An immutable, atomically reference-counted string.
#[derive(Clone, Default, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub(crate) struct FrozenCompactString(ArcStr);

impl FrozenCompactString {
    #[inline]
    pub(crate) fn new(text: impl AsRef<str>) -> Self {
        Self(ArcStr::from(text.as_ref()))
    }

    #[inline]
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    #[inline]
    #[cfg(any(feature = "get-size", test))]
    pub(crate) fn is_heap_allocated(&self) -> bool {
        !ArcStr::is_static(&self.0)
    }
}

impl From<String> for FrozenCompactString {
    #[inline]
    fn from(value: String) -> Self {
        Self(ArcStr::from(value))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use arcstr::ArcStr;

    use super::FrozenCompactString;

    #[test]
    fn representation_is_pointer_sized() {
        assert_eq!(size_of::<FrozenCompactString>(), size_of::<usize>());
        assert_eq!(size_of::<Option<FrozenCompactString>>(), size_of::<usize>());
    }

    #[test]
    fn strings_roundtrip() {
        let empty = FrozenCompactString::new("");
        assert_eq!(empty.as_str(), "");
        assert!(!empty.is_heap_allocated());

        for text in [
            "x",
            "a string too long for inline storage",
            "Python 🐍 and Rust 🦀",
        ] {
            let frozen = FrozenCompactString::new(text);
            assert_eq!(frozen.as_str(), text);
            assert!(frozen.is_heap_allocated());
        }
    }

    #[test]
    fn clones_share_storage() {
        let frozen = FrozenCompactString::new("shared string");
        assert_eq!(ArcStr::strong_count(&frozen.0), Some(1));

        let clone = frozen.clone();
        assert_eq!(frozen.as_str().as_ptr(), clone.as_str().as_ptr());
        assert_eq!(ArcStr::strong_count(&frozen.0), Some(2));

        drop(frozen);
        assert_eq!(clone.as_str(), "shared string");
        assert_eq!(ArcStr::strong_count(&clone.0), Some(1));
    }

    #[test]
    fn equality_hash_and_order_match_str() {
        let alpha = FrozenCompactString::new("alpha");
        let beta = FrozenCompactString::new("beta value");
        assert!(alpha < beta);
        assert!(alpha == FrozenCompactString::new("alpha"));

        let mut expected = DefaultHasher::new();
        "alpha".hash(&mut expected);
        let mut actual = DefaultHasher::new();
        alpha.hash(&mut actual);
        assert_eq!(actual.finish(), expected.finish());
    }
}
