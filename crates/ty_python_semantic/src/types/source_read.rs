//! Continuation checks before interpreting the result of a synchronous source query.

use std::convert::Infallible;

#[cfg(test)]
mod tests;

pub(in crate::types) trait SourceReadControl {
    type Error;

    fn check(&self) -> Result<(), Self::Error>;
}

/// A stopped query can return internal recovery. Check continuation before exposing that value
/// to its caller; ordinary absence and semantic errors are only meaningful in a continuing attempt.
/// The query and its generated dependencies are responsible for their own work admission.
#[inline]
pub(in crate::types) fn read_source<C: SourceReadControl, T>(
    control: &C,
    read: impl FnOnce() -> T,
) -> Result<T, C::Error> {
    control.check()?;
    let value = read();
    control.check()?;
    Ok(value)
}

pub(in crate::types) struct UnrestrictedSourceRead;

impl SourceReadControl for UnrestrictedSourceRead {
    type Error = Infallible;

    #[inline]
    fn check(&self) -> Result<(), Infallible> {
        Ok(())
    }
}
