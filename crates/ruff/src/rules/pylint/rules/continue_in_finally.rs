use crate::ast::types::Range;
use crate::checkers::ast::Checker;
use crate::define_violation;
use crate::registry::Diagnostic;
use crate::violation::Violation;
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use ruff_macros::derive_message_formats;
use rustpython_ast::Stmt;

define_violation!(
    pub struct ContinueInFinally;
);
impl Violation for ContinueInFinally {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Continue is not supported inside a finally block")
    }
}

/// PLE0116
pub fn continue_in_finally(checker: &mut Checker, finalbody: &[Stmt]) {
    if finalbody.is_empty() {
        return;
    }
    let first = finalbody.first().unwrap();
    let last = finalbody.last().unwrap();
    let contents = checker
        .locator
        .slice_source_code_range(&Range::new(first.location, last.end_location.unwrap()));
    for (start, tok, end) in lexer::make_tokenizer(contents).flatten() {
        if tok == Tok::Continue {
            let diagnostic = Diagnostic::new(ContinueInFinally, Range::new(start, end));
            checker.diagnostics.push(diagnostic);
            return;
        }
    }
}
