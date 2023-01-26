use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::Diagnostic;
use crate::source_code::Stylist;
use crate::violations;

/// W292
pub fn no_newline_at_end_of_file(
    stylist: &Stylist,
    contents: &str,
    autofix: bool,
) -> Option<Diagnostic> {
    if !contents.ends_with('\n') {
        // Note: if `lines.last()` is `None`, then `contents` is empty (and so we don't
        // want to raise W292 anyway).
        if let Some(line) = contents.lines().last() {
            // Both locations are at the end of the file (and thus the same).
            let location = Location::new(contents.lines().count(), line.len());
            let mut diagnostic = Diagnostic::new(
                violations::NoNewLineAtEndOfFile,
                Range::new(location, location),
            );
            if autofix {
                diagnostic.amend(Fix::insertion(stylist.line_ending().to_string(), location));
            }
            return Some(diagnostic);
        }
    }
    None
}
