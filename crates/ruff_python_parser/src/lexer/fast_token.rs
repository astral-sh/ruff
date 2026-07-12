use ruff_python_ast::token::TokenKind;

/// Classifies an already-delimited identifier as a keyword or name.
#[inline]
pub(super) fn keyword(text: &[u8]) -> TokenKind {
    match text {
        b"False" => TokenKind::False,
        b"None" => TokenKind::None,
        b"True" => TokenKind::True,
        b"and" => TokenKind::And,
        b"as" => TokenKind::As,
        b"assert" => TokenKind::Assert,
        b"async" => TokenKind::Async,
        b"await" => TokenKind::Await,
        b"break" => TokenKind::Break,
        b"case" => TokenKind::Case,
        b"class" => TokenKind::Class,
        b"continue" => TokenKind::Continue,
        b"def" => TokenKind::Def,
        b"del" => TokenKind::Del,
        b"elif" => TokenKind::Elif,
        b"else" => TokenKind::Else,
        b"except" => TokenKind::Except,
        b"finally" => TokenKind::Finally,
        b"for" => TokenKind::For,
        b"from" => TokenKind::From,
        b"global" => TokenKind::Global,
        b"if" => TokenKind::If,
        b"import" => TokenKind::Import,
        b"in" => TokenKind::In,
        b"is" => TokenKind::Is,
        b"lambda" => TokenKind::Lambda,
        b"lazy" => TokenKind::Lazy,
        b"match" => TokenKind::Match,
        b"nonlocal" => TokenKind::Nonlocal,
        b"not" => TokenKind::Not,
        b"or" => TokenKind::Or,
        b"pass" => TokenKind::Pass,
        b"raise" => TokenKind::Raise,
        b"return" => TokenKind::Return,
        b"try" => TokenKind::Try,
        b"type" => TokenKind::Type,
        b"while" => TokenKind::While,
        b"with" => TokenKind::With,
        b"yield" => TokenKind::Yield,
        _ => TokenKind::Name,
    }
}

/// Matches the longest Python operator or delimiter beginning at `start`.
#[inline]
pub(super) fn operator(source: &[u8], start: usize) -> Option<(TokenKind, usize)> {
    let source = source.get(start..)?;
    let (kind, len) = match source {
        [b'.', b'.', b'.', ..] => (TokenKind::Ellipsis, 3),
        [b'*', b'*', b'=', ..] => (TokenKind::DoubleStarEqual, 3),
        [b'/', b'/', b'=', ..] => (TokenKind::DoubleSlashEqual, 3),
        [b'<', b'<', b'=', ..] => (TokenKind::LeftShiftEqual, 3),
        [b'>', b'>', b'=', ..] => (TokenKind::RightShiftEqual, 3),
        [b'=', b'=', ..] => (TokenKind::EqEqual, 2),
        [b'!', b'=', ..] => (TokenKind::NotEqual, 2),
        [b'<', b'=', ..] => (TokenKind::LessEqual, 2),
        [b'>', b'=', ..] => (TokenKind::GreaterEqual, 2),
        [b'<', b'<', ..] => (TokenKind::LeftShift, 2),
        [b'>', b'>', ..] => (TokenKind::RightShift, 2),
        [b'*', b'*', ..] => (TokenKind::DoubleStar, 2),
        [b'/', b'/', ..] => (TokenKind::DoubleSlash, 2),
        [b'+', b'=', ..] => (TokenKind::PlusEqual, 2),
        [b'-', b'=', ..] => (TokenKind::MinusEqual, 2),
        [b'*', b'=', ..] => (TokenKind::StarEqual, 2),
        [b'/', b'=', ..] => (TokenKind::SlashEqual, 2),
        [b'%', b'=', ..] => (TokenKind::PercentEqual, 2),
        [b'&', b'=', ..] => (TokenKind::AmperEqual, 2),
        [b'|', b'=', ..] => (TokenKind::VbarEqual, 2),
        [b'^', b'=', ..] => (TokenKind::CircumflexEqual, 2),
        [b'@', b'=', ..] => (TokenKind::AtEqual, 2),
        [b':', b'=', ..] => (TokenKind::ColonEqual, 2),
        [b'-', b'>', ..] => (TokenKind::Rarrow, 2),
        [b'?', ..] => (TokenKind::Question, 1),
        [b'!', ..] => (TokenKind::Exclamation, 1),
        [b'(', ..] => (TokenKind::Lpar, 1),
        [b')', ..] => (TokenKind::Rpar, 1),
        [b'[', ..] => (TokenKind::Lsqb, 1),
        [b']', ..] => (TokenKind::Rsqb, 1),
        [b'{', ..] => (TokenKind::Lbrace, 1),
        [b'}', ..] => (TokenKind::Rbrace, 1),
        [b':', ..] => (TokenKind::Colon, 1),
        [b',', ..] => (TokenKind::Comma, 1),
        [b';', ..] => (TokenKind::Semi, 1),
        [b'+', ..] => (TokenKind::Plus, 1),
        [b'-', ..] => (TokenKind::Minus, 1),
        [b'*', ..] => (TokenKind::Star, 1),
        [b'/', ..] => (TokenKind::Slash, 1),
        [b'|', ..] => (TokenKind::Vbar, 1),
        [b'&', ..] => (TokenKind::Amper, 1),
        [b'<', ..] => (TokenKind::Less, 1),
        [b'>', ..] => (TokenKind::Greater, 1),
        [b'=', ..] => (TokenKind::Equal, 1),
        [b'.', ..] => (TokenKind::Dot, 1),
        [b'%', ..] => (TokenKind::Percent, 1),
        [b'~', ..] => (TokenKind::Tilde, 1),
        [b'^', ..] => (TokenKind::CircumFlex, 1),
        [b'@', ..] => (TokenKind::At, 1),
        _ => return None,
    };

    Some((kind, start + len))
}

