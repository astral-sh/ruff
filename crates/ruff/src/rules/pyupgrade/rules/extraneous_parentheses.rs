use ruff_macros::{define_violation, derive_message_formats};
use rustpython_parser::lexer::{LexResult, Tok};

use crate::ast::types::Range;
use crate::fix::Fix;
use crate::registry::{Diagnostic, Rule};
use crate::settings::{flags, Settings};
use crate::source_code::Locator;
use crate::violation::AlwaysAutofixableViolation;

define_violation!(
    pub struct ExtraneousParentheses;
);
impl AlwaysAutofixableViolation for ExtraneousParentheses {
    #[derive_message_formats]
    fn message(&self) -> String {
        format!("Avoid extraneous parentheses")
    }

    fn autofix_title(&self) -> String {
        "Remove extraneous parentheses".to_string()
    }
}

// See: https://github.com/asottile/pyupgrade/blob/97ed6fb3cf2e650d4f762ba231c3f04c41797710/pyupgrade/_main.py#L148
fn match_extraneous_parentheses(tokens: &[LexResult], mut i: usize) -> Option<(usize, usize)> {
    i += 1;

    loop {
        if i >= tokens.len() {
            return None;
        }
        let Ok((_, tok, _)) = &tokens[i] else {
            return None;
        };
        match tok {
            Tok::Comment(..) | Tok::NonLogicalNewline => {
                i += 1;
            }
            Tok::Lpar => {
                break;
            }
            _ => {
                return None;
            }
        }
    }

    // Store the location of the extraneous opening parenthesis.
    let start = i;

    // Verify that we're not in a tuple or coroutine.
    let mut depth = 1;
    while depth > 0 {
        i += 1;
        if i >= tokens.len() {
            return None;
        }
        let Ok((_, tok, _)) = &tokens[i] else {
            return None;
        };

        // If we find a comma or a yield at depth 1 or 2, it's a tuple or coroutine.
        if depth == 1 && matches!(tok, Tok::Comma | Tok::Yield) {
            return None;
        } else if matches!(tok, Tok::Lpar | Tok::Lbrace | Tok::Lsqb) {
            depth += 1;
        } else if matches!(tok, Tok::Rpar | Tok::Rbrace | Tok::Rsqb) {
            depth -= 1;
        }
    }

    // Store the location of the extraneous closing parenthesis.
    let end = i;

    // Verify that we're not in an empty tuple.
    if (start + 1..i).all(|i| {
        matches!(
            tokens[i],
            Ok((_, Tok::Comment(..) | Tok::NonLogicalNewline, _))
        )
    }) {
        return None;
    }

    // Find the next non-coding token.
    i += 1;
    loop {
        if i >= tokens.len() {
            return None;
        }
        let Ok((_, tok, _)) = &tokens[i] else {
            return None;
        };
        match tok {
            Tok::Comment(..) | Tok::NonLogicalNewline => {
                i += 1;
            }
            _ => {
                break;
            }
        }
    }

    if i >= tokens.len() {
        return None;
    }
    let Ok((_, tok, _)) = &tokens[i] else {
        return None;
    };
    if matches!(tok, Tok::Rpar) {
        Some((start, end))
    } else {
        None
    }
}

/// UP034
pub fn extraneous_parentheses(
    tokens: &[LexResult],
    locator: &Locator,
    settings: &Settings,
    autofix: flags::Autofix,
) -> Vec<Diagnostic> {
    let mut diagnostics = vec![];
    let mut i = 0;
    while i < tokens.len() {
        if matches!(tokens[i], Ok((_, Tok::Lpar, _))) {
            if let Some((start, end)) = match_extraneous_parentheses(tokens, i) {
                i = end + 1;
                let Ok((start, ..)) = &tokens[start] else {
                    return diagnostics;
                };
                let Ok((.., end)) = &tokens[end] else {
                    return diagnostics;
                };
                let mut diagnostic =
                    Diagnostic::new(ExtraneousParentheses, Range::new(*start, *end));
                if matches!(autofix, flags::Autofix::Enabled)
                    && settings.rules.should_fix(&Rule::ExtraneousParentheses)
                {
                    let contents = locator.slice_source_code_range(&Range::new(*start, *end));
                    diagnostic.amend(Fix::replacement(
                        contents[1..contents.len() - 1].to_string(),
                        *start,
                        *end,
                    ));
                }
                diagnostics.push(diagnostic);
            } else {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    diagnostics
}
