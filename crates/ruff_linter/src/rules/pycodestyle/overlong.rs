use std::ops::Deref;

use ruff_python_trivia::{is_pragma_comment, CommentRanges};
use ruff_source_file::Line;
use ruff_text_size::{TextLen, TextRange};

use crate::line_width::{IndentWidth, LineLength, LineWidthBuilder};

#[derive(Debug)]
pub(super) struct Overlong {
    range: TextRange,
    width: usize,
}

impl Overlong {
    /// Returns an [`Overlong`] if the measured line exceeds the configured line length, or `None`
    /// otherwise.
    pub(super) fn try_from_line(
        line: &Line,
        comment_ranges: &CommentRanges,
        limit: LineLength,
        task_tags: &[String],
        tab_size: IndentWidth,
    ) -> Option<Self> {
        // The maximum width of the line is the number of bytes multiplied by the tab size (the
        // worst-case scenario is that the line is all tabs). If the maximum width is less than the
        // limit, then the line is not overlong.
        let max_width = line.len() * tab_size.as_usize();
        if max_width < limit.value() as usize {
            return None;
        }

        // Measure the line. If it's already below the limit, exit early.
        let width = measure(line.as_str(), tab_size);
        if width <= limit {
            return None;
        }

        // Strip trailing comments and re-measure the line, if needed.
        let line = StrippedLine::from_line(line, comment_ranges, task_tags);
        let width = match &line {
            StrippedLine::WithoutPragma(line) => {
                let width = measure(line.as_str(), tab_size);
                if width <= limit {
                    return None;
                }
                width
            }
            StrippedLine::Unchanged(_) => width,
        };

        let mut chunks = line.split_whitespace();
        let (Some(first_chunk), Some(second_chunk)) = (chunks.next(), chunks.next()) else {
            // Single word / no printable chars - no way to make the line shorter.
            return None;
        };

        // Do not enforce the line length for lines that end with a URL, as long as the URL
        // begins before the limit.
        let last_chunk = chunks.last().unwrap_or(second_chunk);
        if last_chunk.contains("://") {
            if width.get() - measure(last_chunk, tab_size).get() <= limit.value() as usize {
                return None;
            }
        }

        // Do not enforce the line length limit for SPDX license headers, which are machine-readable
        // and explicitly _not_ recommended to wrap over multiple lines.
        if matches!(
            (first_chunk, second_chunk),
            ("#", "SPDX-License-Identifier:" | "SPDX-FileCopyrightText:")
        ) {
            return None;
        }

        // Obtain the start offset of the part of the line that exceeds the limit.
        let mut start_offset = line.start();
        let mut start_width = LineWidthBuilder::new(tab_size);
        for c in line.chars() {
            if start_width < limit {
                start_offset += c.text_len();
                start_width = start_width.add_char(c);
            } else {
                break;
            }
        }

        Some(Self {
            range: TextRange::new(start_offset, line.end()),
            width: width.get(),
        })
    }

    /// Return the range of the overlong portion of the line.
    pub(super) const fn range(&self) -> TextRange {
        self.range
    }

    /// Return the measured width of the line, without any trailing pragma comments.
    pub(super) const fn width(&self) -> usize {
        self.width
    }
}

/// A [`Line`] that may have trailing pragma comments stripped.
#[derive(Debug)]
enum StrippedLine<'a> {
    /// The [`Line`] was unchanged.
    Unchanged(&'a Line<'a>),
    /// The [`Line`] was changed such that a trailing pragma comment (e.g., `# type: ignore`) was
    /// removed. The stored [`Line`] consists of the portion of the original line that precedes the
    /// pragma comment.
    WithoutPragma(Line<'a>),
}

impl<'a> StrippedLine<'a> {
    /// Strip trailing comments from a [`Line`], if the line ends with a pragma comment (like
    /// `# type: ignore`) or, if necessary, a task comment (like `# TODO`).
    fn from_line(line: &'a Line<'a>, comment_ranges: &CommentRanges, task_tags: &[String]) -> Self {
        let [comment_range] = comment_ranges.comments_in_range(line.range()) else {
            return Self::Unchanged(line);
        };

        // Convert from absolute to relative range.
        let comment_range = comment_range - line.start();
        let comment = &line.as_str()[comment_range];

        // Ex) `# type: ignore`
        if is_pragma_comment(comment) {
            // Remove the pragma from the line.
            let prefix = &line.as_str()[..usize::from(comment_range.start())].trim_end();
            return Self::WithoutPragma(Line::new(prefix, line.start()));
        }

        // Ex) `# TODO(charlie): ...`
        if !task_tags.is_empty() {
            let Some(trimmed) = comment.strip_prefix('#') else {
                return Self::Unchanged(line);
            };
            let trimmed = trimmed.trim_start();
            if task_tags
                .iter()
                .any(|task_tag| trimmed.starts_with(task_tag))
            {
                // Remove the task tag from the line.
                let prefix = &line.as_str()[..usize::from(comment_range.start())].trim_end();
                return Self::WithoutPragma(Line::new(prefix, line.start()));
            }
        }

        Self::Unchanged(line)
    }
}

impl<'a> Deref for StrippedLine<'a> {
    type Target = Line<'a>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Unchanged(line) => line,
            Self::WithoutPragma(line) => line,
        }
    }
}

/// Returns the width of a given string, accounting for the tab size.
fn measure(s: &str, tab_size: IndentWidth) -> LineWidthBuilder {
    let mut width = LineWidthBuilder::new(tab_size);
    width = width.add_str(s);
    width
}
