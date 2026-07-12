use std::cmp::Ordering;

use ruff_python_ast::token::{Token, TokenFlags, TokenKind};
use ruff_text_size::{TextRange, TextSize};
use unicode_ident::{is_xid_continue, is_xid_start};

use crate::Mode;
use crate::lexer::Lexer;
use crate::lexer::classify::classify;
use crate::lexer::fast_token::{keyword, number, operator};
use crate::lexer::indentation::{Indentation, Indentations};

/// A pre-lexed token stream. Ruff exposes an `AoS` token collection after parsing, so emit that
/// representation once and let the parser consume it by index instead of copying an `SoA` stream.
#[derive(Debug)]
pub(crate) struct TwoPassTokens {
    pub(crate) tokens: Vec<Token>,
    pub(crate) rewrites: Vec<(usize, Token)>,
}

impl TwoPassTokens {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            tokens: Vec::with_capacity(capacity),
            rewrites: Vec::new(),
        }
    }

    #[inline]
    fn push(&mut self, kind: TokenKind, start: usize, end: usize, flags: TokenFlags) {
        self.tokens.push(Token::new(
            kind,
            TextRange::new(text_size(start), text_size(end)),
            flags,
        ));
    }

    #[inline]
    fn push_range(&mut self, kind: TokenKind, range: TextRange, flags: TokenFlags) {
        self.tokens.push(Token::new(kind, range, flags));
    }
}

