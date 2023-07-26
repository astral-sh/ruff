use ruff_source_file::Locator;
use ruff_text_size::{TextRange, TextSize};
use rustpython_parser::{lexer, Mode, Tok};

/// Return the range of the first parenthesis pair after a given [`TextSize`].
pub fn match_parens(start: TextSize, locator: &Locator) -> Option<TextRange> {
    let contents = &locator.contents()[usize::from(start)..];

    let mut fix_start = None;
    let mut fix_end = None;
    let mut count = 0u32;

    for (tok, range) in lexer::lex_starts_at(contents, Mode::Module, start).flatten() {
        match tok {
            Tok::Lpar => {
                if count == 0 {
                    fix_start = Some(range.start());
                }
                count = count.saturating_add(1);
            }
            Tok::Rpar => {
                count = count.saturating_sub(1);
                if count == 0 {
                    fix_end = Some(range.end());
                    break;
                }
            }
            _ => {}
        }
    }

    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Some(TextRange::new(start, end)),
        _ => None,
    }
}
