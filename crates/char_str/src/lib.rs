#![doc = include_str!("../README.md")]
#![expect(
    unsafe_code,
    reason = "the compact tagged representation requires raw allocation and pointer access"
)]

use core::{
    borrow::Borrow,
    cmp, fmt,
    hash::{Hash, Hasher},
    ops::Deref,
    str::FromStr,
};
use std::{borrow::Cow, boxed::Box, ffi::OsStr, string::String};

mod repr;
use repr::Repr;

#[cfg(feature = "serde")]
mod serde;

/// An error that occurs when allocating storage for a [`CharStr`] fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReserveError;

impl fmt::Display for ReserveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("cannot allocate memory to hold CharStr")
    }
}

impl core::error::Error for ReserveError {}

/// Compact, immutable, UTF-8 encoded owned string type.
///
/// Short strings are stored inline. Long strings use an exactly-sized, reference-counted heap
/// allocation, making clones cheap without retaining a capacity that can never be used.
///
/// # Examples
///
/// ```
/// use char_str::CharStr;
///
/// let value = CharStr::from("a string longer than the inline limit");
/// let clone = value.clone();
/// assert!(core::ptr::eq(value.as_ptr(), clone.as_ptr()));
/// ```
#[repr(transparent)]
pub struct CharStr(Repr);

const _: () = {
    assert!(size_of::<CharStr>() == size_of::<[usize; 2]>());
    assert!(size_of::<Option<CharStr>>() == size_of::<[usize; 2]>());
    assert!(align_of::<CharStr>() == align_of::<usize>());
    assert!(align_of::<Option<CharStr>>() == align_of::<usize>());
};

impl CharStr {
    /// Creates an empty `CharStr`.
    #[inline]
    pub const fn new() -> Self {
        Self(Repr::new())
    }

    /// Creates a `CharStr` backed directly by a static string when it does not fit inline.
    ///
    /// # Panics
    ///
    /// Panics if `text` is too long to represent.
    #[inline]
    pub const fn from_static_str(text: &'static str) -> Self {
        match Repr::from_static_str(text) {
            Ok(repr) => Self(repr),
            Err(_) => panic!("text is too long"),
        }
    }

    /// Creates an exactly-sized `CharStr`.
    ///
    /// # Errors
    ///
    /// Returns a [`ReserveError`] if the string is too long or its heap buffer cannot be allocated.
    #[inline]
    pub fn try_from_str(text: &str) -> Result<Self, ReserveError> {
        Repr::from_str(text).map(Self)
    }

    /// Creates an exactly-sized `CharStr` by concatenating string slices.
    ///
    /// Heap storage is allocated at most once. To handle allocation failure, use
    /// [`CharStr::try_concat`].
    ///
    /// # Panics
    ///
    /// Panics if the combined length overflows or the exact buffer cannot be allocated.
    #[inline]
    pub fn concat(slices: &[&str]) -> Self {
        Self::try_concat(slices)
            .unwrap_or_else(|_| panic!("cannot allocate memory to hold CharStr"))
    }

    /// Fallible version of [`CharStr::concat`].
    ///
    /// # Errors
    ///
    /// Returns a [`ReserveError`] if the combined length overflows, is too long to represent, or
    /// its heap buffer cannot be allocated.
    #[inline]
    pub fn try_concat(slices: &[&str]) -> Result<Self, ReserveError> {
        Repr::from_slices(slices).map(Self)
    }

    /// Creates an exactly-sized `CharStr` by joining string slices with a separator.
    ///
    /// Heap storage is allocated at most once. To handle allocation failure, use
    /// [`CharStr::try_join`].
    ///
    /// # Panics
    ///
    /// Panics if the joined length overflows or the exact buffer cannot be allocated.
    #[inline]
    pub fn join<T: AsRef<str>>(slices: &[T], separator: &str) -> Self {
        Self::try_join(slices, separator)
            .unwrap_or_else(|_| panic!("cannot allocate memory to hold CharStr"))
    }

