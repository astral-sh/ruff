use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::num::{
    NonZeroI128, NonZeroI16, NonZeroI32, NonZeroI64, NonZeroI8, NonZeroU128, NonZeroU16,
    NonZeroU32, NonZeroU64, NonZeroU8,
};
use std::path::{Path, PathBuf};

use glob::Pattern;
use itertools::Itertools;
use regex::Regex;
use seahash::SeaHasher;

/// A type that be used as part of a cache key.
///
/// A cache looks up artefacts by a cache key. Many cache keys are composed of sub-keys. For example,
/// caching the lint results of a file depend at least on the file content, the user settings, and linter version.
/// Types implementing the [`CacheKey`] trait can be used as part of a cache key by which artefacts are queried.
///
/// ## Implementing `CacheKey`
///
/// You can derive [`CacheKey`] with `#[derive(CacheKey)]` if all fields implement [`CacheKey`]. The resulting
/// cache key will be the combination of the values from calling `cache_key` on each field.
///
/// ```
/// # use ruff_macros::CacheKey;
///
/// #[derive(CacheKey)]
/// struct Test {
///     name: String,
///     version: u32,
/// }
/// ```
///
/// If you need more control over computing the cache key, you can of course implement the [`CacheKey]` yourself:
///
/// ```
///  use ruff_cache::{CacheKey, CacheKeyHasher};
///
/// struct Test {
///     name: String,
///     version: u32,
///     other: String
/// }
///
/// impl CacheKey for Test {
///     fn cache_key(&self, state: &mut CacheKeyHasher) {
///         self.name.cache_key(state);
///         self.version.cache_key(state);
///     }
/// }
/// ```
///
/// ## Portability
///
/// Ideally, the cache key is portable across platforms but this is not yet a strict requirement.
///
/// ## Using [`Hash`]
///
/// You can defer to the [`Hash`] implementation for non-composite types.
/// Be aware, that the [`Hash`] implementation may not be portable.
///
/// ## Why a new trait rather than reusing [`Hash`]?
/// The main reason is that hashes and cache keys have different constraints:
///
/// * Cache keys are less performance sensitive: Hashes must be super fast to compute for performant hashed-collections. That's
///   why some standard types don't implement [`Hash`] where it would be safe to implement [`CacheKey`], e.g. `HashSet`
/// * Cache keys must be deterministic where hash keys do not have this constraint. That's why pointers don't implement [`CacheKey`] but they implement [`Hash`].
/// * Ideally, cache keys are portable
///
/// [`Hash`](Hash)
pub trait CacheKey {
    fn cache_key(&self, state: &mut CacheKeyHasher);

    fn cache_key_slice(data: &[Self], state: &mut CacheKeyHasher)
    where
        Self: Sized,
    {
        for piece in data {
            piece.cache_key(state);
        }
    }
}

impl CacheKey for bool {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u8(u8::from(*self));
    }
}

impl CacheKey for char {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u32(*self as u32);
    }
}

impl CacheKey for usize {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(*self);
    }
}

impl CacheKey for u128 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u128(*self);
    }
}

impl CacheKey for u64 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u64(*self);
    }
}

impl CacheKey for u32 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u32(*self);
    }
}

impl CacheKey for u16 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u16(*self);
    }
}

impl CacheKey for u8 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_u8(*self);
    }
}

impl CacheKey for isize {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_isize(*self);
    }
}

impl CacheKey for i128 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_i128(*self);
    }
}

impl CacheKey for i64 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_i64(*self);
    }
}

impl CacheKey for i32 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_i32(*self);
    }
}

impl CacheKey for i16 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_i16(*self);
    }
}

impl CacheKey for i8 {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_i8(*self);
    }
}
macro_rules! impl_cache_key_non_zero {
    ($name:ident) => {
        impl CacheKey for $name {
            #[inline]
            fn cache_key(&self, state: &mut CacheKeyHasher) {
                self.get().cache_key(state)
            }
        }
    };
}

impl_cache_key_non_zero!(NonZeroU8);
impl_cache_key_non_zero!(NonZeroU16);
impl_cache_key_non_zero!(NonZeroU32);
impl_cache_key_non_zero!(NonZeroU64);
impl_cache_key_non_zero!(NonZeroU128);

impl_cache_key_non_zero!(NonZeroI8);
impl_cache_key_non_zero!(NonZeroI16);
impl_cache_key_non_zero!(NonZeroI32);
impl_cache_key_non_zero!(NonZeroI64);
impl_cache_key_non_zero!(NonZeroI128);

macro_rules! impl_cache_key_tuple {
    () => (
        impl CacheKey for () {
            #[inline]
            fn cache_key(&self, _state: &mut CacheKeyHasher) {}
        }
    );

    ( $($name:ident)+) => (
        impl<$($name: CacheKey),+> CacheKey for ($($name,)+) where last_type!($($name,)+): ?Sized {
            #[allow(non_snake_case)]
            #[inline]
            fn cache_key(&self, state: &mut CacheKeyHasher) {
                let ($(ref $name,)+) = *self;
                $($name.cache_key(state);)+
            }
        }
    );
}

