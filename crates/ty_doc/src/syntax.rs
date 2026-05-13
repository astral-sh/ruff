use ruff_python_ast::token::{Token, TokenKind, Tokens};
use ruff_python_parser::{Mode, ParseOptions, parse_unchecked};
use ruff_text_size::Ranged;

pub(crate) fn parse_python_tokens(source: &str) -> Tokens {
    parse_unchecked(source, ParseOptions::from(Mode::Module))
        .tokens()
        .clone()
}

pub(crate) fn dotted_name_run_end(tokens: &[Token], start: usize) -> Option<usize> {
    let mut end = start + 1;
    while let (Some(previous), Some(dot), Some(name)) =
        (tokens.get(end - 1), tokens.get(end), tokens.get(end + 1))
    {
        if dot.kind() != TokenKind::Dot
            || name.kind() != TokenKind::Name
            || previous.end() != dot.start()
            || dot.end() != name.start()
        {
            break;
        }
        end += 2;
    }
    (end > start + 1).then_some(end)
}
