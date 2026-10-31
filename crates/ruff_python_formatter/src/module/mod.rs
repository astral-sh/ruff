use ruff_formatter::FormatOwnedWithRule;
use ruff_python_ast::Mod;
use ruff_python_ast::flat;

use crate::prelude::*;

pub(crate) mod mod_expression;
pub(crate) mod mod_module;

#[derive(Default)]
pub struct FormatMod;

impl FormatRule<flat::View<'_, Mod>, PyFormatContext<'_>> for FormatMod {
    fn fmt(&self, item: &flat::View<'_, Mod>, f: &mut PyFormatter) -> FormatResult<()> {
        let item = *item;
        match item.classify() {
            flat::ModKind::Module(x) => x.format().fmt(f),
            flat::ModKind::Expression(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for flat::View<'_, Mod> {
    type Format = FormatOwnedWithRule<Self, FormatMod, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatMod)
    }
}