macro_rules! last_type {
    ($a:ident,) => { $a };
    ($a:ident, $($rest_a:ident,)+) => { last_type!($($rest_a,)+) };
}

impl_cache_key_tuple! {}
impl_cache_key_tuple! { T }
impl_cache_key_tuple! { T B }
impl_cache_key_tuple! { T B C }
impl_cache_key_tuple! { T B C D }
impl_cache_key_tuple! { T B C D E }
impl_cache_key_tuple! { T B C D E F }
impl_cache_key_tuple! { T B C D E F G }
impl_cache_key_tuple! { T B C D E F G H }
impl_cache_key_tuple! { T B C D E F G H I }
impl_cache_key_tuple! { T B C D E F G H I J }
impl_cache_key_tuple! { T B C D E F G H I J K }
impl_cache_key_tuple! { T B C D E F G H I J K L }

impl CacheKey for str {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.hash(&mut *state);
    }
}

impl CacheKey for String {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.hash(&mut *state);
    }
}

impl<T: CacheKey> CacheKey for Option<T> {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        match self {
            None => state.write_usize(0),
            Some(value) => {
                state.write_usize(1);
                value.cache_key(state);
            }
        }
    }
}

impl<T: CacheKey> CacheKey for [T] {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(self.len());
        CacheKey::cache_key_slice(self, state);
    }
}

impl<T: ?Sized + CacheKey> CacheKey for &T {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        (**self).cache_key(state);
    }
}

impl<T: ?Sized + CacheKey> CacheKey for &mut T {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        (**self).cache_key(state);
    }
}

impl<T> CacheKey for Vec<T>
where
    T: CacheKey,
{
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(self.len());
        CacheKey::cache_key_slice(self, state);
    }
}

impl<K, V, S> CacheKey for HashMap<K, V, S>
where
    K: CacheKey + Ord,
    V: CacheKey,
{
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(self.len());
        for (key, value) in self
            .iter()
            .sorted_by(|(left, _), (right, _)| left.cmp(right))
        {
            key.cache_key(state);
            value.cache_key(state);
        }
    }
}

impl<V: CacheKey + Ord, S> CacheKey for HashSet<V, S> {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(self.len());
        for value in self.iter().sorted() {
            value.cache_key(state);
        }
    }
}

impl<V: CacheKey> CacheKey for BTreeSet<V> {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(self.len());
        for item in self {
            item.cache_key(state);
        }
    }
}

impl<K: CacheKey + Ord, V: CacheKey> CacheKey for BTreeMap<K, V> {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        state.write_usize(self.len());

        for (key, value) in self {
            key.cache_key(state);
            value.cache_key(state);
        }
    }
}

impl CacheKey for Path {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        for component in self.components() {
            component.hash(&mut *state);
        }
    }
}

impl CacheKey for PathBuf {
    #[inline]
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.as_path().cache_key(state);
    }
}

impl<V: ?Sized> CacheKey for Cow<'_, V>
where
    V: CacheKey + ToOwned,
{
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        (**self).cache_key(state);
    }
}

impl CacheKey for Regex {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.as_str().cache_key(state);
    }
}

impl CacheKey for Pattern {
    fn cache_key(&self, state: &mut CacheKeyHasher) {
        self.as_str().cache_key(state);
    }
}

#[derive(Clone, Default)]
pub struct CacheKeyHasher {
    inner: SeaHasher,
}

impl CacheKeyHasher {
    pub fn new() -> Self {
        Self {
            inner: SeaHasher::new(),
        }
    }
}

impl Hasher for CacheKeyHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.inner.finish()
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        self.inner.write(bytes);
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.inner.write_u8(i);
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.inner.write_u16(i);
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.inner.write_u32(i);
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.inner.write_u64(i);
    }

    #[inline]
    fn write_u128(&mut self, i: u128) {
        self.inner.write_u128(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.inner.write_usize(i);
    }

    #[inline]
    fn write_i8(&mut self, i: i8) {
        self.inner.write_i8(i);
    }

    #[inline]
    fn write_i16(&mut self, i: i16) {
        self.inner.write_i16(i);
    }

    #[inline]
    fn write_i32(&mut self, i: i32) {
        self.inner.write_i32(i);
    }

    #[inline]
    fn write_i64(&mut self, i: i64) {
        self.inner.write_i64(i);
    }

    #[inline]
    fn write_i128(&mut self, i: i128) {
        self.inner.write_i128(i);
    }

    #[inline]
    fn write_isize(&mut self, i: isize) {
        self.inner.write_isize(i);
    }
}
