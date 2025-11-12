use std::error::Error;

use ruff_python_trivia::{CommentRanges, Cursor};
use ruff_text_size::{TextRange, TextSize};
use smallvec::{SmallVec, smallvec};

#[derive(Clone, Debug, Eq, PartialEq)]
enum SuppressionAction {
    Disable,
    Enable,
    Ignore,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SuppressionKind {
    /// multi-line range suppression
    Range,

    /// next-line suppression
    NextLine,

    /// end-of-line suppression
    EndOfLine,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct SuppressionComment {
    /// Range containing the entire suppression comment
    range: TextRange,

    /// The action directive
    action: SuppressionAction,

    /// Ranges containing the lint codes being suppressed
    codes: SmallVec<[TextRange; 2]>,

    /// Range containing the reason for the suppression
    reason: TextRange,
}

#[derive(Debug)]
pub(crate) struct Suppression {
    kind: SuppressionKind,

    /// The lint code being suppressed
    code: String,

    /// Range for which the suppression applies
    range: TextRange,

    /// Any comments associated with the suppression
    comments: Vec<SuppressionComment>,
}

struct SuppressionParser<'src> {
    cursor: Cursor<'src>,
    range: TextRange,
}

impl<'src> SuppressionParser<'src> {
    fn new(source: &'src str, range: TextRange) -> Self {
        let cursor = Cursor::new(&source[range]);
        Self { cursor, range }
    }

    fn parse_comment(&mut self) -> Option<SuppressionComment> {
        self.cursor.start_token();

        if !self.cursor.eat_char('#') {
            return None;
        }

        self.eat_whitespace();

        let action = self.eat_action()?;
        let codes = self.eat_codes()?;

        self.eat_whitespace();
        let reason = TextRange::new(self.offset(), self.range.end());

        Some(SuppressionComment {
            range: self.range,
            action,
            codes,
            reason,
        })
    }

    fn eat_action(&mut self) -> Option<SuppressionAction> {
        if !self.cursor.as_str().starts_with("ruff") {
            return None;
        }

        self.cursor.skip_bytes("ruff".len());
        self.eat_whitespace();

        if !self.cursor.eat_char(':') {
            return None;
        }
        self.eat_whitespace();

        if self.cursor.as_str().starts_with("disable") {
            self.cursor.skip_bytes("disable".len());
            Some(SuppressionAction::Disable)
        } else if self.cursor.as_str().starts_with("enable") {
            self.cursor.skip_bytes("enable".len());
            Some(SuppressionAction::Enable)
        } else if self.cursor.as_str().starts_with("ignore") {
            self.cursor.skip_bytes("ignore".len());
            Some(SuppressionAction::Ignore)
        } else {
            None
        }
    }

    fn eat_codes(&mut self) -> Option<SmallVec<[TextRange; 2]>> {
        self.eat_whitespace();
        if !self.cursor.eat_char('[') {
            return None;
        }

        let mut codes: SmallVec<[TextRange; 2]> = smallvec![];

        loop {
            if self.cursor.is_eof() {
                return None; // Missing closing bracket
            }

            self.eat_whitespace();

            if self.cursor.eat_char(']') {
                break Some(codes);
            }

            let code_start = self.offset();
            if !self.eat_word() {
                return None; // Invalid code
            }

            codes.push(TextRange::new(code_start, self.offset()));

            self.eat_whitespace();
            if !self.cursor.eat_char(',') {
                if self.cursor.eat_char(']') {
                    break Some(codes);
                }

                return None; // Missing comma
            }
        }
    }

    fn eat_whitespace(&mut self) -> bool {
        if self.cursor.eat_if(char::is_whitespace) {
            self.cursor.eat_while(char::is_whitespace);
            true
        } else {
            false
        }
    }

    fn eat_word(&mut self) -> bool {
        if self.cursor.eat_if(char::is_alphabetic) {
            // Allow `:` for better error recovery when someone uses `lint:code` instead of just `code`.
            self.cursor
                .eat_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':'));
            true
        } else {
            false
        }
    }

    fn offset(&self) -> TextSize {
        self.range.start() + self.range.len() - self.cursor.text_len()
    }
}

#[cfg(test)]
mod tests {
    use ruff_text_size::{TextRange, TextSize};
    use similar::DiffableStr;

    use crate::suppression::{SuppressionAction, SuppressionParser};

    #[test]
    fn test_parse_comment() {
        let source = "# ruff: ignore[foo, bar] hello world";
        let mut parser = SuppressionParser::new(
            source,
            TextRange::new(0.into(), TextSize::try_from(source.len()).unwrap()),
        );
        let comment = parser.parse_comment().unwrap();
        assert_eq!(comment.action, SuppressionAction::Ignore);
        assert_eq!(
            comment
                .codes
                .into_iter()
                .map(|range| { source.slice(range.into()) })
                .collect::<Vec<_>>(),
            ["foo", "bar"]
        );
        assert_eq!(source.slice(comment.reason.into()), "hello world");
    }
}
