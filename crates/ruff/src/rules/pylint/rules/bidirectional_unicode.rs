use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::ast::Expr;

use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::registry::Diagnostic;
use crate::violation::Violation;

const BIDI_UNICODE: [char; 10] = [
    '\u{202A}', //{LEFT-TO-RIGHT EMBEDDING}
    '\u{202B}', //{RIGHT-TO-LEFT EMBEDDING}
    '\u{202C}', //{POP DIRECTIONAL FORMATTING}
    '\u{202D}', //{LEFT-TO-RIGHT OVERRIDE}
    '\u{202E}', //{RIGHT-TO-LEFT OVERRIDE}
    '\u{2066}', //{LEFT-TO-RIGHT ISOLATE}
    '\u{2067}', //{RIGHT-TO-LEFT ISOLATE}
    '\u{2068}', //{FIRST STRONG ISOLATE}
    '\u{2069}', //{POP DIRECTIONAL ISOLATE}
    // The following was part of PEP 672:
    // https://www.python.org/dev/peps/pep-0672/
    // so the list above might not be complete
    '\u{200F}', //{RIGHT-TO-LEFT MARK}
                // We don't use
                //   "\u200E" # \n{LEFT-TO-RIGHT MARK}
                // as this is the default for latin files and can't be used
                // to hide code
];

define_violation!(
    pub struct BidirectionalUnicode;
);
impl Violation for BidirectionalUnicode {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid using bidirectional unicode")
    }
}

/// PLE2502
pub fn bidirectional_unicode(checker: &mut Checker, expr: &Expr, value: &str) {
    if value.contains(BIDI_UNICODE) {
        checker.diagnostics.push(Diagnostic::new(
            BidirectionalUnicode,
            Range::from_located(expr),
        ));
    }
}