/// Scans a Python numeric literal and returns its kind and end. Malformed or ambiguous literals
/// return `None` so the streaming lexer can produce the canonical diagnostic.
///
/// ```python
/// value = 0x_f + 1_000e-2j + .5
/// ```
#[inline]
pub(super) fn number(source: &[u8], start: usize) -> Option<(TokenKind, usize)> {
    let first = *source.get(start)?;
    if first == b'.' && !source.get(start + 1).is_some_and(u8::is_ascii_digit) {
        return None;
    }
    if first != b'.' && !first.is_ascii_digit() {
        return None;
    }

    if first == b'0'
        && let Some(&prefix) = source.get(start + 1)
        && matches!(prefix, b'x' | b'X' | b'o' | b'O' | b'b' | b'B')
    {
        let radix = match prefix {
            b'x' | b'X' => 16,
            b'o' | b'O' => 8,
            b'b' | b'B' => 2,
            _ => return None,
        };
        let mut end = start + 2;
        if source.get(end) == Some(&b'_') {
            end += 1;
            if !source.get(end).is_some_and(|&byte| is_digit(byte, radix)) {
                return None;
            }
        }
        let (end, has_digit, _) = digit_run(source, end, radix);
        if !has_digit || is_number_continuation(source.get(end).copied()) {
            return None;
        }
        return Some((TokenKind::Int, end));
    }

    let (mut end, _, has_nonzero_digit) = if first == b'.' {
        (start, false, false)
    } else {
        digit_run(source, start, 10)
    };
    let mut is_float = first == b'.';

    if source.get(end) == Some(&b'.') {
        is_float = true;
        end += 1;
        if source.get(end) == Some(&b'_') {
            return None;
        }
        (end, _, _) = digit_run(source, end, 10);
    }

    if matches!(source.get(end), Some(b'e' | b'E')) {
        is_float = true;
        end += 1;
        if matches!(source.get(end), Some(b'+' | b'-')) {
            end += 1;
        }
        if !source.get(end).is_some_and(u8::is_ascii_digit) {
            return None;
        }
        let (exponent_end, has_digit, _) = digit_run(source, end, 10);
        if !has_digit {
            return None;
        }
        end = exponent_end;
    }

    let is_complex = matches!(source.get(end), Some(b'j' | b'J'));
    if is_complex {
        end += 1;
    }
    if is_number_continuation(source.get(end).copied()) {
        return None;
    }
    if !is_float && !is_complex && first == b'0' && has_nonzero_digit {
        return None;
    }

    let kind = if is_complex {
        TokenKind::Complex
    } else if is_float {
        TokenKind::Float
    } else {
        TokenKind::Int
    };
    Some((kind, end))
}

