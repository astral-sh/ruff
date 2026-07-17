use std::cmp::Ordering;

use ruff_python_ast::token::{Token, TokenFlags, TokenKind};
use ruff_text_size::{TextRange, TextSize};
use unicode_ident::{is_xid_continue, is_xid_start};

use crate::Mode;
use crate::lexer::Lexer;
use crate::lexer::classify::{Classified, classify_into};
use crate::lexer::fast_token::{keyword, number, operator};
use crate::lexer::indentation::{Indentation, Indentations};

/// Tokens produced by the chunked lexer. Parser results store tokens as a `Vec<Token>`, so the
/// lexer builds that representation directly and the parser consumes it by index.
#[derive(Debug)]
pub(crate) struct ChunkedTokens {
    pub(crate) tokens: Vec<Token>,
    pub(crate) rewrites: Vec<(usize, Token)>,
}

impl ChunkedTokens {
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

const CHUNK_SIZE: usize = 32 * 1024;

/// An on-demand lexer that classifies fixed-size source batches with SIMD and buffers complete
/// tokens for the parser. Unsupported or malformed input returns `None` so parsing can restart
/// with the streaming lexer.
#[derive(Debug)]
pub(crate) struct ChunkedLexer<'src> {
    pub(crate) tokens: ChunkedTokens,
    source: &'src str,
    offset: usize,
    classified: Classified,
    indentations: Indentations,
    indentation: Indentation,
    line_start: usize,
    at_logical_line_start: bool,
    logical_line_has_token: bool,
    nesting: u32,
    finished: bool,
}

impl<'src> ChunkedLexer<'src> {
    /// Creates a chunked lexer for a source whose byte offsets fit in [`TextSize`].
    pub(crate) fn new(source: &'src str) -> Option<Self> {
        u32::try_from(source.len()).ok()?;
        Some(Self {
            tokens: ChunkedTokens {
                tokens: Vec::with_capacity(source.len() / 8),
                rewrites: Vec::new(),
            },
            source,
            offset: 0,
            classified: Classified::default(),
            indentations: Indentations::default(),
            indentation: Indentation::root(),
            line_start: 0,
            at_logical_line_start: true,
            logical_line_has_token: false,
            nesting: 0,
            finished: false,
        })
    }

    pub(crate) const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Classify a batch with SIMD, then carve and coalesce it into complete tokens. A final word
    /// or whitespace run is extended so that the next batch always starts at a token boundary.
    pub(crate) fn fill(&mut self) -> Option<()> {
        if self.finished {
            return Some(());
        }

        let source = self.source;
        let bytes = source.as_bytes();
        let tokens = &mut self.tokens;
        let chunk_start = self.offset;
        let mut chunk_end = (chunk_start + CHUNK_SIZE).min(bytes.len());
        if chunk_end < bytes.len() {
            if is_word(bytes[chunk_end - 1]) {
                while chunk_end < bytes.len() && is_word(bytes[chunk_end]) {
                    chunk_end += 1;
                }
            } else if is_whitespace(bytes[chunk_end - 1]) {
                while chunk_end < bytes.len() && is_whitespace(bytes[chunk_end]) {
                    chunk_end += 1;
                }
            }
        }

        classify_into(&bytes[chunk_start..chunk_end], &mut self.classified);
        let mut structural = Starts::new(&self.classified.starts, chunk_start);
        let mut next_offset = chunk_end;
        let mut indentation = self.indentation;
        let mut line_start = self.line_start;
        let mut at_logical_line_start = self.at_logical_line_start;
        let mut logical_line_has_token = self.logical_line_has_token;
        let mut nesting = self.nesting;

        while let Some(start) = structural.next() {
            let byte = bytes[start];

            if matches!(byte, b' ' | b'\t' | b'\x0c') {
                let end = structural.peek().unwrap_or(chunk_end);
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
                    let end = start
                        + usize::from(byte == b'\r' && bytes.get(start + 1) == Some(&b'\n'))
                        + 1;
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
                    if end > start + 1 {
                        structural.seek(end);
                        next_offset = next_offset.max(end);
                    }
                    continue;
                }
                b'#' => {
                    let end = memchr::memchr2(b'\n', b'\r', &bytes[start..])
                        .map_or(bytes.len(), |index| start + index);
                    tokens.push(TokenKind::Comment, start, end, TokenFlags::empty());
                    structural.seek(end);
                    next_offset = next_offset.max(end);
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
                    next_offset = next_offset.max(end);
                    continue;
                }
                _ => {}
            }

            if at_logical_line_start && nesting == 0 {
                match self.indentations.current().try_compare(indentation).ok()? {
                    Ordering::Less => {
                        self.indentations.indent(indentation);
                        tokens.push(TokenKind::Indent, line_start, start, TokenFlags::empty());
                    }
                    Ordering::Greater => {
                        while self.indentations.current().try_compare(indentation).ok()?
                            == Ordering::Greater
                        {
                            self.indentations.dedent_one(indentation).ok()?;
                            tokens.push(TokenKind::Dedent, start, start, TokenFlags::empty());
                        }
                    }
                    Ordering::Equal => {}
                }
                at_logical_line_start = false;
            }

            if byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii() {
                let end = structural.peek().unwrap_or(chunk_end);

                if matches!(bytes.get(end), Some(b'\'' | b'"'))
                    && let Some(flags) = string_prefix(&bytes[start..end])
                {
                    let end = if flags.intersects(TokenFlags::F_STRING | TokenFlags::T_STRING) {
                        append_interpolated_string(source, start, tokens)?
                    } else {
                        let (end, flags) = string(bytes, end, flags)?;
                        tokens.push(TokenKind::String, start, end, flags);
                        end
                    };
                    structural.seek(end);
                    next_offset = next_offset.max(end);
                    logical_line_has_token = true;
                    continue;
                }

                if byte.is_ascii_digit() {
                    let (kind, end) = number(bytes, start)?;
                    tokens.push(kind, start, end, TokenFlags::empty());
                    structural.seek(end);
                    next_offset = next_offset.max(end);
                    logical_line_has_token = true;
                    continue;
                }

                let (kind, flags) = if self.classified.ascii_source {
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
                next_offset = next_offset.max(end);
                logical_line_has_token = true;
                continue;
            }

            if byte == b'.' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit) {
                let (kind, end) = number(bytes, start)?;
                tokens.push(kind, start, end, TokenFlags::empty());
                structural.seek(end);
                next_offset = next_offset.max(end);
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
            if end > start + 1 {
                structural.seek(end);
                next_offset = next_offset.max(end);
            }
            logical_line_has_token = true;
        }

        self.offset = next_offset;
        self.indentation = indentation;
        self.line_start = line_start;
        self.at_logical_line_start = at_logical_line_start;
        self.logical_line_has_token = logical_line_has_token;
        self.nesting = nesting;

        if next_offset == bytes.len() {
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
            while self.indentations.dedent().is_some() {
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
            self.finished = true;
        }

        Some(())
    }
}

/// Drain the chunked lexer for standalone lexer benchmarks and differential tests.
pub(crate) fn lex(source: &str) -> Option<ChunkedTokens> {
    let mut lexer = ChunkedLexer::new(source)?;
    while !lexer.is_finished() {
        lexer.fill()?;
    }
    Some(lexer.tokens)
}

const fn is_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || !byte.is_ascii()
}

const fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\x0c')
}

/// Appends one complete f-string or t-string, including nested interpolation tokens, using the
/// streaming lexer to preserve its context-sensitive tokenization.
///
/// ```python
/// value = f"{item!r:{width}}"
/// ```
fn append_interpolated_string(
    source: &str,
    start: usize,
    tokens: &mut ChunkedTokens,
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

/// Converts a valid Python string prefix into token flags, preserving raw-prefix casing.
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

/// Scans a non-interpolated string from its opening quote and returns its end and quote flags.
/// Unterminated strings and single-quoted strings containing a newline fall back to the streaming
/// lexer for diagnostics.
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

/// Iterates structural-token starts from the classifier bitmap and can skip the interior of a
/// token that was coalesced by the scalar pass.
struct Starts<'a> {
    bitmap: &'a [u64],
    block: usize,
    pending: u64,
    base: usize,
}

impl<'a> Starts<'a> {
    fn new(bitmap: &'a [u64], base: usize) -> Self {
        Self {
            bitmap,
            block: 0,
            pending: bitmap.first().copied().unwrap_or(0),
            base,
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
        Some(self.base + self.block * 64 + bit)
    }

    #[inline]
    fn peek(&self) -> Option<usize> {
        if self.pending != 0 {
            return Some(self.base + self.block * 64 + self.pending.trailing_zeros() as usize);
        }
        self.bitmap[self.block + 1..]
            .iter()
            .position(|&word| word != 0)
            .map(|index| {
                let block = self.block + index + 1;
                self.base + block * 64 + self.bitmap[block].trailing_zeros() as usize
            })
    }

    /// Discards every pending structural start before `offset`.
    #[inline]
    fn seek(&mut self, offset: usize) {
        let offset = offset - self.base;
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
    use crate::lexer::chunked::{CHUNK_SIZE, lex};

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
        let actual = lex(source).expect("chunked lexer to accept the source");
        assert_eq!(actual.tokens, legacy(source));
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
    fn chunk_boundaries() {
        let prefix = "x=0;".repeat(CHUNK_SIZE / 4 - 1);
        for suffix in [
            "long_identifier_crossing_the_boundary = 1\n",
            "    spaced = 1\n",
            "# comment crossing the boundary\nvalue = 1\n",
            "value = 'a string crossing the boundary'\n",
            "value = f'{name}: interpolation crossing the boundary'\n",
            "变量跨越边界 = 1\n",
            "x=0\r\nvalue = 1\r\n",
        ] {
            assert_matches(&format!("{prefix}{suffix}"));
        }

        let prefix = "x=0\n".repeat(CHUNK_SIZE / 4 - 2);
        assert_matches(&format!(
            "{prefix}if True:\n    values = (\n        1,\n        2,\n    )\n"
        ));
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
