use ruff_text_size::{TextRange, TextSize};
use std::borrow::Cow;

use rustpython_parser::{lexer, Mode, Tok};

use ruff_python_ast::source_code::Locator;

#[derive(Debug)]
pub struct Comment<'a> {
    pub value: Cow<'a, str>,
    pub range: TextRange,
}

impl Comment<'_> {
    pub const fn start(&self) -> TextSize {
        self.range.start()
    }

    pub const fn end(&self) -> TextSize {
        self.range.end()
    }

    pub const fn range(&self) -> TextRange {
        self.range
    }
}

/// Collect all comments in an import block.
pub fn collect_comments<'a>(range: TextRange, locator: &'a Locator) -> Vec<Comment<'a>> {
    let contents = locator.slice(range);
    lexer::lex_located(contents, Mode::Module, range.start())
        .flatten()
        .filter_map(|(tok, range)| {
            if let Tok::Comment(value) = tok {
                Some(Comment {
                    value: value.into(),
                    range,
                })
            } else {
                None
            }
        })
        .collect()
}
