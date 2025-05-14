//! Sort and group diagnostics by line number, so they can be correlated with assertions.
//!
//! We don't assume that we will get the diagnostics in source order.

use ruff_db::diagnostic::Diagnostic;
use ruff_source_file::{LineIndex, OneIndexed};
use std::ops::{Deref, Range};

/// All diagnostics for one embedded Python file, sorted and grouped by start line number.
///
/// The diagnostics are kept in a flat vector, sorted by line number. A separate vector of
/// [`LineDiagnosticRange`] has one entry for each contiguous slice of the diagnostics vector
/// containing diagnostics which all start on the same line.
#[derive(Debug)]
pub(crate) struct SortedDiagnostics<'a> {
    diagnostics: Vec<&'a Diagnostic>,
    line_ranges: Vec<LineDiagnosticRange>,
}

impl<'a> SortedDiagnostics<'a> {
    pub(crate) fn new(
        diagnostics: impl IntoIterator<Item = &'a Diagnostic>,
        line_index: &LineIndex,
    ) -> Self {
        let mut diagnostics: Vec<_> = diagnostics
            .into_iter()
            .map(|diagnostic| DiagnosticWithLine {
                line_number: diagnostic
                    .primary_span()
                    .and_then(|span| span.range())
                    .map_or(OneIndexed::from_zero_indexed(0), |range| {
                        line_index.line_index(range.start())
                    }),
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

    pub(crate) fn iter_lines(&self) -> LineDiagnosticsIterator<'_> {
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
pub(crate) struct LineDiagnosticsIterator<'a> {
    diagnostics: &'a [&'a Diagnostic],
    inner: std::slice::Iter<'a, LineDiagnosticRange>,
}

impl<'a> Iterator for LineDiagnosticsIterator<'a> {
    type Item = LineDiagnostics<'a>;

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

impl std::iter::FusedIterator for LineDiagnosticsIterator<'_> {}

/// All diagnostics that start on a single line of source code in one embedded Python file.
#[derive(Debug)]
pub(crate) struct LineDiagnostics<'a> {
    /// Line number on which these diagnostics start.
    pub(crate) line_number: OneIndexed,

    /// Diagnostics starting on this line.
    pub(crate) diagnostics: &'a [&'a Diagnostic],
}

impl<'a> Deref for LineDiagnostics<'a> {
    type Target = [&'a Diagnostic];

    fn deref(&self) -> &Self::Target {
        self.diagnostics
    }
}

#[derive(Debug)]
struct DiagnosticWithLine<'a> {
    line_number: OneIndexed,
    diagnostic: &'a Diagnostic,
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use ruff_db::diagnostic::{Annotation, Diagnostic, DiagnosticId, LintName, Severity, Span};
    use ruff_db::files::system_path_to_file;
    use ruff_db::source::line_index;
    use ruff_db::system::DbWithWritableSystem as _;
    use ruff_source_file::OneIndexed;
    use ruff_text_size::{TextRange, TextSize};

    #[test]
    fn sort_and_group() {
        let mut db = Db::setup();
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
            .map(|range| {
                let mut diag = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("dummy")),
                    Severity::Error,
                    "dummy",
                );
                let span = Span::from(file).with_range(range);
                diag.annotate(Annotation::primary(span));
                diag
            })
            .collect();

        let sorted = super::SortedDiagnostics::new(diagnostics.iter(), &lines);
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
