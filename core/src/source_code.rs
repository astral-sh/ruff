// re-export our public interface
pub use ruff_source_location::*;

pub type LineNumber = OneIndexed;

#[derive(Debug)]
pub struct SourceRange {
    pub start: SourceLocation,
    pub end: Option<SourceLocation>,
}

impl SourceRange {
    pub fn new(start: SourceLocation, end: SourceLocation) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }
    pub fn unwrap_end(&self) -> SourceLocation {
        self.end.unwrap()
    }
}

impl From<std::ops::Range<SourceLocation>> for SourceRange {
    fn from(value: std::ops::Range<SourceLocation>) -> Self {
        Self {
            start: value.start,
            end: Some(value.end),
        }
    }
}

/// Converts source code byte-offset to Python convention line and column numbers.
pub struct SourceLocator<'a> {
    pub source: &'a str,
    index: LineIndex,
}

impl<'a> SourceLocator<'a> {
    #[inline]
    pub fn new(source: &'a str) -> Self {
        let index = LineIndex::from_source_text(source);
        Self { source, index }
    }

    pub fn to_source_code(&self) -> SourceCode {
        SourceCode::new(self.source, &self.index)
    }

    pub fn locate(&mut self, offset: crate::text_size::TextSize) -> SourceLocation {
        let offset = offset.to_u32().into();
        self.to_source_code().source_location(offset)
    }

    pub fn locate_error<T, U>(&mut self, base: crate::error::BaseError<T>) -> LocatedError<U>
    where
        T: Into<U>,
    {
        let location = self.locate(base.offset);
        LocatedError {
            error: base.error.into(),
            location: Some(location),
            source_path: base.source_path,
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
            (location.row.to_usize(), location.column.to_usize())
        } else {
            (0, 0)
        }
    }
}

impl<T> std::fmt::Display for LocatedError<T>
where
    T: std::fmt::Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let (row, column) = self
            .location
            .map_or((0, 0), |l| (l.row.to_usize(), l.column.to_usize()));
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
