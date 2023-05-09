use crate::{source_code::SourceLocation, text_size::TextSize};
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

    pub fn into_located<U>(self, locator: &mut super::SourceLocator) -> LocatedError<U>
    where
        T: Into<U>,
    {
        let location = locator.locate(self.offset);
        LocatedError {
            error: self.error.into(),
            location: Some(location),
            source_path: self.source_path,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct LocatedError<T> {
    pub error: T,
    pub location: Option<SourceLocation>,
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

    pub fn python_location(&self) -> (usize, usize) {
        if let Some(location) = self.location {
            (
                location.row.to_one_indexed(),
                location.column.to_one_indexed(),
            )
        } else {
            (0, 0)
        }
    }
}

impl<T> Display for LocatedError<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (row, column) = self.location.map_or((0, 0), |l| {
            (l.row.to_one_indexed(), l.column.to_one_indexed())
        });
        write!(f, "{} at row {} col {}", &self.error, row, column,)
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
