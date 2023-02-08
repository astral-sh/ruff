//! Struct used to index source code, to enable efficient lookup of tokens that
//! are omitted from the AST (e.g., commented lines).

use rustpython_parser::ast::Location;
use rustpython_parser::lexer::{LexResult, Tok};

pub struct Indexer {
    commented_lines: Vec<usize>,
    continuation_lines: Vec<usize>,
}

impl Indexer {
    pub fn commented_lines(&self) -> &[usize] {
        &self.commented_lines
    }

    pub fn continuation_lines(&self) -> &[usize] {
        &self.continuation_lines
    }
}

impl From<&[LexResult]> for Indexer {
    fn from(lxr: &[LexResult]) -> Self {
        let mut commented_lines = Vec::new();
        let mut continuation_lines = Vec::new();
        let mut prev: Option<(&Location, &Tok, &Location)> = None;
        for (start, tok, end) in lxr.iter().flatten() {
            if matches!(tok, Tok::Comment(_)) {
                commented_lines.push(start.row());
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
        }
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser::lexer;
    use rustpython_parser::lexer::LexResult;

    use crate::source_code::Indexer;

    #[test]
    fn continuation() {
        let contents = r#"x = 1"#;
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(indexer.continuation_lines(), Vec::<usize>::new().as_slice());

        let contents = r#"
# Hello, world!

x = 1

y = 2
"#
        .trim();
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
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
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
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
        let lxr: Vec<LexResult> = lexer::make_tokenizer(contents).collect();
        let indexer: Indexer = lxr.as_slice().into();
        assert_eq!(indexer.continuation_lines(), [9, 12]);
    }
}
