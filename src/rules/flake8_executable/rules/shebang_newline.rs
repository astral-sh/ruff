use ruff_macros::derive_message_formats;
use rustpython_ast::Location;

use crate::ast::types::Range;
use crate::define_simple_violation;
use crate::registry::Diagnostic;
use crate::rules::flake8_executable::helpers::ShebangDirective;
use crate::violation::Violation;

define_simple_violation!(
    ShebangNewline,
    "Shebang should be at the beginning of the file"
);

/// EXE005
pub fn shebang_newline(lineno: usize, shebang: &ShebangDirective) -> Option<Diagnostic> {
    if let ShebangDirective::Match(_, start, end, _) = shebang {
        if lineno > 1 {
            let diagnostic = Diagnostic::new(
                ShebangNewline,
                Range::new(
                    Location::new(lineno + 1, *start),
                    Location::new(lineno + 1, *end),
                ),
            );
            Some(diagnostic)
        } else {
            None
        }
    } else {
        None
    }
}
