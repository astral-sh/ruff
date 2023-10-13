use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix};
use ruff_macros::{derive_message_formats, violation};
use ruff_python_parser::TokenKind;
use ruff_text_size::{Ranged, TextRange, TextSize};

use crate::checkers::logical_lines::LogicalLinesContext;
use crate::rules::pycodestyle::rules::logical_lines::LogicalLine;

/// ## What it does
/// Checks for extraneous whitespace immediately preceding an open parenthesis
/// or bracket.
///
/// ## Why is this bad?
/// According to [PEP 8], open parentheses and brackets should not be preceded
/// by any trailing whitespace.
///
/// ## Example
/// ```python
/// spam (1)
/// ```
///
/// Use instead:
/// ```python
/// spam(1)
/// ```
///
/// [PEP 8]: https://peps.python.org/pep-0008/#pet-peeves
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

impl AlwaysFixableViolation for WhitespaceBeforeParameters {
    #[derive_message_formats]
    fn message(&self) -> String {
        let bracket = self.bracket_text();
        format!("Whitespace before '{bracket}'")
    }

    fn fix_title(&self) -> String {
        let bracket = self.bracket_text();
        format!("Removed whitespace before '{bracket}'")
    }
}

/// E211
pub(crate) fn whitespace_before_parameters(line: &LogicalLine, context: &mut LogicalLinesContext) {
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
            diagnostic.set_fix(Fix::safe_edit(Edit::deletion(start, end)));
            context.push_diagnostic(diagnostic);
        }
        pre_pre_kind = Some(prev_token);
        prev_token = kind;
        prev_end = token.end();
    }
}