    /// Fallible version of [`CharStr::join`].
    ///
    /// # Errors
    ///
    /// Returns a [`ReserveError`] if the joined length overflows, is too long to represent, or
    /// its heap buffer cannot be allocated.
    #[inline]
    pub fn try_join<T: AsRef<str>>(slices: &[T], separator: &str) -> Result<Self, ReserveError> {
        Repr::from_joined_slices(slices, separator).map(Self)
    }

    /// Returns the string as a string slice.
    #[inline]
    pub const fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the string as a byte slice.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Returns the length in bytes.
    #[inline]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` if the string has a length of zero.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns whether the string uses a reference-counted heap allocation.
    #[inline]
    pub const fn is_heap_allocated(&self) -> bool {
        self.0.is_heap_buffer()
    }
}

impl Clone for CharStr {
    #[inline]
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }

    #[inline]
    fn clone_from(&mut self, source: &Self) {
        self.0.clone_from(&source.0);
    }
}

impl Drop for CharStr {
    #[inline]
    fn drop(&mut self) {
        self.0.release();
    }
}

// SAFETY: Heap storage is immutable and uses the same atomic reference-counting protocol as
// `Arc`; inline and static storage are immutable values.
unsafe impl Send for CharStr {}
// SAFETY: See the `Send` implementation above.
unsafe impl Sync for CharStr {}

impl Default for CharStr {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Deref for CharStr {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Debug for CharStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for CharStr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.as_str(), f)
    }
}

impl AsRef<str> for CharStr {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<OsStr> for CharStr {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        OsStr::new(self.as_str())
    }
}

impl AsRef<[u8]> for CharStr {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Borrow<str> for CharStr {
    #[inline]
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl Eq for CharStr {}

impl PartialEq for CharStr {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<str> for CharStr {
    #[inline]
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<CharStr> for str {
    #[inline]
    fn eq(&self, other: &CharStr) -> bool {
        self == other.as_str()
    }
}

impl PartialEq<&str> for CharStr {
    #[inline]
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<CharStr> for &str {
    #[inline]
    fn eq(&self, other: &CharStr) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<String> for CharStr {
    #[inline]
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<CharStr> for String {
    #[inline]
    fn eq(&self, other: &CharStr) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Ord for CharStr {
    #[inline]
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl PartialOrd for CharStr {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for CharStr {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl From<char> for CharStr {
    #[inline]
    fn from(value: char) -> Self {
        Self(Repr::from_char(value))
    }
}

impl From<&str> for CharStr {
    #[inline]
    #[track_caller]
    fn from(value: &str) -> Self {
        Self::try_from_str(value)
            .unwrap_or_else(|_| panic!("cannot allocate memory to hold CharStr"))
    }
}

impl From<String> for CharStr {
    #[inline]
    #[track_caller]
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&String> for CharStr {
    #[inline]
    #[track_caller]
    fn from(value: &String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<Cow<'_, str>> for CharStr {
    fn from(value: Cow<'_, str>) -> Self {
        Self::from(value.as_ref())
    }
}

impl From<Box<str>> for CharStr {
    #[inline]
    #[track_caller]
    fn from(value: Box<str>) -> Self {
        Self::from(value.as_ref())
    }
}

impl From<&CharStr> for CharStr {
    #[inline]
    fn from(value: &CharStr) -> Self {
        value.clone()
    }
}

impl From<CharStr> for String {
    #[inline]
    fn from(value: CharStr) -> Self {
        value.as_str().into()
    }
}

impl From<&CharStr> for String {
    #[inline]
    fn from(value: &CharStr) -> Self {
        value.as_str().into()
    }
}

impl FromStr for CharStr {
    type Err = ReserveError;

    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from_str(s)
    }
}

impl FromIterator<char> for CharStr {
    fn from_iter<T: IntoIterator<Item = char>>(iter: T) -> Self {
        Self::from(String::from_iter(iter))
    }
}

impl<'a> FromIterator<&'a char> for CharStr {
    fn from_iter<T: IntoIterator<Item = &'a char>>(iter: T) -> Self {
        iter.into_iter().copied().collect()
    }
}
