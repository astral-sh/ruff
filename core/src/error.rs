use std::fmt::Display;

use crate::Location;

#[derive(Debug, PartialEq, Eq)]
pub struct BaseError<T> {
    pub error: T,
    pub location: Location,
    pub source_path: String,
}

impl<T> std::ops::Deref for BaseError<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.error
    }
}

impl<T> std::error::Error for BaseError<T>
where
    T: std::fmt::Display + std::fmt::Debug,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl<T> Display for BaseError<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.location.fmt_with(f, &self.error)
    }
}

impl<T> BaseError<T> {
    pub fn error(self) -> T {
        self.error
    }

    pub fn from<U>(obj: BaseError<U>) -> Self
    where
        U: Into<T>,
    {
        Self {
            error: obj.error.into(),
            location: obj.location,
            source_path: obj.source_path,
        }
    }

    pub fn into<U>(self) -> BaseError<U>
    where
        T: Into<U>,
    {
        BaseError::from(self)
    }
}
