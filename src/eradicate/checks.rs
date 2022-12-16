use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::autofix::Fix;
use crate::checks::{CheckCode, CheckKind};
use crate::eradicate::detection::comment_contains_code;
use crate::settings::flags;
use crate::{Check, Settings, SourceCodeLocator};

fn is_standalone_comment(line: &str) -> bool {
    for char in line.chars() {
        if char == '#' {
            return true;
        } else if !char.is_whitespace() {
            return false;
        }
    }
    unreachable!("Comment should contain '#' character")
}

/// ERA001
pub fn commented_out_code(
    locator: &SourceCodeLocator,
    start: Location,
    end: Location,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Option<Check> {
    let location = Location::new(start.row(), 0);
    let end_location = Location::new(end.row() + 1, 0);
    let line = locator.slice_source_code_range(&Range {
        location,
        end_location,
    });

    // Verify that the comment is on its own line, and that it contains code.
    if is_standalone_comment(&line) && comment_contains_code(&line) {
        let mut check = Check::new(
            CheckKind::CommentedOutCode,
            Range {
                location: start,
                end_location: end,
            },
        );
        if matches!(autofix, flags::Autofix::Enabled)
            && settings.fixable.contains(&CheckCode::ERA001)
        {
            check.amend(Fix::deletion(location, end_location));
        }
        Some(check)
    } else {
        None
    }
}
