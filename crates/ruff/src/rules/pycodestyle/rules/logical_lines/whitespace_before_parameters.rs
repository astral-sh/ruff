use ruff_text_size::{TextRange, TextSize};

use ruff_diagnostics::{AlwaysAutofixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_ast::token_kind::TokenKind;

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;

#[violation]
pub struct WhitespaceBeforeParameters {
    bracket: TokenKind,
}

impl WhitespaceBeforeParameters {
    fn bracket_text(&self) -> char {
        match self.bracket {
            TokenKind::Lpar => '(',
            TokenKind::Lsqb => '[',
            _ => unreachable!(),
        }
    }
}

impl AlwaysAutofixableViolation for WhitespaceBeforeParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        let bracket = self.bracket_text();
        format!("Whitespace before '{bracket}'")
    }

    fn autofix_title(&self) -> String {
        let bracket = self.bracket_text();
        format!("Removed whitespace before '{bracket}'")
    }
}

/// E211
pub(crate) fn whitespace_before_parameters(
    line: &LogicalLine,
    autofix: bool,
    context: &mut LogicalLinesContext,
) {
    let previous = line.tokens().first().unwrap();

    let mut pre_pre_kind: Option<TokenKind> = None;
    let mut prev_token = previous.kind();
    let mut prev_end = previous.end();

    for token in line.tokens() {
        let kind = token.kind();

        if matches!(kind, TokenKind::Lpar | TokenKind::Lsqb)
            && matches!(
                prev_token,
                TokenKind::Name | TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace
            )
            && (pre_pre_kind != Some(TokenKind::Class))
            && token.start() != prev_end
        {
            let start = prev_end;
            let end = token.end() - TextSize::from(1);
            let kind: WhitespaceBeforeParameters = WhitespaceBeforeParameters { bracket: kind };

            let mut diagnostic = Diagnostic::new(kind, TextRange::new(start, end));

            if autofix {
                diagnostic.set_fix(Fix::automatic(Edit::deletion(start, end)));
            }
            context.push_diagnostic(diagnostic);
        }
        pre_pre_kind = Some(prev_token);
        prev_token = kind;
        prev_end = token.end();
    }
}
