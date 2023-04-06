//! Struct used to index source code, to enable efficient lookup of tokens that
//! are omitted from the AST (e.g., commented lines).

use rustpython_parser::ast::Location;
use rustpython_parser::lexer::LexResult;
use rustpython_parser::Tok;

use crate::types::Range;

pub struct Indexer {
    commented_lines: Vec<usize>,
    continuation_lines: Vec<usize>,
    string_ranges: Vec<Range>,
}

impl Indexer {
    /// Return a slice of all lines that include a comment.
    pub fn commented_lines(&self) -> &[usize] {
        &self.commented_lines
    }

    /// Return a slice of all lines that end with a continuation (backslash).
    pub fn continuation_lines(&self) -> &[usize] {
        &self.continuation_lines
    }

    /// Return a slice of all ranges that include a triple-quoted string.
    pub fn string_ranges(&self) -> &[Range] {
        &self.string_ranges
    }
}

impl From<&[LexResult]> for Indexer {
    fn from(lxr: &[LexResult]) -> Self {
        let mut commented_lines = Vec::new();
        let mut continuation_lines = Vec::new();
        let mut string_ranges = Vec::new();
        let mut prev: Option<(&Location, &Tok, &Location)> = None;
        for (start, tok, end) in lxr.iter().flatten() {
            match tok {
                Tok::Comment(..) => commented_lines.push(start.row()),
                Tok::String {
                    triple_quoted: true,
                    ..
                } => string_ranges.push(Range::new(*start, *end)),
                _ => (),
            }

            if let Some((.., prev_tok, prev_end)) = prev {
                if !matches!(
                    prev_tok,
                    Tok::Newline | Tok::NonLogicalNewline | Tok::Comment(..)
                ) {
                    for line in prev_end.row()..start.row() {
                        continuation_lines.push(line);
                    }
                }
            }
            prev = Some((start, tok, end));
        }
        Self {
            commented_lines,
            continuation_lines,
            string_ranges,
        }
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser::ast::Location;
    use rustpython_parser::lexer::LexResult;
    use rustpython_parser::{lexer, Mode};

    use crate::source_code::Indexer;
    use crate::types::Range;

    #[test]
    fn continuation() {
        let contents = r#"x = 1"#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(indexer.continuation_lines(), Vec::<usize>::new().as_slice());

        let contents = r#"
# Hello, world!

x = 1

y = 2
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(indexer.continuation_lines(), Vec::<usize>::new().as_slice());

        let contents = r#"
x = \
    1

if True:
    z = \
        \
        2

(
    "abc" # Foo
    "def" \
    "ghi"
)
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(indexer.continuation_lines(), [1, 5, 6, 11]);

        let contents = r#"
x = 1; import sys
import os

if True:
    x = 1; import sys
    import os

if True:
    x = 1; \
        import os

x = 1; \
import os
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(indexer.continuation_lines(), [9, 12]);
    }

    #[test]
    fn string_ranges() {
        let contents = r#""this is a single-quoted string""#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(indexer.string_ranges(), &vec![]);

        let contents = r#"
            """
            this is a multiline string
            """
            "#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(
            indexer.string_ranges(),
            &vec![Range::new(Location::new(2, 12), Location::new(4, 15))]
        );

        let contents = r#"
            """
            '''this is a multiline string with multiple delimiter types'''
            """
            "#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(
            indexer.string_ranges(),
            &vec![Range::new(Location::new(2, 12), Location::new(4, 15))]
        );

        let contents = r#"
            """
            this is one
            multiline string
            """
            """
            and this is
            another
            """
            "#;
        let lxr: Vec<LexResult> = lexer::lex(contents, Mode::Module).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(
            indexer.string_ranges(),
            &vec![
                Range::new(Location::new(2, 12), Location::new(5, 15)),
                Range::new(Location::new(6, 12), Location::new(9, 15))
            ]
        );
    }
}