/// Classify the source with NEON, carve strings and comments out of the structural stream, and
/// coalesce words, numbers, and operators into the complete token stream consumed by the parser.
///
/// Invalid or unusually contextual input returns `None` so the existing lexer keeps its exact
/// diagnostics and recovery behavior.
pub(crate) fn lex(source: &str) -> Option<TwoPassTokens> {
    let bytes = source.as_bytes();
    if u32::try_from(bytes.len()).is_err() {
        return None;
    }
    let classified = classify(bytes);
    let mut structural = Starts::new(&classified.starts);
    let mut tokens = TwoPassTokens::with_capacity(bytes.len() / 8);
    let mut indentations = Indentations::default();
    let mut indentation = Indentation::root();
    let mut line_start = 0;
    let mut at_logical_line_start = true;
    let mut logical_line_has_token = false;
    let mut nesting = 0_u32;

    while let Some(start) = structural.next() {
        let byte = bytes[start];

        if matches!(byte, b' ' | b'\t' | b'\x0c') {
            let end = structural.peek().unwrap_or(bytes.len());
            if at_logical_line_start && nesting == 0 {
                for byte in &bytes[start..end] {
                    indentation = match byte {
                        b' ' => indentation.add_space(),
                        b'\t' => indentation.add_tab(),
                        b'\x0c' => Indentation::root(),
                        _ => return None,
                    };
                }
            }
            continue;
        }

        match byte {
            b'\n' | b'\r' => {
                let end =
                    start + usize::from(byte == b'\r' && bytes.get(start + 1) == Some(&b'\n')) + 1;
                let kind = if nesting == 0 && logical_line_has_token {
                    at_logical_line_start = true;
                    logical_line_has_token = false;
                    indentation = Indentation::root();
                    line_start = end;
                    TokenKind::Newline
                } else {
                    if nesting == 0 {
                        at_logical_line_start = true;
                        indentation = Indentation::root();
                        line_start = end;
                    }
                    TokenKind::NonLogicalNewline
                };
                tokens.push(kind, start, end, TokenFlags::empty());
                structural.seek(end);
                continue;
            }
            b'#' => {
                let end = memchr::memchr2(b'\n', b'\r', &bytes[start..])
                    .map_or(bytes.len(), |index| start + index);
                tokens.push(TokenKind::Comment, start, end, TokenFlags::empty());
                structural.seek(end);
                continue;
            }
            b'\\' => {
                // Explicit continuations in indentation have slightly different semantics; leave
                // those rare cases, and a continuation at EOF, to the existing lexer.
                if at_logical_line_start {
                    return None;
                }
                let end = match bytes.get(start + 1..) {
                    Some([b'\n', ..]) => start + 2,
                    Some([b'\r', b'\n', ..]) => start + 3,
                    Some([b'\r', ..]) => start + 2,
                    _ => return None,
                };
                if end == bytes.len() {
                    return None;
                }
                structural.seek(end);
                continue;
            }
            _ => {}
        }

        if at_logical_line_start && nesting == 0 {
            match indentations.current().try_compare(indentation).ok()? {
                Ordering::Less => {
                    indentations.indent(indentation);
                    tokens.push(TokenKind::Indent, line_start, start, TokenFlags::empty());
                }
                Ordering::Greater => {
                    while indentations.current().try_compare(indentation).ok()? == Ordering::Greater
                    {
                        indentations.dedent_one(indentation).ok()?;
                        tokens.push(TokenKind::Dedent, start, start, TokenFlags::empty());
                    }
                }
                Ordering::Equal => {}
            }
            at_logical_line_start = false;
        }

        if byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii() {
            let end = structural.peek().unwrap_or(bytes.len());

            if matches!(bytes.get(end), Some(b'\'' | b'"'))
                && let Some(flags) = string_prefix(&bytes[start..end])
            {
                let end = if flags.intersects(TokenFlags::F_STRING | TokenFlags::T_STRING) {
                    append_interpolated_string(source, start, &mut tokens)?
                } else {
                    let (end, flags) = string(bytes, end, flags)?;
                    tokens.push(TokenKind::String, start, end, flags);
                    end
                };
                structural.seek(end);
                logical_line_has_token = true;
                continue;
            }

            if byte.is_ascii_digit() {
                let (kind, end) = number(bytes, start)?;
                tokens.push(kind, start, end, TokenFlags::empty());
                structural.seek(end);
                logical_line_has_token = true;
                continue;
            }

            let (kind, flags) = if classified.ascii_source {
                (keyword(&bytes[start..end]), TokenFlags::empty())
            } else {
                let text = &source[start..end];
                if text.is_ascii() {
                    (keyword(text.as_bytes()), TokenFlags::empty())
                } else if valid_identifier(text) {
                    (TokenKind::Name, TokenFlags::NON_ASCII_NAME)
                } else {
                    return None;
                }
            };
            tokens.push(kind, start, end, flags);
            logical_line_has_token = true;
            continue;
        }

        if matches!(byte, b'\'' | b'"') {
            let (end, flags) = string(bytes, start, TokenFlags::empty())?;
            tokens.push(TokenKind::String, start, end, flags);
            structural.seek(end);
            logical_line_has_token = true;
            continue;
        }

        if byte == b'.' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit) {
            let (kind, end) = number(bytes, start)?;
            tokens.push(kind, start, end, TokenFlags::empty());
            structural.seek(end);
            logical_line_has_token = true;
            continue;
        }

        // `?` is only a token in IPython mode.
        if byte == b'?' {
            return None;
        }
        let (kind, end) = operator(bytes, start)?;
        match kind {
            TokenKind::Lpar | TokenKind::Lsqb | TokenKind::Lbrace => nesting += 1,
            TokenKind::Rpar | TokenKind::Rsqb | TokenKind::Rbrace => {
                nesting = nesting.saturating_sub(1);
            }
            _ => {}
        }
        tokens.push(kind, start, end, TokenFlags::empty());
        structural.seek(end);
        logical_line_has_token = true;
    }

    if nesting != 0 {
        return None;
    }
    if logical_line_has_token {
        tokens.push(
            TokenKind::Newline,
            bytes.len(),
            bytes.len(),
            TokenFlags::empty(),
        );
    }
    while indentations.dedent().is_some() {
        tokens.push(
            TokenKind::Dedent,
            bytes.len(),
            bytes.len(),
            TokenFlags::empty(),
        );
    }
    tokens.push(
        TokenKind::EndOfFile,
        bytes.len(),
        bytes.len(),
        TokenFlags::empty(),
    );

    Some(tokens)
}