#[inline]
fn digit_run(source: &[u8], mut position: usize, radix: u8) -> (usize, bool, bool) {
    let mut has_digit = false;
    let mut has_nonzero_digit = false;
    while let Some(&byte) = source.get(position) {
        if is_digit(byte, radix) {
            has_digit = true;
            has_nonzero_digit |= byte != b'0';
            position += 1;
        } else if byte == b'_'
            && source
                .get(position + 1)
                .is_some_and(|&next| is_digit(next, radix))
        {
            position += 1;
        } else {
            break;
        }
    }
    (position, has_digit, has_nonzero_digit)
}

#[inline]
const fn is_digit(byte: u8, radix: u8) -> bool {
    match radix {
        2 => matches!(byte, b'0'..=b'1'),
        8 => matches!(byte, b'0'..=b'7'),
        10 => byte.is_ascii_digit(),
        16 => byte.is_ascii_hexdigit(),
        _ => false,
    }
}

#[inline]
const fn is_number_continuation(byte: Option<u8>) -> bool {
    matches!(
        byte,
        Some(b'_' | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | 0x80..=0xff)
    )
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::token::TokenKind;

    use super::{keyword, number, operator};

    #[test]
    fn keywords() {
        for (text, kind) in [
            (b"False".as_slice(), TokenKind::False),
            (b"None", TokenKind::None),
            (b"True", TokenKind::True),
            (b"and", TokenKind::And),
            (b"as", TokenKind::As),
            (b"assert", TokenKind::Assert),
            (b"async", TokenKind::Async),
            (b"await", TokenKind::Await),
            (b"break", TokenKind::Break),
            (b"case", TokenKind::Case),
            (b"class", TokenKind::Class),
            (b"continue", TokenKind::Continue),
            (b"def", TokenKind::Def),
            (b"del", TokenKind::Del),
            (b"elif", TokenKind::Elif),
            (b"else", TokenKind::Else),
            (b"except", TokenKind::Except),
            (b"finally", TokenKind::Finally),
            (b"for", TokenKind::For),
            (b"from", TokenKind::From),
            (b"global", TokenKind::Global),
            (b"if", TokenKind::If),
            (b"import", TokenKind::Import),
            (b"in", TokenKind::In),
            (b"is", TokenKind::Is),
            (b"lambda", TokenKind::Lambda),
            (b"lazy", TokenKind::Lazy),
            (b"match", TokenKind::Match),
            (b"nonlocal", TokenKind::Nonlocal),
            (b"not", TokenKind::Not),
            (b"or", TokenKind::Or),
            (b"pass", TokenKind::Pass),
            (b"raise", TokenKind::Raise),
            (b"return", TokenKind::Return),
            (b"try", TokenKind::Try),
            (b"type", TokenKind::Type),
            (b"while", TokenKind::While),
            (b"with", TokenKind::With),
            (b"yield", TokenKind::Yield),
        ] {
            assert_eq!(keyword(text), kind, "keyword {text:?}");
        }
        for text in [b"false".as_slice(), b"TRUE", b"match_", b"identifier", b""] {
            assert_eq!(keyword(text), TokenKind::Name, "name {text:?}");
        }
    }

    #[test]
    fn operators() {
        for (text, kind) in [
            ("...", TokenKind::Ellipsis),
            ("**=", TokenKind::DoubleStarEqual),
            ("//=", TokenKind::DoubleSlashEqual),
            ("<<=", TokenKind::LeftShiftEqual),
            (">>=", TokenKind::RightShiftEqual),
            ("==", TokenKind::EqEqual),
            ("!=", TokenKind::NotEqual),
            ("<=", TokenKind::LessEqual),
            (">=", TokenKind::GreaterEqual),
            ("<<", TokenKind::LeftShift),
            (">>", TokenKind::RightShift),
            ("**", TokenKind::DoubleStar),
            ("//", TokenKind::DoubleSlash),
            ("+=", TokenKind::PlusEqual),
            ("-=", TokenKind::MinusEqual),
            ("*=", TokenKind::StarEqual),
            ("/=", TokenKind::SlashEqual),
            ("%=", TokenKind::PercentEqual),
            ("&=", TokenKind::AmperEqual),
            ("|=", TokenKind::VbarEqual),
            ("^=", TokenKind::CircumflexEqual),
            ("@=", TokenKind::AtEqual),
            (":=", TokenKind::ColonEqual),
            ("->", TokenKind::Rarrow),
            ("?", TokenKind::Question),
            ("!", TokenKind::Exclamation),
            ("(", TokenKind::Lpar),
            (")", TokenKind::Rpar),
            ("[", TokenKind::Lsqb),
            ("]", TokenKind::Rsqb),
            ("{", TokenKind::Lbrace),
            ("}", TokenKind::Rbrace),
            (":", TokenKind::Colon),
            (",", TokenKind::Comma),
            (";", TokenKind::Semi),
            ("+", TokenKind::Plus),
            ("-", TokenKind::Minus),
            ("*", TokenKind::Star),
            ("/", TokenKind::Slash),
            ("|", TokenKind::Vbar),
            ("&", TokenKind::Amper),
            ("<", TokenKind::Less),
            (">", TokenKind::Greater),
            ("=", TokenKind::Equal),
            (".", TokenKind::Dot),
            ("%", TokenKind::Percent),
            ("~", TokenKind::Tilde),
            ("^", TokenKind::CircumFlex),
            ("@", TokenKind::At),
        ] {
            let source = format!("x{text}y");
            assert_eq!(
                operator(source.as_bytes(), 1),
                Some((kind, 1 + text.len())),
                "operator {text:?}"
            );
        }
        assert_eq!(operator(b"..", 0), Some((TokenKind::Dot, 1)));
        assert_eq!(operator(b"***", 0), Some((TokenKind::DoubleStar, 2)));
        assert_eq!(operator(b"///", 0), Some((TokenKind::DoubleSlash, 2)));
        assert_eq!(operator(b"x", 0), None);
        assert_eq!(operator(b"", 0), None);
    }

    #[test]
    fn valid_numbers() {
        for (text, kind) in [
            ("0", TokenKind::Int),
            ("00_0", TokenKind::Int),
            ("1_234", TokenKind::Int),
            ("0b1_0", TokenKind::Int),
            ("0B_1", TokenKind::Int),
            ("0o7_0", TokenKind::Int),
            ("0O_7", TokenKind::Int),
            ("0xF_f", TokenKind::Int),
            ("0X_a", TokenKind::Int),
            ("1.", TokenKind::Float),
            (".1", TokenKind::Float),
            ("1_2.3_4", TokenKind::Float),
            ("00_1.0", TokenKind::Float),
            ("1e2", TokenKind::Float),
            ("1E+2", TokenKind::Float),
            ("1_2e-3_4", TokenKind::Float),
            ("1j", TokenKind::Complex),
            ("01J", TokenKind::Complex),
            (".1j", TokenKind::Complex),
            ("1.e2J", TokenKind::Complex),
        ] {
            let source = format!("x{text}+");
            assert_eq!(
                number(source.as_bytes(), 1),
                Some((kind, 1 + text.len())),
                "number {text:?}"
            );
        }
    }

    #[test]
    fn invalid_or_ambiguous_numbers_fall_back() {
        for text in [
            "", ".", ".x", "0x", "0x_", "0x__1", "0b2", "0o8", "1__2", "1_", "1._2", "1e", "1e+",
            "1e_2", "01", "00_1", "123abc", "1π", "0x1g", "1j2", "1j_",
        ] {
            assert_eq!(number(text.as_bytes(), 0), None, "number {text:?}");
        }
        assert_eq!(number(b"x1", 0), None);
        assert_eq!(number(b"x1", 2), None);
    }
}
