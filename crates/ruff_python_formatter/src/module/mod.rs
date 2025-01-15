use ruff_formatter::FormatOwnedWithRule;
use ruff_python_ast::Mod;

use crate::prelude::*;

pub(crate) mod mod_expression;
pub(crate) mod mod_module;

#[derive(Default)]
pub struct FormatMod;

impl FormatRule<Mod<'_>, PyFormatContext<'_>> for FormatMod {
    fn fmt(&self, item: &Mod<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            Mod::Module(x) => x.format().fmt(f),
            Mod::Expression(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Mod<'ast> {
    type Format = FormatOwnedWithRule<Mod<'ast>, FormatMod, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatMod)
    }
}
