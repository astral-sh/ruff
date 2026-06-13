use ruff_python_trivia::leading_indentation;
use ruff_source_file::UniversalNewlines;

/// Calculate indentation width, treating tabs like Python does.
pub(super) fn indentation(line: &str) -> usize {
    leading_indentation(line)
        .chars()
        .map(|char| if char == '\t' { 8 } else { 1 })
        .sum()
}

pub(super) fn parsed_lines(raw: &str) -> Vec<&str> {
    raw.universal_newlines().map(|line| line.as_str()).collect()
}