fn append_interpolated_string(
    source: &str,
    start: usize,
    tokens: &mut TwoPassTokens,
) -> Option<usize> {
    let mut lexer = Lexer::new(source, Mode::ParenthesizedExpression, text_size(start));
    let mut depth = 0_u32;

    loop {
        let kind = lexer.next_token();
        match kind {
            TokenKind::FStringStart | TokenKind::TStringStart => depth += 1,
            TokenKind::FStringEnd | TokenKind::TStringEnd => {
                depth = depth.checked_sub(1)?;
                tokens.push_range(kind, lexer.current_range(), lexer.current_flags());
                if depth == 0 {
                    let end = lexer.current_range().end().to_usize();
                    return lexer.finish().is_empty().then_some(end);
                }
                continue;
            }
            TokenKind::Unknown | TokenKind::EndOfFile => return None,
            _ => {}
        }
        tokens.push_range(kind, lexer.current_range(), lexer.current_flags());
    }
}

fn string_prefix(text: &[u8]) -> Option<TokenFlags> {
    let flags = match text {
        b"f" | b"F" => TokenFlags::F_STRING,
        b"t" | b"T" => TokenFlags::T_STRING,
        b"u" | b"U" => TokenFlags::UNICODE_STRING,
        b"b" | b"B" => TokenFlags::BYTE_STRING,
        b"r" => TokenFlags::RAW_STRING_LOWERCASE,
        b"R" => TokenFlags::RAW_STRING_UPPERCASE,
        [b'r', b'f' | b'F'] | [b'f' | b'F', b'r'] => {
            TokenFlags::F_STRING | TokenFlags::RAW_STRING_LOWERCASE
        }
        [b'R', b'f' | b'F'] | [b'f' | b'F', b'R'] => {
            TokenFlags::F_STRING | TokenFlags::RAW_STRING_UPPERCASE
        }
        [b'r', b't' | b'T'] | [b't' | b'T', b'r'] => {
            TokenFlags::T_STRING | TokenFlags::RAW_STRING_LOWERCASE
        }
        [b'R', b't' | b'T'] | [b't' | b'T', b'R'] => {
            TokenFlags::T_STRING | TokenFlags::RAW_STRING_UPPERCASE
        }
        [b'r', b'b' | b'B'] | [b'b' | b'B', b'r'] => {
            TokenFlags::BYTE_STRING | TokenFlags::RAW_STRING_LOWERCASE
        }
        [b'R', b'b' | b'B'] | [b'b' | b'B', b'R'] => {
            TokenFlags::BYTE_STRING | TokenFlags::RAW_STRING_UPPERCASE
        }
        _ => return None,
    };
    Some(flags)
}

fn string(source: &[u8], quote: usize, mut flags: TokenFlags) -> Option<(usize, TokenFlags)> {
    let quote_byte = *source.get(quote)?;
    if quote_byte == b'"' {
        flags |= TokenFlags::DOUBLE_QUOTES;
    }
    let triple = source.get(quote + 1..quote + 3) == Some(&[quote_byte, quote_byte]);
    if triple {
        flags |= TokenFlags::TRIPLE_QUOTED_STRING;
    }
    let mut offset = quote + if triple { 3 } else { 1 };

    loop {
        let relative = if triple {
            memchr::memchr(quote_byte, source.get(offset..)?)?
        } else {
            memchr::memchr3(quote_byte, b'\n', b'\r', source.get(offset..)?)?
        };
        let found = offset + relative;
        let escaped = source[..found]
            .iter()
            .rev()
            .take_while(|&&byte| byte == b'\\')
            .count()
            % 2
            == 1;
        if escaped {
            offset = found + 1 + usize::from(source.get(found..found + 2) == Some(b"\r\n"));
            continue;
        }
        if !triple && matches!(source[found], b'\n' | b'\r') {
            return None;
        }
        if !triple {
            return Some((found + 1, flags));
        }
        if source.get(found + 1..found + 3) == Some(&[quote_byte, quote_byte]) {
            return Some((found + 3, flags));
        }
        offset = found + 1;
    }
}

