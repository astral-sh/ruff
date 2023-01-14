//! Struct used to index source code, to enable efficient lookup of tokens that
//! are omitted from the AST (e.g., commented lines).

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
        for (start, tok, ..) in lxr.iter().flatten() {
            if matches!(tok, Tok::Comment(_)) {
                commented_lines.push(start.row());
            } else if matches!(tok, Tok::NonLogicalNewline) {
                continuation_lines.push(start.row());
            }
        }
        Self {
            commented_lines,
            continuation_lines,
        }
    }
}
