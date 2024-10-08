//! Sort and group diagnostics by line number, so they can be correlated with assertions.
//!
//! We don't assume that we will get the diagnostics in source order.

use ruff_source_file::{LineIndex, OneIndexed};
use ruff_text_size::Ranged;
use smallvec::SmallVec;
use std::ops::Deref;

#[derive(Debug)]
pub(crate) struct SortedDiagnostics<T>(Vec<DiagnosticWithLine<T>>);

impl<T> SortedDiagnostics<T>
where
    T: Ranged + Clone,
{
    pub(crate) fn new(diagnostics: impl IntoIterator<Item = T>, line_index: &LineIndex) -> Self {
        let mut diagnostics: Vec<_> = diagnostics
            .into_iter()
            .map(|diagnostic| DiagnosticWithLine {
                line_number: line_index.line_index(diagnostic.start()),
                diagnostic,
            })
            .collect();
        diagnostics.sort_by_key(|diagnostic_with_line| diagnostic_with_line.line_number);

        Self(diagnostics)
    }

    pub(crate) fn iter_lines(&self) -> LineDiagnosticsIterator<T> {
        LineDiagnosticsIterator {
            inner: self.0.iter(),
        }
    }
}

/// Iterator to group sorted diagnostics by line.
pub(crate) struct LineDiagnosticsIterator<'a, T> {
    inner: std::slice::Iter<'a, DiagnosticWithLine<T>>,
}

impl<T> Iterator for LineDiagnosticsIterator<'_, T>
where
    T: Ranged + Clone,
{
    type Item = LineDiagnostics<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let DiagnosticWithLine {
            line_number: current_line_number,
            diagnostic,
        } = self.inner.next()?;
        let mut diagnostics = DiagnosticVec::new();
        diagnostics.push(diagnostic.clone());
        while let Some(DiagnosticWithLine {
            line_number,
            diagnostic,
        }) = self.inner.clone().next()
        {
            if line_number == current_line_number {
                diagnostics.push(diagnostic.clone());
                self.inner.next();
            } else {
                break;
            }
        }
        Some(LineDiagnostics {
            line_number: *current_line_number,
            diagnostics,
        })
    }
}

impl<T> std::iter::FusedIterator for LineDiagnosticsIterator<'_, T> where T: Clone + Ranged {}

/// A vector of diagnostics belonging to a single line.
///
/// Most lines will have zero or one diagnostics, so we use a [`SmallVec`] optimized for a single
/// element to avoid most heap vector allocations.
type DiagnosticVec<T> = SmallVec<[T; 1]>;

#[derive(Debug)]
pub(crate) struct LineDiagnostics<T> {
    /// Line number on which these diagnostics start.
    pub(crate) line_number: OneIndexed,

    /// Diagnostics starting on this line.
    pub(crate) diagnostics: DiagnosticVec<T>,
}

impl<T> Deref for LineDiagnostics<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.diagnostics
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
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::line_index;
    use ruff_db::system::{DbWithTestSystem, SystemPathBuf};
    use ruff_source_file::OneIndexed;
    use ruff_text_size::{TextRange, TextSize};

    #[test]
    fn sort_and_group() {
        let mut db = Db::setup(SystemPathBuf::from("/src"));
        db.write_file("/src/test.py", "one\ntwo\n").unwrap();
        let file = system_path_to_file(&db, "/src/test.py").unwrap();
        let lines = line_index(&db, file);

        let ranges = vec![
            TextRange::new(TextSize::new(0), TextSize::new(1)),
            TextRange::new(TextSize::new(5), TextSize::new(10)),
            TextRange::new(TextSize::new(1), TextSize::new(7)),
        ];

        let sorted = super::SortedDiagnostics::new(&ranges, &lines);
        let grouped = sorted.iter_lines().collect::<Vec<_>>();

        let [line1, line2] = &grouped[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line_number, OneIndexed::from_zero_indexed(0));
        assert_eq!(line1.diagnostics.len(), 2);
        assert_eq!(line2.line_number, OneIndexed::from_zero_indexed(1));
        assert_eq!(line2.diagnostics.len(), 1);
    }
}
