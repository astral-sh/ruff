//! Sort and group check outputs (diagnostics and hover results) by line number,
//! so they can be correlated with assertions.
//!
//! We don't assume that we will get the outputs in source order.

use ruff_db::diagnostic::Diagnostic;
use ruff_source_file::{LineIndex, OneIndexed};
use std::ops::Range;

use crate::hover::HoverOutput;

/// Represents either a diagnostic or a hover result for matching against assertions.
#[derive(Debug, Clone)]
pub(crate) enum CheckOutput {
    /// A regular diagnostic from the type checker
    Diagnostic(Diagnostic),

    /// A hover result for testing hover assertions
    Hover(HoverOutput),
}

impl CheckOutput {
    fn line_number(&self, line_index: &LineIndex) -> OneIndexed {
        match self {
            CheckOutput::Diagnostic(diag) => diag
                .primary_span()
                .and_then(|span| span.range())
                .map_or(OneIndexed::from_zero_indexed(0), |range| {
                    line_index.line_index(range.start())
                }),
            CheckOutput::Hover(hover) => line_index.line_index(hover.offset),
        }
    }
}

/// All check outputs for one embedded Python file, sorted and grouped by line number.
///
/// The outputs are kept in a flat vector, sorted by line number. A separate vector of
/// [`LineOutputRange`] has one entry for each contiguous slice of the `outputs` vector
/// containing outputs which all start on the same line.
#[derive(Debug)]
pub(crate) struct SortedCheckOutputs<'a> {
    outputs: Vec<&'a CheckOutput>,
    line_ranges: Vec<LineOutputRange>,
}

impl<'a> SortedCheckOutputs<'a> {
    pub(crate) fn new(
        outputs: impl IntoIterator<Item = &'a CheckOutput>,
        line_index: &LineIndex,
    ) -> Self {
        let mut outputs: Vec<_> = outputs
            .into_iter()
            .map(|output| OutputWithLine {
                line_number: output.line_number(line_index),
                output,
            })
            .collect();
        outputs.sort_unstable_by_key(|output_with_line| output_with_line.line_number);

        let mut result = Self {
            outputs: Vec::with_capacity(outputs.len()),
            line_ranges: vec![],
        };

        let mut current_line_number = None;
        let mut start = 0;
        for OutputWithLine {
            line_number,
            output,
        } in outputs
        {
            match current_line_number {
                None => {
                    current_line_number = Some(line_number);
                }
                Some(current) => {
                    if line_number != current {
                        let end = result.outputs.len();
                        result.line_ranges.push(LineOutputRange {
                            line_number: current,
                            output_index_range: start..end,
                        });
                        start = end;
                        current_line_number = Some(line_number);
                    }
                }
            }
            result.outputs.push(output);
        }
        if let Some(line_number) = current_line_number {
            result.line_ranges.push(LineOutputRange {
                line_number,
                output_index_range: start..result.outputs.len(),
            });
        }

        result
    }

    pub(crate) fn iter_lines(&self) -> LineCheckOutputsIterator<'_> {
        LineCheckOutputsIterator {
            outputs: self.outputs.as_slice(),
            inner: self.line_ranges.iter(),
        }
    }
}

#[derive(Debug)]
struct OutputWithLine<'a> {
    line_number: OneIndexed,
    output: &'a CheckOutput,
}

/// Range delineating check outputs in [`SortedCheckOutputs`] that belong to a single line.
#[derive(Debug)]
struct LineOutputRange {
    line_number: OneIndexed,
    output_index_range: Range<usize>,
}

/// Iterator to group sorted check outputs by line.
pub(crate) struct LineCheckOutputsIterator<'a> {
    outputs: &'a [&'a CheckOutput],
    inner: std::slice::Iter<'a, LineOutputRange>,
}

impl<'a> Iterator for LineCheckOutputsIterator<'a> {
    type Item = LineCheckOutputs<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let LineOutputRange {
            line_number,
            output_index_range,
        } = self.inner.next()?;
        Some(LineCheckOutputs {
            line_number: *line_number,
            outputs: &self.outputs[output_index_range.clone()],
        })
    }
}

impl std::iter::FusedIterator for LineCheckOutputsIterator<'_> {}

/// All check outputs that belong to a single line of source code in one embedded Python file.
#[derive(Debug)]
pub(crate) struct LineCheckOutputs<'a> {
    /// Line number on which these outputs start.
    pub(crate) line_number: OneIndexed,

    /// Check outputs starting on this line.
    pub(crate) outputs: &'a [&'a CheckOutput],
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

        let check_outputs: Vec<_> = ranges
            .into_iter()
            .map(|range| {
                let mut diag = Diagnostic::new(
                    DiagnosticId::Lint(LintName::of("dummy")),
                    Severity::Error,
                    "dummy",
                );
                let span = Span::from(file).with_range(range);
                diag.annotate(Annotation::primary(span));
                super::CheckOutput::Diagnostic(diag)
            })
            .collect();

        let sorted = super::SortedCheckOutputs::new(&check_outputs, &lines);
        let grouped = sorted.iter_lines().collect::<Vec<_>>();

        let [line1, line2] = &grouped[..] else {
            panic!("expected two lines");
        };

        assert_eq!(line1.line_number, OneIndexed::from_zero_indexed(0));
        assert_eq!(line1.outputs.len(), 2);
        assert_eq!(line2.line_number, OneIndexed::from_zero_indexed(1));
        assert_eq!(line2.outputs.len(), 1);
    }
}
