//! Sort and group diagnostics by line number, so they can be correlated with assertions.
//!
//! We don't assume that we will get the diagnostics in source order.

use red_knot_python_semantic::types::TypeCheckDiagnostic;
use ruff_python_parser::ParseError;
use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::{Ranged, TextRange};
use std::borrow::Cow;
use std::ops::{Deref, Range};

pub(super) trait Diagnostic: std::fmt::Debug {
    fn rule(&self) -> &str;

    fn message(&self) -> Cow<str>;

    fn range(&self) -> TextRange;
}

impl Diagnostic for TypeCheckDiagnostic {
    fn rule(&self) -> &str {
        TypeCheckDiagnostic::rule(self)
    }

    fn message(&self) -> Cow<str> {
        TypeCheckDiagnostic::message(self).into()
    }

    fn range(&self) -> TextRange {
        Ranged::range(self)
    }
}

impl Diagnostic for ParseError {
    fn rule(&self) -> &str {
        "invalid-syntax"
    }

    fn message(&self) -> Cow<str> {
        self.error.to_string().into()
    }

    fn range(&self) -> TextRange {
        self.location
    }
}

impl Diagnostic for Box<dyn Diagnostic> {
    fn rule(&self) -> &str {
        (**self).rule()
    }

    fn message(&self) -> Cow<str> {
        (**self).message()
    }

    fn range(&self) -> TextRange {
        (**self).range()
    }
}

/// All diagnostics for one embedded Python file, sorted and grouped by start line number.
///
/// The diagnostics are kept in a flat vector, sorted by line number. A separate vector of
/// [`LineDiagnosticRange`] has one entry for each contiguous slice of the diagnostics vector
/// containing diagnostics which all start on the same line.
#[derive(Debug)]
pub(crate) struct SortedDiagnostics<T> {
    diagnostics: Vec<T>,
    line_ranges: Vec<LineDiagnosticRange>,
}

impl<T> SortedDiagnostics<T>
where
    T: Diagnostic,
{
    pub(crate) fn new(diagnostics: impl IntoIterator<Item = T>, line_index: &LineIndex) -> Self {
        let mut diagnostics: Vec<_> = diagnostics
            .into_iter()
            .map(|diagnostic| DiagnosticWithLine {
                line_number: line_index.line_index(diagnostic.range().start()),
                diagnostic,
            })
            .collect();
        diagnostics.sort_unstable_by_key(|diagnostic_with_line| diagnostic_with_line.line_number);

        let mut diags = Self {
            diagnostics: Vec::with_capacity(diagnostics.len()),
            line_ranges: vec![],
        };

        let mut current_line_number = None;
        let mut start = 0;
        for DiagnosticWithLine {
            line_number,
            diagnostic,
        } in diagnostics
        {
            match current_line_number {
                None => {
                    current_line_number = Some(line_number);
                }
                Some(current) => {
                    if line_number != current {
                        let end = diags.diagnostics.len();
                        diags.line_ranges.push(LineDiagnosticRange {
                            line_number: current,
                            diagnostic_index_range: start..end,
                        });
                        start = end;
                        current_line_number = Some(line_number);
                    }
                }
            }
            diags.diagnostics.push(diagnostic);
        }
        if let Some(line_number) = current_line_number {
            diags.line_ranges.push(LineDiagnosticRange {
                line_number,
                diagnostic_index_range: start..diags.diagnostics.len(),
            });
        }

        diags
    }

    pub(crate) fn iter_lines(&self) -> LineDiagnosticsIterator<T> {
        LineDiagnosticsIterator {
            diagnostics: self.diagnostics.as_slice(),
            inner: self.line_ranges.iter(),
        }
    }
}

/// Range delineating diagnostics in [`SortedDiagnostics`] that begin on a single line.
#[derive(Debug)]
struct LineDiagnosticRange {
    line_number: OneIndexed,
    diagnostic_index_range: Range<usize>,
}

/// Iterator to group sorted diagnostics by line.
pub(crate) struct LineDiagnosticsIterator<'a, T> {
    diagnostics: &'a [T],
    inner: std::slice::Iter<'a, LineDiagnosticRange>,
}

impl<'a, T> Iterator for LineDiagnosticsIterator<'a, T>
where
    T: Diagnostic,
{
    type Item = LineDiagnostics<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        let LineDiagnosticRange {
            line_number,
            diagnostic_index_range,
        } = self.inner.next()?;
        Some(LineDiagnostics {
            line_number: *line_number,
            diagnostics: &self.diagnostics[diagnostic_index_range.clone()],
        })
    }
}

impl<T> std::iter::FusedIterator for LineDiagnosticsIterator<'_, T> where T: Diagnostic {}

/// All diagnostics that start on a single line of source code in one embedded Python file.
#[derive(Debug)]
pub(crate) struct LineDiagnostics<'a, T> {
    /// Line number on which these diagnostics start.
    pub(crate) line_number: OneIndexed,

    /// Diagnostics starting on this line.
    pub(crate) diagnostics: &'a [T],
}

impl<T> Deref for LineDiagnostics<'_, T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.diagnostics
    }
}

#[derive(Debug)]
struct DiagnosticWithLine<T> {
    line_number: OneIndexed,
    diagnostic: T,
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use crate::diagnostic::Diagnostic;
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::line_index;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_source_file::OneIndexed;
    use ruff_text_size::{TextRange, TextSize};
    use std::borrow::Cow;

    #[test]
    fn sort_and_group() {
        let mut db = Db::setup(SystemPathBuf::from("/src"));
        db.write_file("/src/test.py", "one\ntwo\n").unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        let lines = line_index(&db, file);

        let ranges = [
            TextRange::new(TextSize::new(0), TextSize::new(1)),
            TextRange::new(TextSize::new(5), TextSize::new(10)),
            TextRange::new(TextSize::new(1), TextSize::new(7)),
        ];

        let diagnostics: Vec<_> = ranges
            .into_iter()
            .map(|range| DummyDiagnostic { range })
            .collect();

        let sorted = super::SortedDiagnostics::new(diagnostics, &lines);
        let grouped = sorted.iter_lines().collect::<Vec<_>>();

        let [line1, line2] = &grouped[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line_number, OneIndexed::from_zero_indexed(0));
        assert_eq!(line1.diagnostics.len(), 2);
        assert_eq!(line2.line_number, OneIndexed::from_zero_indexed(1));
        assert_eq!(line2.diagnostics.len(), 1);
    }

    #[derive(Debug)]
    struct DummyDiagnostic {
        range: TextRange,
    }

    impl Diagnostic for DummyDiagnostic {
        fn rule(&self) -> &str {
            "dummy"
        }

        fn message(&self) -> Cow<str> {
            "dummy".into()
        }

        fn range(&self) -> TextRange {
            self.range
        }
    }
}
