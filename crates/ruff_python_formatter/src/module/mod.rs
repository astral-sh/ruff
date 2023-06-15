use crate::context::PyFormatContext;
use crate::{AsFormat, IntoFormat, PyFormatter};
use ruff_formatter::{Format, FormatOwnedWithRule, FormatRefWithRule, FormatResult, FormatRule};
use rustpython_parser::ast::Mod;

pub(crate) mod mod_expression;
pub(crate) mod mod_function_type;
pub(crate) mod mod_interactive;
pub(crate) mod mod_module;

#[derive(Default)]
pub struct FormatMod;

impl FormatRule<Mod, PyFormatContext<'_>> for FormatMod {
    fn fmt(&self, item: &Mod, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            Mod::Module(x) => x.format().fmt(f),
            Mod::Interactive(x) => x.format().fmt(f),
            Mod::Expression(x) => x.format().fmt(f),
            Mod::FunctionType(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for Mod {
    type Format<'a> = FormatRefWithRule<'a, Mod, FormatMod, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatMod::default())
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for Mod {
    type Format = FormatOwnedWithRule<Mod, FormatMod, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatMod::default())
    }
}
