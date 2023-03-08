use rustpython_parser::ast::Location;

use ruff_formatter::prelude::*;
use ruff_formatter::{write, Format};
use ruff_python_ast::types::Range;
use ruff_text_size::TextSize;

use crate::context::ASTFormatContext;
use crate::format::builders::literal;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct FloatAtom {
    range: Range,
}

impl Format<ASTFormatContext<'_>> for FloatAtom {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let locator = f.context().locator();
        let contents = f.context().contents();
        let start_index = locator.offset(self.range.location);
        let end_index = locator.offset(self.range.end_location);

        if let Some(dot_index) = contents[start_index..end_index].find('.') {
            let integer = &contents[start_index..start_index + dot_index];
            let fractional = &contents[start_index + dot_index + 1..end_index];

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
        let locator = f.context().locator();
        let contents = f.context().contents();
        let start_index = locator.offset(self.range.location);
        let end_index = locator.offset(self.range.end_location);

        // Scientific notation
        if let Some(exponent_index) = contents[start_index..end_index]
            .find('e')
            .or_else(|| contents[start_index..end_index].find('E'))
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
            let plus = contents[start_index + exponent_index + 1..end_index].starts_with('+');
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
        let locator = f.context().locator();
        let contents = f.context().contents();
        let start_index = locator.offset(self.range.location);
        let end_index = locator.offset(self.range.end_location);

        for prefix in ["0b", "0B", "0o", "0O", "0x", "0X"] {
            if contents[start_index..end_index].starts_with(prefix) {
                // In each case, the prefix must be lowercase, while the suffix must be uppercase.
                let prefix = &contents[start_index..start_index + prefix.len()];
                let suffix = &contents[start_index + prefix.len()..end_index];

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

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct ComplexLiteral {
    range: Range,
}

impl Format<ASTFormatContext<'_>> for ComplexLiteral {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext<'_>>) -> FormatResult<()> {
        let locator = f.context().locator();
        let contents = f.context().contents();
        let start_index = locator.offset(self.range.location);
        let end_index = locator.offset(self.range.end_location);

        if contents[start_index..end_index].ends_with('j') {
            write!(f, [literal(self.range)])?;
        } else if contents[start_index..end_index].ends_with('J') {
            write!(
                f,
                [literal(Range::new(
                    self.range.location,
                    Location::new(
                        self.range.end_location.row(),
                        self.range.end_location.column() - 1
                    ),
                ))]
            )?;
            write!(f, [text("j")])?;
        } else {
            unreachable!("expected complex literal to end with j or J");
        }

        Ok(())
    }
}

#[inline]
pub const fn complex_literal(range: Range) -> ComplexLiteral {
    ComplexLiteral { range }
}
