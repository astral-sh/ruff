use crate::{text_size::TextSize, Location};
use std::fmt::Display;

#[derive(Debug, PartialEq, Eq)]
pub struct BaseError<T> {
    pub error: T,
    pub offset: TextSize,
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
    T: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

impl<T> Display for BaseError<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} at byte offset {}",
            &self.error,
            u32::from(self.offset)
        )
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
            offset: obj.offset,
            source_path: obj.source_path,
        }
    }

    pub fn into<U>(self) -> BaseError<U>
    where
        T: Into<U>,
    {
        BaseError::from(self)
    }

    pub fn into_located<U>(self, locator: &str) -> LocatedError<U>
    where
        T: Into<U>,
    {
        todo!()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct LocatedError<T> {
    pub error: T,
    pub location: Location,
    pub source_path: String,
}

impl<T> LocatedError<T> {
    pub fn error(self) -> T {
        self.error
    }

    pub fn from<U>(obj: LocatedError<U>) -> Self
    where
        U: Into<T>,
    {
        Self {
            error: obj.error.into(),
            location: obj.location,
            source_path: obj.source_path,
        }
    }

    pub fn into<U>(self) -> LocatedError<U>
    where
        T: Into<U>,
    {
        LocatedError::from(self)
    }
}

impl<T> Display for LocatedError<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{} at row {} col {}",
            &self.error, self.location.row, self.location.column,
        )
    }
}

impl<T> std::error::Error for LocatedError<T>
where
    T: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
