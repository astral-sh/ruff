use std::borrow::Cow;

use rustpython_ast::Location;
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;

use crate::ast::types::Range;
use crate::SourceCodeLocator;

#[derive(Debug)]
pub struct Comment<'a> {
    pub value: Cow<'a, str>,
    pub location: Location,
    pub end_location: Location,
}

/// Collect all comments in an import block.
pub fn collect_comments<'a>(range: &Range, locator: &'a SourceCodeLocator) -> Vec<Comment<'a>> {
    let contents = locator.slice_source_code_range(range);
    lexer::make_tokenizer_located(&contents, range.location)
        .flatten()
        .filter_map(|(start, tok, end)| {
            if matches!(tok, Tok::Comment) {
                Some(Comment {
                    value: locator.slice_source_code_range(&Range {
                        location: start,
                        end_location: end,
                    }),
                    location: start,
                    end_location: end,
                })
            } else {
                None
            }
        })
        .collect()
}
