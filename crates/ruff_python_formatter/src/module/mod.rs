use ruff_formatter::FormatOwnedWithRule;
use ruff_python_ast::{Mod, ModKind};

use crate::prelude::*;

pub(crate) mod mod_expression;
pub(crate) mod mod_module;

#[derive(Default)]
pub struct FormatMod;

impl FormatRule<Mod<'_>, PyFormatContext<'_>> for FormatMod {
    fn fmt(&self, item: &Mod<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let item = *item;
        match item.classify() {
            ModKind::Module(x) => x.format().fmt(f),
            ModKind::Expression(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Mod<'_> {
    type Format = FormatOwnedWithRule<Self, FormatMod, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatMod)
    }
}
