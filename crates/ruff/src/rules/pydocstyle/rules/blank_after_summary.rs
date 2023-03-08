use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::checkers::ast::Checker;
use crate::docstrings::definition::Docstring;
use crate::fix::Fix;
use crate::message::Location;
use crate::registry::{AsRule, Diagnostic};
use crate::rules::pydocstyle::helpers::LinesWhenConsiderLineContinuation;
use crate::violation::{AutofixKind, Availability, Violation};

#[violation]
pub struct BlankLineAfterSummary {
    pub num_lines: usize,
}

fn fmt_blank_line_after_summary_autofix_msg(info: &BlankLineAfterSummary) -> String {
    if info.num_lines == 0 {
        return "Insert single blank line".to_string();
    }
    "Remove redundant blank lines".to_string()
}

impl Violation for BlankLineAfterSummary {
    const AUTOFIX: Option<AutofixKind> = Some(AutofixKind::new(Availability::Sometimes));

    #[derive_message_formats]
    fn message(&self) -> String {
        let BlankLineAfterSummary { num_lines } = self;
        if *num_lines == 0 {
            format!("1 blank line required between summary line and description")
        } else {
            format!(
                "1 blank line required between summary line and description (found {num_lines})"
            )
        }
    }

    fn autofix_title_formatter(&self) -> Option<fn(&Self) -> String> {
        Some(fmt_blank_line_after_summary_autofix_msg)
    }
}

enum Strategy {
    // start
    InsertNewLineOrLineContinuation(usize),
    // start, count, total_blank_lines
    RemoveRedundantLine(usize, usize, usize),
    None,
}

fn do_raw_string_line_count(content: &str) -> Strategy {
    // skip the summary line
    let mut lines = content.trim().lines().skip(1);

    // Check if there are content that immediately follow the summary
    let has_follower = match lines.next() {
        Some(line) => !line.trim().is_empty(),
        None => return Strategy::None,
    };

    // If we have follower, we need to insert a new line
    // or add line continuation character to separate it from the one-line summary
    if has_follower {
        return Strategy::InsertNewLineOrLineContinuation(1);
    }

    // We have one line that separates summary and its follower
    // But we may have redundant blank lines
    let mut blanks_to_remove = 0_usize;
    for line in lines {
        if !line.trim().is_empty() {
            break;
        }
        blanks_to_remove += 1;
    }

    // No redundant blank lines
    if blanks_to_remove == 0 {
        return Strategy::None;
    }

    // start = 1 summary + 1 blank line
    // count = blanks line to remove
    // total blank = count + 1
    Strategy::RemoveRedundantLine(2, blanks_to_remove, blanks_to_remove + 1)
}

// Behaviors are similar to above
fn do_normal_string_line_count(content: &str) -> Strategy {
    let mut lines = LinesWhenConsiderLineContinuation::from(content.trim());

    // Considering line continuation means that summary can >= 1
    let mut summary_lines_count = 0_usize;
    match lines.next() {
        Some((_, actual_lines)) => {
            summary_lines_count += actual_lines;
        }
        None => {
            return Strategy::None;
        }
    };

    // We need to count the actual lines of the follower in order to propose fix
    let (has_follower, actual_lines_read) = match lines.next() {
        Some((line, actual_lines_read)) => (!line.trim().is_empty(), actual_lines_read),
        None => return Strategy::None,
    };

    if has_follower {
        return Strategy::InsertNewLineOrLineContinuation(summary_lines_count);
    }

    let mut blanks_count = 1;
    let mut actual_lines_to_remove = 0_usize;
    for (line, actual_line_count) in lines {
        if !line.trim().is_empty() {
            break;
        }
        blanks_count += 1;
        actual_lines_to_remove += actual_line_count;
    }

    if blanks_count == 1 {
        return Strategy::None;
    }

    Strategy::RemoveRedundantLine(
        summary_lines_count + actual_lines_read,
        actual_lines_to_remove,
        blanks_count,
    )
}

/// D205
pub fn blank_after_summary(checker: &mut Checker, docstring: &Docstring) {
    let body = docstring.body;

    // Consider r and ur docstring
    let strategy = (|| {
        let contents = docstring.contents;
        if contents.starts_with('r') || contents.starts_with("ur") {
            return do_raw_string_line_count(body);
        }
        do_normal_string_line_count(body)
    })();

    // Count # of lines trimmed when doing line count
    let count_trimmed_lines = || {
        // Find the "summary" line (defined as the first non-blank line).
        let mut trimmed_lines = 0;
        for line in body.lines() {
            if !line.trim().is_empty() {
                break;
            }
            trimmed_lines += 1;
        }
        trimmed_lines
    };

    match strategy {
        Strategy::InsertNewLineOrLineContinuation(start) => {
            let mut diagnostic = Diagnostic::new(
                BlankLineAfterSummary { num_lines: 0 },
                Range::from(docstring.expr),
            );

            // Assume users prefer a new line rather than a line continuation
            let summary_line = count_trimmed_lines();
            if checker.patch(diagnostic.kind.rule()) {
                let start = docstring.expr.location.row() + summary_line + start;
                diagnostic.amend(Fix::insertion(
                    checker.stylist.line_ending().to_string(),
                    Location::new(start, 0),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
        Strategy::RemoveRedundantLine(start, count, blanks_count) => {
            let mut diagnostic = Diagnostic::new(
                BlankLineAfterSummary {
                    num_lines: blanks_count,
                },
                Range::from(docstring.expr),
            );

            let summary_line = count_trimmed_lines();
            if checker.patch(diagnostic.kind.rule()) {
                let start = docstring.expr.location.row() + summary_line + start;
                diagnostic.amend(Fix::deletion(
                    Location::new(start, 0),
                    Location::new(start + count, 0),
                ));
            }
            checker.diagnostics.push(diagnostic);
        }
        Strategy::None => {}
    }
}
