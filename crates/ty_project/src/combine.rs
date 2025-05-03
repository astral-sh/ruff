use std::{collections::HashMap, hash::BuildHasher};

use ruff_db::system::SystemPathBuf;
use ruff_python_ast::PythonVersion;
use ty_python_semantic::{PythonPath, PythonPlatform};

/// Combine two values, preferring the values in `self`.
///
/// The logic should follow that of Cargo's `config.toml`:
///
/// > If a key is specified in multiple config files, the values will get merged together.
/// > Numbers, strings, and booleans will use the value in the deeper config directory taking
/// > precedence over ancestor directories, where the home directory is the lowest priority.
/// > Arrays will be joined together with higher precedence items being placed later in the
/// > merged array.
///
/// ## uv Compatibility
///
/// The merging behavior differs from uv in that values with higher precedence in arrays
/// are placed later in the merged array. This is because we want to support overriding
/// earlier values and values from other configurations, including unsetting them.
/// For example: patterns coming last in file inclusion and exclusion patterns
/// allow overriding earlier patterns, matching the `gitignore` behavior.
/// Generally speaking, it feels more intuitive if later values override earlier values
/// than the other way around: `ty --exclude png --exclude "!important.png"`.
///
/// The main downside of this approach is that the ordering can be surprising in cases
/// where the option has a "first match" semantic and not a "last match" wins.
/// One such example is `extra-paths` where the semantics is given by Python:
/// the module on the first matching search path wins.
///
/// ```toml
/// [environment]
/// extra-paths = ["b", "c"]
/// ```
///
/// ```bash
/// ty --extra-paths a
/// ```
///
/// That's why a user might expect that this configuration results in `["a", "b", "c"]`,
/// because the CLI has higher precedence. However, the current implementation results in a
/// resolved extra search path of `["b", "c", "a"]`, which means `a` will be tried last.
///
/// There's an argument here that the user should be able to specify the order of the paths,
/// because only then is the user in full control of where to insert the path when specifying `extra-paths`
/// in multiple sources.
///
/// ## Macro
/// You can automatically derive `Combine` for structs with named fields by using `derive(ruff_macros::Combine)`.
pub trait Combine {
    #[must_use]
    fn combine(mut self, other: Self) -> Self
    where
        Self: Sized,
    {
        self.combine_with(other);
        self
    }

    fn combine_with(&mut self, other: Self);
}

impl<T> Combine for Option<T>
where
    T: Combine,
{
    fn combine(self, other: Self) -> Self
    where
        Self: Sized,
    {
        match (self, other) {
            (Some(a), Some(b)) => Some(a.combine(b)),
            (None, Some(b)) => Some(b),
            (a, _) => a,
        }
    }

    fn combine_with(&mut self, other: Self) {
        match (self, other) {
            (Some(a), Some(b)) => {
                a.combine_with(b);
            }
            (a @ None, Some(b)) => {
                *a = Some(b);
            }
            _ => {}
        }
    }
}

impl<T> Combine for Vec<T> {
    fn combine_with(&mut self, mut other: Self) {
        // `self` takes precedence over `other` but values with higher precedence must be placed after.
        // Swap the vectors so that `other` is the one that gets extended, so that the values of `self` come after.
        std::mem::swap(self, &mut other);
        self.extend(other);
    }
}

impl<K, V, S> Combine for HashMap<K, V, S>
where
    K: Eq + std::hash::Hash,
    S: BuildHasher,
{
    fn combine_with(&mut self, mut other: Self) {
        // `self` takes precedence over `other` but `extend` overrides existing values.
        // Swap the hash maps so that `self` is the one that gets extended.
        std::mem::swap(self, &mut other);
        self.extend(other);
    }
}

/// Implements [`Combine`] for a value that always returns `self` when combined with another value.
macro_rules! impl_noop_combine {
    ($name:ident) => {
        impl Combine for $name {
            #[inline(always)]
            fn combine_with(&mut self, _other: Self) {}

            #[inline(always)]
            fn combine(self, _other: Self) -> Self {
                self
            }
        }
    };
}

impl_noop_combine!(SystemPathBuf);
impl_noop_combine!(PythonPlatform);
impl_noop_combine!(PythonPath);
impl_noop_combine!(PythonVersion);

// std types
impl_noop_combine!(bool);
impl_noop_combine!(usize);
impl_noop_combine!(u8);
impl_noop_combine!(u16);
impl_noop_combine!(u32);
impl_noop_combine!(u64);
impl_noop_combine!(u128);
impl_noop_combine!(isize);
impl_noop_combine!(i8);
impl_noop_combine!(i16);
impl_noop_combine!(i32);
impl_noop_combine!(i64);
impl_noop_combine!(i128);
impl_noop_combine!(String);

#[cfg(test)]
mod tests {
    use crate::combine::Combine;
    use std::collections::HashMap;

    #[test]
    fn combine_option() {
        assert_eq!(Some(1).combine(Some(2)), Some(1));
        assert_eq!(None.combine(Some(2)), Some(2));
        assert_eq!(Some(1).combine(None), Some(1));
    }

    #[test]
    fn combine_vec() {
        assert_eq!(None.combine(Some(vec![1, 2, 3])), Some(vec![1, 2, 3]));
        assert_eq!(Some(vec![1, 2, 3]).combine(None), Some(vec![1, 2, 3]));
        assert_eq!(
            Some(vec![1, 2, 3]).combine(Some(vec![4, 5, 6])),
            Some(vec![4, 5, 6, 1, 2, 3])
        );
    }

    #[test]
    fn combine_map() {
        let a: HashMap<u32, _> = HashMap::from_iter([(1, "a"), (2, "a"), (3, "a")]);
        let b: HashMap<u32, _> = HashMap::from_iter([(0, "b"), (2, "b"), (5, "b")]);

        assert_eq!(None.combine(Some(b.clone())), Some(b.clone()));
        assert_eq!(Some(a.clone()).combine(None), Some(a.clone()));
        assert_eq!(
            Some(a).combine(Some(b)),
            Some(HashMap::from_iter([
                (0, "b"),
                // The value from `a` takes precedence
                (1, "a"),
                (2, "a"),
                (3, "a"),
                (5, "b")
            ]))
        );
    }
}
