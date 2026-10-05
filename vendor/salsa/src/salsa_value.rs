#![allow(clippy::undocumented_unsafe_blocks)] // Implementations are structural; see trait safety docs.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[cfg(feature = "rayon")]
use rayon::iter::Either;

use crate::sync::Arc;

/// A value Salsa can safely retain across revisions.
///
/// Salsa values can be stored in interned and tracked structs or used as
/// tracked query results. Ordinary `'static` values are accepted directly at
/// those storage boundaries; types carrying the database lifetime must
/// implement this trait.
///
/// # Safety
///
/// Implementing this trait asserts that the type is effectively `'static`
/// from Salsa's perspective: an instance produced in an older revision must
/// remain safe to retain and use in a newer revision. This includes calling
/// safe trait methods such as [`PartialEq::eq`] on the old value. In particular,
/// `SalsaValue` must not be implemented for a database-lifetime reference such
/// as `&'db T`, which may point to storage changed or freed in a newer revision.
///
/// `#[derive(SalsaValue)]` checks this requirement structurally. It cannot
/// account for additional invariants maintained by unsafe code in the type's
/// methods.
#[diagnostic::on_unimplemented(
    message = "`{Self}` doesn't implement `SalsaValue`",
    note = "add `#[derive(salsa::SalsaValue)]` to `{Self}`"
)]
pub unsafe trait SalsaValue {}

/// Selects an explicit [`SalsaValue`] implementation or an unchanged `'static` value.
#[doc(hidden)]
pub mod helper {
    use std::marker::PhantomData;

    use super::SalsaValue;

    pub struct Dispatch<T>(PhantomData<T>);

    impl<T: SalsaValue> Dispatch<T> {
        pub fn assert_salsa_value() {}
    }

    pub trait Fallback<T> {
        fn assert_salsa_value();
    }

    impl<T: 'static> Fallback<T> for Dispatch<T> {
        fn assert_salsa_value() {}
    }

    pub const fn assert_salsa_value<T: SalsaValue>() {}
}

macro_rules! salsa_values {
    ($($ty:ty),* $(,)?) => {
        $(
            // SAFETY: Values of this type contain no database-lifetime references.
            unsafe impl SalsaValue for $ty {}
        )*
    };
}

salsa_values! {
    (),
    bool,
    char,
    f32,
    f64,
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    String,
    PathBuf,
    std::collections::hash_map::RandomState,
    std::convert::Infallible,
    std::num::NonZeroI8,
    std::num::NonZeroI16,
    std::num::NonZeroI32,
    std::num::NonZeroI64,
    std::num::NonZeroI128,
    std::num::NonZeroIsize,
    std::num::NonZeroU8,
    std::num::NonZeroU16,
    std::num::NonZeroU32,
    std::num::NonZeroU64,
    std::num::NonZeroU128,
    std::num::NonZeroUsize,
    rustc_hash::FxBuildHasher,
    crate::Id,
}

#[cfg(feature = "compact_str")]
salsa_values!(compact_str::CompactString);

// SAFETY: Values of these types contain no database-lifetime references.
unsafe impl SalsaValue for str {}
unsafe impl SalsaValue for Path {}
unsafe impl<T: SalsaValue> SalsaValue for [T] {}

// SAFETY: A genuinely `'static` reference remains valid across revisions.
unsafe impl<T: ?Sized> SalsaValue for &'static T {}

// SAFETY: `PhantomData` contains no data.
unsafe impl<T: ?Sized> SalsaValue for PhantomData<T> {}

// SAFETY: `BuildHasherDefault` contains no value of type `T`.
unsafe impl<T> SalsaValue for std::hash::BuildHasherDefault<T> {}

macro_rules! default_salsa_value {
    ($($container:ident)::+ < $($parameter:ident),+ >) => {
        // SAFETY: Every retained parameter is a `SalsaValue`.
        unsafe impl<$($parameter: SalsaValue),+> SalsaValue
            for $($container)::+<$($parameter),+>
        {}
    };
}

default_salsa_value!(Vec<T>);
default_salsa_value!(Option<T>);
default_salsa_value!(Result<T, E>);
default_salsa_value!(thin_vec::ThinVec<T>);
default_salsa_value!(std::ops::Range<T>);
default_salsa_value!(std::ops::RangeInclusive<T>);

// SAFETY: The owned pointee is a `SalsaValue`.
unsafe impl<T: ?Sized + SalsaValue> SalsaValue for Box<T> {}
unsafe impl<T: ?Sized + SalsaValue> SalsaValue for Arc<T> {}
unsafe impl<T: ?Sized + SalsaValue> SalsaValue for Rc<T> {}

#[cfg(feature = "triomphe")]
// SAFETY: The owned pointee is a `SalsaValue`.
unsafe impl<T: ?Sized + SalsaValue> SalsaValue for triomphe::Arc<T> {}

// SAFETY: Every retained element is a `SalsaValue`.
unsafe impl<T: SalsaValue, const N: usize> SalsaValue for [T; N] {}
unsafe impl<A> SalsaValue for smallvec::SmallVec<A>
where
    A: smallvec::Array,
    A::Item: SalsaValue,
{
}

default_salsa_value!(HashMap<K, V, S>);
default_salsa_value!(HashSet<K, S>);
default_salsa_value!(BTreeMap<K, V>);
default_salsa_value!(BTreeSet<K>);
default_salsa_value!(hashbrown::HashMap<K, V, S>);
default_salsa_value!(hashbrown::HashSet<K, S>);
default_salsa_value!(indexmap::IndexMap<K, V, S>);
default_salsa_value!(indexmap::IndexSet<K, S>);

#[cfg(feature = "ordermap")]
default_salsa_value!(ordermap::OrderMap<K, V, S>);

#[cfg(feature = "ordermap")]
default_salsa_value!(ordermap::OrderSet<K, S>);

#[cfg(feature = "rayon")]
default_salsa_value!(Either<L, R>);

macro_rules! tuple_salsa_value {
    ($($t:ident),+) => {
        // SAFETY: Every retained tuple element is a `SalsaValue`.
        unsafe impl<$($t: SalsaValue),+> SalsaValue for ($($t,)*) {}
    };
}

tuple_salsa_value!(A);
tuple_salsa_value!(A, B);
tuple_salsa_value!(A, B, C);
tuple_salsa_value!(A, B, C, D);
tuple_salsa_value!(A, B, C, D, E);
tuple_salsa_value!(A, B, C, D, E, F);
tuple_salsa_value!(A, B, C, D, E, F, G);
tuple_salsa_value!(A, B, C, D, E, F, G, H);
tuple_salsa_value!(A, B, C, D, E, F, G, H, I);
tuple_salsa_value!(A, B, C, D, E, F, G, H, I, J);
tuple_salsa_value!(A, B, C, D, E, F, G, H, I, J, K);
tuple_salsa_value!(A, B, C, D, E, F, G, H, I, J, K, L);
