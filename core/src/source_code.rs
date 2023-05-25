// re-export our public interface
use crate::text_size::{TextLen, TextSize};
use memchr::memrchr2;

pub use ruff_source_location::{
    newlines::{find_newline, UniversalNewlineIterator},
    LineIndex, OneIndexed, SourceCode, SourceLocation,
};
pub type LineNumber = OneIndexed;

#[derive(Debug, Copy, Clone, Default)]
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
pub struct RandomLocator<'a> {
    pub source: &'a str,
    index: LineIndex,
}

impl<'a> RandomLocator<'a> {
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

/// Converts source code byte-offset to Python convention line and column numbers.
pub struct LinearLocator<'a> {
    pub source: &'a str,
    state: LinearLocatorState,
    #[cfg(debug_assertions)]
    index: LineIndex,
}

struct LinearLocatorState {
    line_start: TextSize,
    line_end: Option<TextSize>,
    line_number: OneIndexed,
    cursor: TextSize,
    is_ascii: bool,
}

impl LinearLocatorState {
    fn init(source: &str) -> Self {
        let mut line_start = TextSize::default();
        if source.starts_with('\u{feff}') {
            line_start += '\u{feff}'.text_len();
        }
        let (line_end, is_ascii) = if let Some((position, line_ending)) = find_newline(source) {
            let is_ascii = source[..position].is_ascii();
            (
                Some(TextSize::new(position as u32 + line_ending.len() as u32)),
                is_ascii,
            )
        } else {
            (None, source.is_ascii())
        };
        let line_number = OneIndexed::MIN;
        Self {
            line_start,
            line_end,
            line_number,
            cursor: line_start,
            is_ascii,
        }
    }

    fn new_line_start(&self, next_offset: TextSize) -> Option<TextSize> {
        if let Some(new_line_start) = self.line_end {
            if new_line_start <= next_offset {
                return Some(new_line_start);
            }
        }
        None
    }
}

impl<'a> LinearLocator<'a> {
    // nl = newline

    #[inline]
    pub fn new(source: &'a str) -> Self {
        let state = LinearLocatorState::init(source);
        Self {
            source,
            state,
            #[cfg(debug_assertions)]
            index: LineIndex::from_source_text(source),
        }
    }

    pub fn locate(&mut self, offset: crate::text_size::TextSize) -> SourceLocation {
        debug_assert!(
            self.state.cursor <= offset,
            "{:?} -> {:?} {}",
            self.state.cursor,
            offset,
            &self.source[offset.to_usize()..self.state.cursor.to_usize()]
        );
        let (column, new_state) = self.locate_inner(offset);
        if let Some(state) = new_state {
            self.state = state;
        } else {
            self.state.cursor = offset;
        }
        SourceLocation {
            row: self.state.line_number,
            column,
        }
    }

    pub fn locate_only(&mut self, offset: crate::text_size::TextSize) -> SourceLocation {
        let (column, new_state) = self.locate_inner(offset);
        let state = new_state.as_ref().unwrap_or(&self.state);
        SourceLocation {
            row: state.line_number,
            column,
        }
    }

    fn locate_inner(
        &mut self,
        offset: crate::text_size::TextSize,
    ) -> (OneIndexed, Option<LinearLocatorState>) {
        let (column, new_state) = if let Some(new_line_start) = self.state.new_line_start(offset) {
            // not fit in current line
            let focused = &self.source[new_line_start.to_usize()..offset.to_usize()];
            let (lines, line_start, column) =
                if let Some(last_newline) = memrchr2(b'\r', b'\n', focused.as_bytes()) {
                    let last_newline = new_line_start.to_usize() + last_newline;
                    let lines = UniversalNewlineIterator::from(
                        &self.source[self.state.cursor.to_usize()..last_newline + 1],
                    )
                    .count();
                    let line_start = last_newline as u32 + 1;
                    let column = offset.to_u32() - line_start;
                    (lines as u32, line_start, column)
                } else {
                    let column = (offset - new_line_start).to_u32();
                    (1, new_line_start.to_u32(), column)
                };
            let line_number = self.state.line_number.saturating_add(lines);
            let (line_end, is_ascii) = if let Some((newline, line_ending)) =
                find_newline(&self.source[line_start as usize..])
            {
                let newline = line_start as usize + newline;
                let is_ascii = self.source[line_start as usize..newline].is_ascii();
                (
                    Some(TextSize::new(newline as u32 + line_ending.len() as u32)),
                    is_ascii,
                )
            } else {
                let is_ascii = self.source[line_start as usize..].is_ascii();
                (None, is_ascii)
            };
            let line_start = TextSize::new(line_start);
            let state = LinearLocatorState {
                line_start,
                line_end,
                line_number,
                cursor: offset,
                is_ascii,
            };
            (column, Some(state))
        } else {
            let column = (offset - self.state.line_start).to_u32();
            (column, None)
        };
        let state = new_state.as_ref().unwrap_or(&self.state);
        let column = if state.is_ascii {
            column
        } else {
            self.source[state.line_start.to_usize()..][..column as usize]
                .chars()
                .count() as u32
        };
        let column = OneIndexed::from_zero_indexed(column);
        #[cfg(debug_assertions)]
        {
            let location = SourceLocation {
                row: state.line_number,
                column,
            };
            let source_code = SourceCode::new(self.source, &self.index);
            assert_eq!(
                location,
                source_code.source_location(offset),
                "input: {} -> {} {}",
                self.state.cursor.to_usize(),
                offset.to_usize(),
                &self.source[self.state.cursor.to_usize()..offset.to_usize()]
            );
        }
        (column, new_state)
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

#[test]
fn test_linear_locator() {
    let source = r#"
123456789
abcdefghi

유니코드
    "#
    .strip_prefix(char::is_whitespace)
    .unwrap();
    let mut locator = LinearLocator::new(source);
    let mut random_locator = RandomLocator::new(source);

    let mut test = |(row, col), offset| {
        let input = TextSize::from(offset);
        let expected: SourceLocation = SourceLocation {
            row: OneIndexed::new(row).unwrap(),
            column: OneIndexed::new(col).unwrap(),
        };
        let actual = locator.locate(input);
        let actual2 = random_locator.locate(input);
        assert_eq!(expected, actual);
        assert_eq!(expected, actual2);
    };

    test((1, 1), 0);
    test((1, 6), 5);
    test((1, 9), 8);
    test((2, 1), 10);
    test((4, 1), 21);
    test((4, 3), 27);
}
