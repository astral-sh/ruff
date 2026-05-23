use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::Mod;

use crate::prelude::*;

pub(crate) mod mod_expression;
pub(crate) mod mod_module;

#[derive(Default)]
pub struct FormatMod;

impl FormatRule<Mod<'_>, PyFormatContext<'_>> for FormatMod {
    fn fmt(&self, item: &Mod, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            Mod::Module(x) => x.format().fmt(f),
            Mod::Expression(x) => x.format().fmt(f),
        }
    }
}

impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for Mod<'ast> {
    type Format<'a>
        = FormatRefWithRule<'a, Mod<'ast>, FormatMod, PyFormatContext<'context>>
    where
        Self: 'a;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatMod)
    }
}

impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for Mod<'ast> {
    type Format = FormatOwnedWithRule<Mod<'ast>, FormatMod, PyFormatContext<'context>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatMod)
    }
}
