use ruff_text_size::TextRange;

use super::{Builder, EmittedNode, Interval};
use crate::{BytesLiteralFlags, NodeKind, StringLiteralFlags, kind};

/// Appends the decoded contents of one literal, preserving all preceding values in the AST.
pub struct LiteralWriter<'a, T>(&'a mut T);

impl LiteralWriter<'_, String> {
    pub fn push(&mut self, ch: char) {
        self.0.push(ch);
    }

    pub fn push_str(&mut self, text: &str) {
        self.0.push_str(text);
    }
}

impl LiteralWriter<'_, Vec<u8>> {
    pub fn push(&mut self, byte: u8) {
        self.0.push(byte);
    }

    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
}

impl Builder {
    /// Decodes directly into the AST's text buffer and discards the partial value on error.
    pub fn try_string_literal<E>(
        &mut self,
        range: TextRange,
        flags: StringLiteralFlags,
        decode: impl FnOnce(&mut LiteralWriter<'_, String>) -> Result<(), E>,
    ) -> Result<EmittedNode<kind::StringLiteral>, E> {
        let value = self.decode_text(range, decode)?;
        let id = self.push_string_literal(range, [value, u64::from(flags.bits())]);
        Ok((id, range, NodeKind::StringLiteral))
    }

    /// Decodes a literal segment of an f-string or t-string into the AST's text buffer.
    pub fn try_interpolated_string_literal_element<E>(
        &mut self,
        range: TextRange,
        decode: impl FnOnce(&mut LiteralWriter<'_, String>) -> Result<(), E>,
    ) -> Result<EmittedNode<kind::InterpolatedStringLiteralElement>, E> {
        let value = self.decode_text(range, decode)?;
        let id = self.push_interpolated_string_literal_element(range, [value]);
        Ok((id, range, NodeKind::InterpolatedStringLiteralElement))
    }

    fn decode_text<E>(
        &mut self,
        range: TextRange,
        decode: impl FnOnce(&mut LiteralWriter<'_, String>) -> Result<(), E>,
    ) -> Result<u64, E> {
        let text = &mut self.ast.text;
        let start = text.len();
        // Decoding escapes can only shorten the source. Reserving its length once
        // avoids doubling the buffer for the last few bytes of a long literal.
        text.reserve(range.len().to_usize());
        if let Err(error) = decode(&mut LiteralWriter(text)) {
            text.truncate(start);
            return Err(error);
        }
        Ok(Interval::new(start, text.len() - start).encode())
    }

    /// Decodes directly into the AST's bytes buffer and discards the partial value on error.
    pub fn try_bytes_literal<E>(
        &mut self,
        range: TextRange,
        flags: BytesLiteralFlags,
        decode: impl FnOnce(&mut LiteralWriter<'_, Vec<u8>>) -> Result<(), E>,
    ) -> Result<EmittedNode<kind::BytesLiteral>, E> {
        let bytes = &mut self.ast.bytes;
        let start = bytes.len();
        bytes.reserve(range.len().to_usize());
        if let Err(error) = decode(&mut LiteralWriter(bytes)) {
            bytes.truncate(start);
            return Err(error);
        }
        let value = Interval::new(start, bytes.len() - start).encode();
        let id = self.push_bytes_literal(range, [value, u64::from(flags.bits())]);
        Ok((id, range, NodeKind::BytesLiteral))
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::convert::Infallible;

    use super::{Builder, BytesLiteralFlags, StringLiteralFlags, TextRange};
    use crate::flat::{BytesValue, Emit, build};

    #[test]
    fn string_lengths_at_inline_limit() -> Result<(), Infallible> {
        let range = TextRange::default();
        let mut builder = Builder::default();
        for len in [
            0,
            1,
            usize::from(u16::MAX) - 1,
            usize::from(u16::MAX),
            80_000,
        ] {
            let text = "x".repeat(len);
            let (part, ..) =
                builder.try_string_literal(range, StringLiteralFlags::empty(), |out| {
                    out.push_str(&text);
                    Ok::<(), Infallible>(())
                })?;
            let value = builder.list([part]);
            let (literal, ..) = build::ExprStringLiteral { range, value }.emit(&mut builder);
            let value = builder.get(literal).value();
            assert_eq!(value.is_empty(), len == 0);
            assert_eq!(value.len(), len);
            assert_eq!(value.to_str(), text);
        }
        Ok(())
    }

    #[test]
    fn byte_literals_borrow_consecutive_parts() -> Result<(), Infallible> {
        let mut builder = Builder::default();
        let mut part = |data: &[u8]| {
            builder
                .try_bytes_literal(TextRange::default(), BytesLiteralFlags::empty(), |out| {
                    out.extend_from_slice(data);
                    Ok::<(), Infallible>(())
                })
                .map(|(id, _, _)| id)
        };
        let a = part(b"a")?;
        let empty = part(b"")?;
        let b = part(b"b")?;
        let c = part(b"c")?;
        let adjacent = builder.list([a, empty, b]);
        let gap = builder.list([a, c]);
        let reversed = builder.list([b, a]);
        let value = |list| Cow::<[u8]>::from(BytesValue::new(builder.get_list(list)));
        assert!(matches!(value(adjacent), Cow::Borrowed(b"ab")));
        assert_eq!(value(gap).as_ref(), b"ac");
        assert_eq!(value(reversed).as_ref(), b"ba");
        Ok(())
    }

    #[test]
    fn decoding_errors_discard_only_the_partial_literal() {
        let mut builder = Builder::default();
        let range = TextRange::default();
        builder.store_string("before");
        builder.store_bytes(b"before");
        let rows = builder.node_count();
        let text = builder.try_string_literal(range, StringLiteralFlags::empty(), |out| {
            out.push_str("partial");
            Err(())
        });
        let bytes = builder.try_bytes_literal(range, BytesLiteralFlags::empty(), |out| {
            out.extend_from_slice(b"partial");
            Err(())
        });
        assert!(text.is_err());
        assert!(bytes.is_err());
        assert_eq!(builder.ast.text, "before");
        assert_eq!(builder.ast.bytes, b"before");
        assert_eq!(builder.node_count(), rows);
    }
}
