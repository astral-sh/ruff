#![allow(dead_code, unused_imports, unused_variables)]

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::types::Range;

use crate::registry::AsRule;
use crate::rules::pycodestyle::helpers::{is_keyword_token, is_op_token, is_soft_keyword_token};

#[violation]
pub struct WhitespaceBeforeParameters {
    pub bracket: String,
}

impl AlwaysAutofixableViolation for WhitespaceBeforeParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        let WhitespaceBeforeParameters { bracket } = self;
        format!("Whitespace before {bracket}")
    }

    fn autofix_title(&self) -> String {
        let WhitespaceBeforeParameters { bracket } = self;
        format!("Removed whitespace before {bracket}")
    }
}

/// E211
#[cfg(debug_assertions)]
pub fn whitespace_before_parameters(
    tokens: &[(Location, &Tok, Location)],
    autofix: bool,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    let (_, mut prev_token, mut prev_end) = tokens.first().unwrap();
    for (idx, (start, tok, end)) in tokens.iter().enumerate() {
        if is_op_token(tok)
            && (**tok == Tok::Lpar || **tok == Tok::Lsqb)
            && *start != prev_end
            && (matches!(prev_token, Tok::Name { .. })
                || matches!(prev_token, Tok::Rpar | Tok::Rsqb | Tok::Rbrace))
            && (idx < 2 || *(tokens[idx - 2].1) != Tok::Class)
            && !is_keyword_token(tok)
            && !is_soft_keyword_token(tok)
        {
            let start = Location::new(prev_end.row(), prev_end.column());
            let end = Location::new(end.row(), end.column() - 1);

            let kind: WhitespaceBeforeParameters = WhitespaceBeforeParameters {
                bracket: tok.to_string(),
            };

            let mut diagnostic = Diagnostic::new(kind, Range::new(start, end));

            if autofix {
                diagnostic.amend(Fix::deletion(start, end));
            }
            diagnostics.push(diagnostic);
        }
        prev_token = *tok;
        prev_end = *end;
    }
    diagnostics
}

#[cfg(not(debug_assertions))]
pub fn whitespace_before_parameters(
    _tokens: &[(Location, &Tok, Location)],
    _autofix: bool,
) -> Vec<Diagnostic> {
    vec![]
}
