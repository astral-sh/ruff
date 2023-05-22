use std::borrow::Cow;

use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::{lexer, Mode, Tok};

use ruff_python_ast::source_code::Locator;

#[derive(Debug)]
pub(crate) struct Comment<'a> {
    pub(crate) value: Cow<'a, str>,
    pub(crate) range: TextRange,
}

impl Comment<'_> {
    pub(crate) const fn start(&self) -> TextSize {
        self.range.start()
    }

    pub(crate) const fn end(&self) -> TextSize {
        self.range.end()
    }
}

/// Collect all comments in an import block.
pub(crate) fn collect_comments<'a>(range: TextRange, locator: &'a Locator) -> Vec<Comment<'a>> {
    let contents = locator.slice(range);
    lexer::lex_starts_at(contents, Mode::Module, range.start())
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
