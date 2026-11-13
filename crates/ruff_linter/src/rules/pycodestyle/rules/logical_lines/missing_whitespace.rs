use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast::token::{Token, TokenKind};
use ruff_text_size::Ranged;

use crate::Edit;
use crate::checkers::ast::LintContext;
use crate::codes::Category;
use crate::{AlwaysFixableViolation, Fix};

use super::{DefinitionState, LogicalLine};

/// ## What it does
/// Checks for missing whitespace after `,`, `;`, and `:`.
///
/// ## Why is this bad?
/// Missing whitespace after `,`, `;`, and `:` makes the code harder to read.
///
/// ## Example
/// ```python
/// a = (1,2)
/// ```
///
/// Use instead:
/// ```python
/// a = (1, 2)
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(preview_since = "v0.0.269", category = Category::Formatting)]
pub(crate) struct MissingWhitespace {
    token: TokenKind,
}

impl AlwaysFixableViolation for MissingWhitespace {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Missing whitespace after {}", self.token)
    }

    fn fix_title(&self) -> String {
        "Add missing whitespace".to_string()
    }
}

/// E231
pub(crate) fn missing_whitespace(line: &LogicalLine, context: &LintContext) {
    let tokens = line.tokens();
    // Only a colon can require the enclosing brackets or type parameters. Advance
    // from the previous colon so that each preceding token is visited at most once.
    let mut preceding = 0;
    let mut brackets = Vec::new();
    let mut interpolated_strings = 0u32;
    let mut definition = DefinitionState::from_tokens(tokens);
    for (index, token) in tokens.iter().enumerate() {
        let kind = token.kind();
        if !matches!(kind, TokenKind::Comma | TokenKind::Semi | TokenKind::Colon) {
            continue;
        }
        if line
            .text_after(token)
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || c == '\\')
        {
            continue;
        }
        let next_kind = tokens.get(index + 1).map(Token::kind);
        match kind {
            TokenKind::Comma if matches!(next_kind, Some(TokenKind::Rpar | TokenKind::Rsqb)) => {
                continue;
            }
            TokenKind::Colon => {
                if next_kind == Some(TokenKind::Equal) {
                    continue;
                }
                for preceding_token in &tokens[preceding..index] {
                    let kind = preceding_token.kind();
                    definition.visit_token_kind(kind);
                    match kind {
                        TokenKind::FStringStart | TokenKind::TStringStart => {
                            interpolated_strings += 1;
                        }
                        TokenKind::FStringEnd | TokenKind::TStringEnd => {
                            interpolated_strings = interpolated_strings.saturating_sub(1);
                        }
                        TokenKind::Lsqb | TokenKind::Lbrace if interpolated_strings == 0 => {
                            brackets.push(kind);
                        }
                        TokenKind::Rsqb | TokenKind::Rbrace if interpolated_strings == 0 => {
                            brackets.pop();
                        }
                        _ => {}
                    }
                }
                preceding = index + 1;
                // A colon can introduce a format specifier in an f-string. This also
                // ignores dictionary and lambda colons inside replacement fields.
                if interpolated_strings > 0
                    || (next_kind.is_some()
                        && brackets.last() == Some(&TokenKind::Lsqb)
                        && !(definition.in_type_params() && brackets.len() == 1))
                {
                    continue;
                }
            }
            _ => {}
        }
        if let Some(mut diagnostic) =
            context.report_diagnostic_if_enabled(MissingWhitespace { token: kind }, token.range())
        {
            diagnostic.set_fix(Fix::safe_edit(Edit::insertion(
                " ".to_string(),
                token.end(),
            )));
        }
    }
}
