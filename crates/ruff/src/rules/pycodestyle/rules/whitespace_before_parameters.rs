#![allow(dead_code, unused_imports, unused_variables)]

use rustpython_parser::ast::Location;
use rustpython_parser::Tok;

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;
use ruff_python_ast::types::Range;

use crate::registry::AsRule;
use crate::rules::pycodestyle::helpers::{is_keyword_token, is_op_token, is_soft_keyword_token};
use crate::rules::pycodestyle::logical_lines::LogicalLineTokens;

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
#[cfg(feature = "logical_lines")]
pub fn whitespace_before_parameters(tokens: &LogicalLineTokens, autofix: bool) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    let previous = tokens.first().unwrap();

    let mut pre_pre_kind: Option<TokenKind> = None;
    let mut prev_token = previous.kind();
    let mut prev_end = previous.end();

    for (idx, token) in tokens.iter().enumerate() {
        let kind = token.kind();

        if (kind == TokenKind::Lpar || kind == TokenKind::Lsqb)
            && token.start() != prev_end
            && matches!(
                prev_token,
                TokenKind::Name | TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
            )
            && (pre_pre_kind != Some(TokenKind::Class))
        {
            let start = Location::new(prev_end.row(), prev_end.column());
            let end = token.end();
            let end = Location::new(end.row(), end.column() - 1);

            let kind: WhitespaceBeforeParameters = WhitespaceBeforeParameters {
                bracket: if kind == TokenKind::Lpar {
                    "'('"
                } else {
                    "'['"
                }
                .to_string(),
            };

            let mut diagnostic = Diagnostic::new(kind, Range::new(start, end));

            if autofix {
                diagnostic.amend(Edit::deletion(start, end));
            }
            diagnostics.push(diagnostic);
        }
        pre_pre_kind = Some(prev_token);
        prev_token = kind;
        prev_end = token.end();
    }
    diagnostics
}

#[cfg(not(feature = "logical_lines"))]
pub fn whitespace_before_parameters(
    _tokens: &LogicalLineTokens,
    _autofix: bool,
) -> Vec<Diagnostic> {
    vec![]
}
