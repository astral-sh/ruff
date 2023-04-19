use ruff_python_ast::helpers::{create_expr, unparse_expr};
use ruff_python_ast::source_code::Stylist;
use ruff_python_ast::types::Range;
use rustpython_parser::ast::{Cmpop, Expr, ExprKind, Location};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub fn is_ambiguous_name(name: &str) -> bool {
    name == "l" || name == "I" || name == "O"
}

pub fn compare(left: &Expr, ops: &[Cmpop], comparators: &[Expr], stylist: &Stylist) -> String {
    unparse_expr(
        &create_expr(ExprKind::Compare {
            left: Box::new(left.clone()),
            ops: ops.to_vec(),
            comparators: comparators.to_vec(),
        }),
        stylist,
    )
}

pub(super) fn is_overlong(
    line: &str,
    limit: usize,
    ignore_overlong_task_comments: bool,
    task_tags: &[String],
) -> Option<Overlong> {
    let mut start_column = 0;
    let mut width = 0;
    let mut end = 0;

    for c in line.chars() {
        if width < limit {
            start_column += 1;
        }

        width += c.width().unwrap_or(0);
        end += 1;
    }

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
        if width - last_chunk.width() <= limit {
            return None;
        }
    }

    Some(Overlong {
        column: start_column,
        end_column: end,
        width,
    })
}

pub(super) struct Overlong {
    column: usize,
    end_column: usize,
    width: usize,
}

impl Overlong {
    pub(super) fn range(&self, line_no: usize) -> Range {
        Range::new(
            Location::new(line_no + 1, self.column),
            Location::new(line_no + 1, self.end_column),
        )
    }

    pub(super) const fn width(&self) -> usize {
        self.width
    }
}
