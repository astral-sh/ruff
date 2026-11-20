#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::flat::{BytesValue, Emit, build};
    use crate::{Builder, BytesLiteralFlags, StringLiteralFlags};
    use ruff_text_size::TextRange;

    #[test]
    fn string_lengths_at_inline_limit() {
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
            let part = builder.add(build::StringLiteral {
                range,
                value: &text,
                flags: StringLiteralFlags::empty(),
            });
            let value = builder.list([part]);
            let literal = build::ExprStringLiteral { range, value }.emit(&mut builder);
            let value = builder.get(literal).value();
            assert_eq!(value.is_empty(), len == 0);
            assert_eq!(value.len(), len);
            assert_eq!(value.to_str(), text);
        }
    }

    #[test]
    fn byte_literals_borrow_consecutive_parts() {
        let mut builder = Builder::default();
        let mut part = |data: &[u8]| {
            builder.add(build::BytesLiteral {
                range: TextRange::default(),
                value: data,
                flags: BytesLiteralFlags::empty(),
            })
        };
        let a = part(b"a");
        let empty = part(b"");
        let b = part(b"b");
        let c = part(b"c");
        let adjacent = builder.list([a, empty, b]);
        let gap = builder.list([a, c]);
        let reversed = builder.list([b, a]);
        let value = |list| Cow::<[u8]>::from(BytesValue::new(builder.get_list(list)));
        assert!(matches!(value(adjacent), Cow::Borrowed(b"ab")));
        assert_eq!(value(gap).as_ref(), b"ac");
        assert_eq!(value(reversed).as_ref(), b"ba");
    }
}