fn valid_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || is_xid_start(first)) && chars.all(is_xid_continue)
}

struct Starts<'a> {
    bitmap: &'a [u64],
    block: usize,
    pending: u64,
}

impl<'a> Starts<'a> {
    fn new(bitmap: &'a [u64]) -> Self {
        Self {
            bitmap,
            block: 0,
            pending: bitmap.first().copied().unwrap_or(0),
        }
    }

    #[inline]
    fn next(&mut self) -> Option<usize> {
        while self.pending == 0 {
            self.block += 1;
            self.pending = *self.bitmap.get(self.block)?;
        }
        let bit = self.pending.trailing_zeros() as usize;
        self.pending &= self.pending - 1;
        Some(self.block * 64 + bit)
    }

    #[inline]
    fn peek(&self) -> Option<usize> {
        if self.pending != 0 {
            return Some(self.block * 64 + self.pending.trailing_zeros() as usize);
        }
        self.bitmap[self.block + 1..]
            .iter()
            .position(|&word| word != 0)
            .map(|index| {
                let block = self.block + index + 1;
                block * 64 + self.bitmap[block].trailing_zeros() as usize
            })
    }

    #[inline]
    fn seek(&mut self, offset: usize) {
        self.block = offset / 64;
        self.pending =
            self.bitmap.get(self.block).copied().unwrap_or(0) & (u64::MAX << (offset % 64));
    }
}

// The public lexer rejects sources larger than `u32::MAX`, so every byte offset above is
// representable as a `TextSize`.
#[expect(clippy::cast_possible_truncation)]
fn text_size(offset: usize) -> TextSize {
    TextSize::new(offset as u32)
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::token::{Token, TokenKind};

    use crate::Mode;
    use crate::lexer::Lexer;
    use crate::lexer::two_pass::lex;

    fn legacy(source: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(source, Mode::Module, 0.into());
        let mut tokens = Vec::new();
        loop {
            let kind = lexer.next_token();
            tokens.push(Token::new(
                kind,
                lexer.current_range(),
                lexer.current_flags(),
            ));
            if kind == TokenKind::EndOfFile {
                break;
            }
        }
        assert!(lexer.finish().is_empty());
        tokens
    }

    fn assert_matches(source: &str) {
        let actual = lex(source).expect("two-pass lexer to accept the source");
        assert_eq!(
            format!("{:?}", actual.tokens),
            format!("{:?}", legacy(source))
        );
    }

    #[test]
    fn common_python() {
        assert_matches(
            "def calculate(first, second=0x_f):\n    values = [first, second, .5, 1_000e-2j]\n    return values[0] ** 2 // 3  # result\n",
        );
    }

    #[test]
    fn indentation_comments_and_eols() {
        assert_matches("# leading\r\nif True:\r\n\tif False:\r\n\t\tpass\r\n\r\npass\r\n");
        assert_matches("if True:\n    value = (\n        1,\n        2,\n    )\n    # trailing\n");
    }

    #[test]
    fn strings_and_interpolated_strings() {
        assert_matches("a = 'one'\nb = rb\"two\\\"three\"\nc = '''three\nlines'''\n");
        assert_matches("value = f\"{name!r}: {items[0]:04d}\"\nother = rf'{path}\\suffix'\n");
    }

    #[test]
    fn unicode_identifiers_and_strings() {
        assert_matches("变量 = 'λ'\nprint(变量)\n");
    }

    #[test]
    fn invalid_or_contextual_input_falls_back() {
        for source in [
            "value = 01\n",
            "value = 'unterminated\n",
            "(value\n",
            "?value\n",
            "value = 1\\\n",
            "value = 1\\\r",
            "value = 1\\\r\n",
        ] {
            assert!(lex(source).is_none(), "{source:?}");
        }
    }
}
