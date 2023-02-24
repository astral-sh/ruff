use ruff_formatter::prelude::*;
use ruff_formatter::write;
use ruff_text_size::TextSize;

use crate::builders::literal;
use crate::context::ASTFormatContext;
use crate::cst::{Excepthandler, ExcepthandlerKind};
use crate::format::builders::block;
use crate::shared_traits::AsFormat;
use crate::trivia::TriviaKind;

pub struct FormatExcepthandler<'a> {
    item: &'a Excepthandler,
}

impl AsFormat<ASTFormatContext<'_>> for Excepthandler {
    type Format<'a> = FormatExcepthandler<'a>;

    fn format(&self) -> Self::Format<'_> {
        FormatExcepthandler { item: self }
    }
}

impl Format<ASTFormatContext<'_>> for FormatExcepthandler<'_> {
    fn fmt(&self, f: &mut Formatter<ASTFormatContext>) -> FormatResult<()> {
        let excepthandler = self.item;
        let ExcepthandlerKind::ExceptHandler { type_, name, body } = &excepthandler.node;

        write!(f, [text("except")])?;
        if let Some(type_) = &type_ {
            write!(f, [space(), type_.format()])?;
            if let Some(name) = &name {
                write!(
                    f,
                    [
                        space(),
                        text("as"),
                        space(),
                        dynamic_text(name, TextSize::default()),
                    ]
                )?;
            }
        }
        write!(f, [text(":")])?;

        // Format any end-of-line comments.
        let mut first = true;
        for range in excepthandler.trivia.iter().filter_map(|trivia| {
            if trivia.relationship.is_trailing() {
                if let TriviaKind::EndOfLineComment(range) = trivia.kind {
                    Some(range)
                } else {
                    None
                }
            } else {
                None
            }
        }) {
            if std::mem::take(&mut first) {
                write!(f, [line_suffix(&text("  "))])?;
            }
            write!(f, [line_suffix(&literal(range))])?;
        }

        write!(f, [block_indent(&block(body))])?;

        Ok(())
    }
}
