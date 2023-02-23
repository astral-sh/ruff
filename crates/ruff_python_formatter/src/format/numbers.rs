use ruff_formatter::prelude::*;
use ruff_formatter::{write, Format};
use ruff_text_size::TextSize;

use crate::builders::literal;
use crate::context::ASTFormatContext;
use crate::core::types::Range;

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
