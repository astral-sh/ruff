//! User-implementable salsa traits for refining the return type via `returns(as_ref)` and `returns(as_deref)`.

use std::ops::Deref;

/// Used to determine the return type and value for tracked fields and functions annotated with `returns(as_ref)`.
pub trait SalsaAsRef {
    // The type returned by tracked fields and functions annotated with `returns(as_ref)`.
    type AsRef<'a>
    where
        Self: 'a;

    // The value returned by tracked fields and functions annotated with `returns(as_ref)`.
    fn as_ref(&self) -> Self::AsRef<'_>;
}

impl<T> SalsaAsRef for Option<T> {
    type AsRef<'a>
        = Option<&'a T>
    where
        Self: 'a;

    fn as_ref(&self) -> Self::AsRef<'_> {
        self.as_ref()
    }
}

impl<T, E> SalsaAsRef for Result<T, E> {
    type AsRef<'a>
        = Result<&'a T, &'a E>
    where
        Self: 'a;

    fn as_ref(&self) -> Self::AsRef<'_> {
        self.as_ref()
    }
}

/// Used to determine the return type and value for tracked fields and functions annotated with `returns(as_deref)`.
pub trait SalsaAsDeref {
    // The type returned by tracked fields and functions annotated with `returns(as_deref)`.
    type AsDeref<'a>
    where
        Self: 'a;

    // The value returned by tracked fields and functions annotated with `returns(as_deref)`.
    fn as_deref(&self) -> Self::AsDeref<'_>;
}

impl<T: Deref> SalsaAsDeref for Option<T> {
    type AsDeref<'a>
        = Option<&'a T::Target>
    where
        Self: 'a;

    fn as_deref(&self) -> Self::AsDeref<'_> {
        self.as_deref()
    }
}

impl<T: Deref, E> SalsaAsDeref for Result<T, E> {
    type AsDeref<'a>
        = Result<&'a T::Target, &'a E>
    where
        Self: 'a;

    fn as_deref(&self) -> Self::AsDeref<'_> {
        self.as_deref()
    }
}
