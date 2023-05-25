use std::ops::{Add, Sub};

use crate::prelude::*;
use ruff_formatter::{write, Format};
use ruff_text_size::{TextRange, TextSize};

use crate::format::builders::literal;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct FloatAtom {
    range: TextRange,
}

impl Format<ASTFormatContext<'_>> for FloatAtom {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let contents = f.context().contents();

        let content = &contents[self.range];
        if let Some(dot_index) = content.find('.') {
            let integer = &content[..dot_index];
            let fractional = &content[dot_index + 1..];

            if integer.is_empty() {
                write!(f, [text("0")])?;
            } else {
                write!(
                    f,
                    [literal(
                        TextRange::new(
                            self.range.start(),
                            self.range
                                .start()
                                .add(TextSize::try_from(dot_index).unwrap())
                        ),
                        ContainsNewlines::No
                    )]
                )?;
            }

            write!(f, [text(".")])?;

            if fractional.is_empty() {
                write!(f, [text("0")])?;
            } else {
                write!(
                    f,
                    [literal(
                        TextRange::new(
                            self.range
                                .start()
                                .add(TextSize::try_from(dot_index + 1).unwrap()),
                            self.range.end()
                        ),
                        ContainsNewlines::No
                    )]
                )?;
            }
        } else {
            write!(f, [literal(self.range, ContainsNewlines::No)])?;
        }

        Ok(())
    }
}

#[inline]
const fn float_atom(range: TextRange) -> FloatAtom {
    FloatAtom { range }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct FloatLiteral {
    range: TextRange,
}

impl Format<ASTFormatContext<'_>> for FloatLiteral {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let contents = f.context().contents();

        let content = &contents[self.range];

        // Scientific notation
        if let Some(exponent_index) = content.find('e').or_else(|| content.find('E')) {
            // Write the base.
            write!(
                f,
                [float_atom(TextRange::new(
                    self.range.start(),
                    self.range
                        .start()
                        .add(TextSize::try_from(exponent_index).unwrap())
                ))]
            )?;

            write!(f, [text("e")])?;

            // Write the exponent, omitting the sign if it's positive.
            let plus = content[exponent_index + 1..].starts_with('+');
            write!(
                f,
                [literal(
                    TextRange::new(
                        self.range.start().add(
                            TextSize::try_from(exponent_index + 1 + usize::from(plus)).unwrap()
                        ),
                        self.range.end()
                    ),
                    ContainsNewlines::No
                )]
            )?;
        } else {
            write!(f, [float_atom(self.range)])?;
        }

        Ok(())
    }
}

#[inline]
pub(crate) const fn float_literal(range: TextRange) -> FloatLiteral {
    FloatLiteral { range }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct IntLiteral {
    range: TextRange,
}

impl Format<ASTFormatContext<'_>> for IntLiteral {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let contents = f.context().contents();

        for prefix in ["0b", "0B", "0o", "0O", "0x", "0X"] {
            let content = &contents[self.range];
            if content.starts_with(prefix) {
                // In each case, the prefix must be lowercase, while the suffix must be uppercase.
                let prefix = &content[..prefix.len()];
                let suffix = &content[prefix.len()..];

                if prefix.bytes().any(|b| b.is_ascii_uppercase())
                    || suffix.bytes().any(|b| b.is_ascii_lowercase())
                {
                    // Write out the fixed version.
                    write!(
                        f,
                        [
                            dynamic_text(&prefix.to_lowercase(), None),
                            dynamic_text(&suffix.to_uppercase(), None)
                        ]
                    )?;
                } else {
                    // Use the existing source.
                    write!(f, [literal(self.range, ContainsNewlines::No)])?;
                }

                return Ok(());
            }
        }

        write!(f, [literal(self.range, ContainsNewlines::No)])?;

        Ok(())
    }
}

#[inline]
pub(crate) const fn int_literal(range: TextRange) -> IntLiteral {
    IntLiteral { range }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) struct ComplexLiteral {
    range: TextRange,
}

impl Format<ASTFormatContext<'_>> for ComplexLiteral {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let contents = f.context().contents();
        let content = &contents[self.range];

        if content.ends_with('j') {
            write!(f, [literal(self.range, ContainsNewlines::No)])?;
        } else if content.ends_with('J') {
            write!(
                f,
                [literal(
                    TextRange::new(self.range.start(), self.range.end().sub(TextSize::from(1))),
                    ContainsNewlines::No
                )]
            )?;
            write!(f, [text("j")])?;
        } else {
            unreachable!("expected complex literal to end with j or J");
        }

        Ok(())
    }
}

#[inline]
pub(crate) const fn complex_literal(range: TextRange) -> ComplexLiteral {
    ComplexLiteral { range }
}
