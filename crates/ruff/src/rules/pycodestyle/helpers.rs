use ruff_python_ast::{CmpOp, Expr, Ranged};
use ruff_text_size::{TextLen, TextRange};
use unicode_width::UnicodeWidthStr;

use ruff_source_file::{Line, Locator};

use crate::line_width::{LineLength, LineWidth, TabSize};

pub(super) fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

pub(super) fn compare(
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
    locator: &Locator,
) -> String {
    let start = left.start();
    let end = comparators.last().map_or_else(|| left.end(), Ranged::end);
    let mut contents = String::with_capacity(usize::from(end - start));

    // Add the left side of the comparison.
    contents.push_str(locator.slice(left.range()));

    for (op, comparator) in ops.iter().zip(comparators) {
        // Add the operator.
        contents.push_str(match op {
            CmpOp::Eq => " == ",
            CmpOp::NotEq => " != ",
            CmpOp::Lt => " < ",
            CmpOp::LtE => " <= ",
            CmpOp::Gt => " > ",
            CmpOp::GtE => " >= ",
            CmpOp::In => " in ",
            CmpOp::NotIn => " not in ",
            CmpOp::Is => " is ",
            CmpOp::IsNot => " is not ",
        });

        // Add the right side of the comparison.
        contents.push_str(locator.slice(comparator.range()));
    }

    contents
}

pub(super) fn is_overlong(
    line: &Line,
    limit: LineLength,
    ignore_overlong_task_comments: bool,
    task_tags: &[String],
    tab_size: TabSize,
) -> Option<Overlong> {
    // Each character is between 1-4 bytes. If the number of bytes is smaller than the limit, it cannot be overlong.
    if line.len() < limit.get() {
        return None;
    }

    let mut width = LineWidth::new(tab_size);
    width = width.add_str(line.as_str());
    if width <= limit {
        return None;
    }

    let mut chunks = line.split_whitespace();
    let (Some(first_chunk), Some(second_chunk)) = (chunks.next(), chunks.next()) else {
        // Single word / no printable chars - no way to make the line shorter
        return None;
    };

    if first_chunk == "#" {
        if ignore_overlong_task_comments {
            let second = second_chunk.trim_end_matches(':');
            if task_tags.iter().any(|task_tag| task_tag == second) {
                return None;
            }
        }
    }

    // Do not enforce the line length for lines that end with a URL, as long as the URL
    // begins before the limit.
    let last_chunk = chunks.last().unwrap_or(second_chunk);
    if last_chunk.contains("://") {
        if width.get() - last_chunk.width() <= limit.get() {
            return None;
        }
    }

    // Obtain the start offset of the part of the line that exceeds the limit
    let mut start_offset = line.start();
    let mut start_width = LineWidth::new(tab_size);
    for c in line.chars() {
        if start_width < limit {
            start_offset += c.text_len();
            start_width = start_width.add_char(c);
        } else {
            break;
        }
    }
    Some(Overlong {
        range: TextRange::new(start_offset, line.end()),
        width: width.get(),
    })
}

pub(super) struct Overlong {
    range: TextRange,
    width: usize,
}

impl Overlong {
    pub(super) const fn range(&self) -> TextRange {
        self.range
    }

    pub(super) const fn width(&self) -> usize {
        self.width
    }
}
