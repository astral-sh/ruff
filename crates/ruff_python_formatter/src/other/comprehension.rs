use crate::{FormatNodeRule, PyFormatter};
use ruff_formatter::format_element::tag::VerbatimKind;
use ruff_formatter::prelude::{source_position, source_text_slice, ContainsNewlines, Tag};
use ruff_formatter::{write, Buffer, FormatElement, FormatResult};
use rustpython_parser::ast::Comprehension;

#[derive(Default)]
pub struct FormatComprehension;

impl FormatNodeRule<Comprehension> for FormatComprehension {
    fn fmt_fields(&self, item: &Comprehension, f: &mut PyFormatter) -> FormatResult<()> {
        write!(f, [source_position(item.range.start())])?;

        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            VerbatimKind::Verbatim {
                length: item.range.len(),
            },
        )))?;
        write!(f, [source_text_slice(item.range, ContainsNewlines::Detect)])?;
        f.write_element(FormatElement::Tag(Tag::EndVerbatim))?;

        write!(f, [source_position(item.range.end())])
    }
}
