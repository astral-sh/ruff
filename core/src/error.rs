use std::fmt::Display;

use crate::Location;

#[derive(Debug, PartialEq, Eq)]
pub struct Error<T> {
    pub error: T,
    pub location: Location,
    pub source_path: String,
}

impl<T> std::ops::Deref for Error<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.error
    }
}

impl<T> std::error::Error for Error<T>
where
    T: std::fmt::Display + std::fmt::Debug,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl<T> Display for Error<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.location.fmt_with(f, &self.error)
    }
}

impl<T> Error<T> {
    pub fn error(self) -> T {
        self.error
    }
}
