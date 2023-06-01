pub(crate) mod mod_expression;
pub(crate) mod mod_function_type;
pub(crate) mod mod_interactive;
pub(crate) mod mod_module;

use crate::context::PyFormatContext;
use ruff_formatter::format_element::tag::VerbatimKind;
use ruff_formatter::prelude::*;
use ruff_formatter::write;
use rustpython_parser::ast::{Mod, Ranged};

pub(crate) struct FormatModule<'a> {
    module: &'a Mod,
}

impl<'a> FormatModule<'a> {
    pub(crate) fn new(module: &'a Mod) -> Self {
        Self { module }
    }
}

impl Format<PyFormatContext<'_>> for FormatModule<'_> {
    fn fmt(&self, f: &mut Formatter<PyFormatContext<'_>>) -> FormatResult<()> {
        let range = self.module.range();

        write!(f, [source_position(range.start())])?;

        f.write_element(FormatElement::Tag(Tag::StartVerbatim(
            VerbatimKind::Verbatim {
                length: range.len(),
            },
        )))?;
        write!(f, [source_text_slice(range, ContainsNewlines::Detect)])?;
        f.write_element(FormatElement::Tag(Tag::EndVerbatim))?;

        write!(f, [source_position(range.end())])
    }
}
