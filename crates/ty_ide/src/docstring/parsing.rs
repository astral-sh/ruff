use ruff_python_trivia::leading_indentation;
use ruff_source_file::UniversalNewlines;

/// Calculate indentation width, treating tabs like Python does.
pub(super) fn indentation(line: &str) -> usize {
    leading_indentation(line)
        .chars()
        .map(|char| if char == '\t' { 8 } else { 1 })
        .sum()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::docstring) struct ParsedLine<'a> {
    pub(in crate::docstring) text: &'a str,
    pub(in crate::docstring) start: usize,
    pub(in crate::docstring) end: usize,
}

pub(in crate::docstring) fn parsed_lines(raw: &str) -> Vec<ParsedLine<'_>> {
    raw.universal_newlines()
        .map(|line| ParsedLine {
            text: line.as_str(),
            start: line.start().to_usize(),
            end: line.end().to_usize(),
        })
        .collect()
}
