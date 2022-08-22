use crate::Location;
use std::fmt::Display;

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

#[derive(Debug, thiserror::Error)]
pub struct CompileError<T> {
    pub body: BaseError<T>,
    pub statement: Option<String>,
}

impl<T> std::ops::Deref for CompileError<T> {
    type Target = BaseError<T>;
    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

impl<T> std::fmt::Display for CompileError<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let loc = self.location;
        if let Some(ref stmt) = self.statement {
            // visualize the error when location and statement are provided
            loc.fmt_with(f, &self.error)?;
            write!(f, "\n{stmt}{arrow:>pad$}", pad = loc.column(), arrow = "^")
        } else {
            loc.fmt_with(f, &self.error)
        }
    }
}

impl<T> CompileError<T> {
    pub fn from<U>(error: BaseError<U>, source: &str) -> Self
    where
        T: From<U>,
    {
        let statement = get_statement(source, error.location);
        CompileError {
            body: error.into(),
            statement,
        }
    }
}

fn get_statement(source: &str, loc: Location) -> Option<String> {
    if loc.column() == 0 || loc.row() == 0 {
        return None;
    }
    let line = source.split('\n').nth(loc.row() - 1)?.to_owned();
    Some(line + "\n")
}
