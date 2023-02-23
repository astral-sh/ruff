use ruff_formatter::prelude::*;
use ruff_formatter::{write, Format};
use ruff_text_size::TextSize;
use rustpython_parser::ast::Location;

use crate::builders::literal;
use crate::context::ASTFormatContext;
use crate::core::types::Range;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct FloatAtom {
    range: Range,
}

impl Format<ASTFormatContext<'_>> for FloatAtom {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let (source, start, end) = f.context().locator().slice(self.range);

        if let Some(dot_index) = source[start..end].find('.') {
            let integer = &source[start..start + dot_index];
            let fractional = &source[start + dot_index + 1..end];

            if integer.is_empty() {
                write!(f, [text("0")])?;
            } else {
                write!(
                    f,
                    [literal(Range::new(
                        self.range.location,
                        Location::new(
                            self.range.location.row(),
                            self.range.location.column() + dot_index
                        ),
                    ))]
                )?;
            }

            write!(f, [text(".")])?;

            if fractional.is_empty() {
                write!(f, [text("0")])?;
            } else {
                write!(
                    f,
                    [literal(Range::new(
                        Location::new(
                            self.range.location.row(),
                            self.range.location.column() + dot_index + 1
                        ),
                        self.range.end_location
                    ))]
                )?;
            }
        } else {
            write!(f, [literal(self.range)])?;
        }

        Ok(())
    }
}

#[inline]
const fn float_atom(range: Range) -> FloatAtom {
    FloatAtom { range }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct FloatLiteral {
    range: Range,
}

impl Format<ASTFormatContext<'_>> for FloatLiteral {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let (source, start, end) = f.context().locator().slice(self.range);

        // Scientific notation
        if let Some(exponent_index) = source[start..end]
            .find('e')
            .or_else(|| source[start..end].find('E'))
        {
            // Write the base.
            write!(
                f,
                [float_atom(Range::new(
                    self.range.location,
                    Location::new(
                        self.range.location.row(),
                        self.range.location.column() + exponent_index
                    ),
                ))]
            )?;

            write!(f, [text("e")])?;

            // Write the exponent, omitting the sign if it's positive.
            let plus = source[start + exponent_index + 1..end].starts_with('+');
            write!(
                f,
                [literal(Range::new(
                    Location::new(
                        self.range.location.row(),
                        self.range.location.column() + exponent_index + 1 + usize::from(plus)
                    ),
                    self.range.end_location
                ))]
            )?;
        } else {
            write!(f, [float_atom(self.range)])?;
        }

        Ok(())
    }
}

#[inline]
pub const fn float_literal(range: Range) -> FloatLiteral {
    FloatLiteral { range }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct IntLiteral {
    range: Range,
}

impl Format<ASTFormatContext<'_>> for IntLiteral {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let (source, start, end) = f.context().locator().slice(self.range);

        for prefix in ["0b", "0B", "0o", "0O", "0x", "0X"] {
            if source[start..end].starts_with(prefix) {
                // In each case, the prefix must be lowercase, while the suffix must be uppercase.
                let prefix = &source[start..start + prefix.len()];
                let suffix = &source[start + prefix.len()..end];

                if prefix.bytes().any(|b| b.is_ascii_uppercase())
                    || suffix.bytes().any(|b| b.is_ascii_lowercase())
                {
                    // Write out the fixed version.
                    write!(
                        f,
                        [
                            dynamic_text(&prefix.to_lowercase(), TextSize::default()),
                            dynamic_text(&suffix.to_uppercase(), TextSize::default())
                        ]
                    )?;
                } else {
                    // Use the existing source.
                    write!(f, [literal(self.range)])?;
                }

                return Ok(());
            }
        }

        write!(f, [literal(self.range)])?;

        Ok(())
    }
}

#[inline]
pub const fn int_literal(range: Range) -> IntLiteral {
    IntLiteral { range }
}
