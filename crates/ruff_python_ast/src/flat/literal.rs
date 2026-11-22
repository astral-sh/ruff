use super::{Ast, AstNode, Builder, offset, split_words};
use crate::int::IntValue;
use crate::{Int, Number};

// Byte 8 identifies how to read the first eight bytes of an ExprNumberLiteral row.
#[derive(Clone, Copy)]
#[repr(u8)]
pub(super) enum NumberKind {
    Int,
    Float,
    BigInt,
    Complex,
}

impl Builder {
    pub(super) fn store_number(&mut self, number: Number<'_>) -> (u64, NumberKind) {
        match number {
            Number::Int(Int(IntValue::Small(value))) => (value, NumberKind::Int),
            Number::Int(Int(IntValue::Big(text))) => (self.store_string(text), NumberKind::BigInt),
            Number::Float(value) => (value.to_bits(), NumberKind::Float),
            Number::Complex { real, imag } => {
                // A constructed complex literal can have a nonzero real component.
                let base = offset(self.ast.extra.len());
                self.ast.extra.extend(split_words(real.to_bits()));
                self.ast.extra.extend(split_words(imag.to_bits()));
                (u64::from(base), NumberKind::Complex)
            }
        }
    }
}

impl<'a> crate::ExprNumberLiteral<'a> {
    #[inline]
    pub(super) fn int_value(self) -> Option<Int<&'a str>> {
        // A caller checking for an integer can skip decoding complex components from extra.
        let node = self.syntax_node();
        node.ast.read_integer(
            node.row.expr_number_literal_value(),
            node.row.expr_number_literal_number_kind(),
        )
    }
}

impl Ast {
    #[inline]
    fn read_integer(&self, word: u64, kind: u8) -> Option<Int<&str>> {
        match kind {
            k if k == NumberKind::Int as u8 => Some(Int::from(word)),
            k if k == NumberKind::BigInt as u8 => Some(Int(IntValue::Big(self.string(word)))),
            _ => None,
        }
    }

    #[inline]
    pub(super) fn read_number(&self, word: u64, kind: u8) -> Number<'_> {
        if let Some(integer) = self.read_integer(word, kind) {
            return Number::Int(integer);
        }
        match kind {
            k if k == NumberKind::Float as u8 => Number::Float(f64::from_bits(word)),
            k if k == NumberKind::Complex as u8 => {
                let base = split_words(word)[0] as usize;
                Number::Complex {
                    real: f64::from_bits(self.extra_word::<true>(base)),
                    imag: f64::from_bits(self.extra_word::<true>(base + 2)),
                }
            }
            _ => unreachable!("invalid number kind"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::flat::{BytesValue, Emit, build};
    use crate::{Builder, BytesLiteralFlags, Int, Number, StringLiteralFlags};
    use ruff_text_size::TextRange;

    #[test]
    fn copy_numbers_between_arenas() {
        let range = TextRange::default();
        let big: Int = "18446744073709551616".parse().unwrap();
        let mut source = Builder::default();
        let values = [
            Number::Int(Int::from(u64::MAX)),
            Number::Int(big.as_borrowed()),
            Number::Float(-0.0),
            Number::Complex {
                real: 1.5,
                imag: f64::from_bits(0x7ff8_0000_0000_0001),
            },
        ];
        let mut destination = Builder::default();
        // Put the copied fields at different offsets in both text and extra.
        destination.add(build::ExprNumberLiteral {
            range,
            value: Number::Int(big.as_borrowed()),
        });
        destination.add(build::ExprNumberLiteral {
            range,
            value: Number::Complex {
                real: 0.0,
                imag: 0.0,
            },
        });
        let ids = values.map(|value| {
            let id = source.add(build::ExprNumberLiteral { range, value });
            destination.copy_node(source.get(id))
        });
        drop(source);
        for (id, expected) in ids.into_iter().zip(values) {
            let node = destination.get(id);
            assert_eq!(
                crate::Expr::from(node).as_int_literal(),
                expected.as_int().copied()
            );
            match (node.value(), expected) {
                (Number::Float(a), Number::Float(b)) => assert_eq!(a.to_bits(), b.to_bits()),
                (Number::Complex { real: a, imag: b }, Number::Complex { real: x, imag: y }) => {
                    assert_eq!(a.to_bits(), x.to_bits());
                    assert_eq!(b.to_bits(), y.to_bits());
                }
                (actual, expected) => assert_eq!(actual, expected),
            }
        }
    }

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
