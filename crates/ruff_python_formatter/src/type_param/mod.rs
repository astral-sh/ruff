use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::TypeParam;

use crate::prelude::*;

pub(crate) mod type_param_param_spec;
pub(crate) mod type_param_type_var;
pub(crate) mod type_param_type_var_tuple;
pub(crate) mod type_params;

#[derive(Default)]
pub struct FormatTypeParam;

impl FormatRule<TypeParam<'_>, PyFormatContext<'_>> for FormatTypeParam {
    fn fmt(&self, item: &TypeParam, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            TypeParam::TypeVar(x) => x.format().fmt(f),
            TypeParam::TypeVarTuple(x) => x.format().fmt(f),
            TypeParam::ParamSpec(x) => x.format().fmt(f),
        }
    }
}

impl<'ast, 'context> AsFormat<PyFormatContext<'context>> for TypeParam<'ast> {
    type Format<'a>
        = FormatRefWithRule<'a, TypeParam<'ast>, FormatTypeParam, PyFormatContext<'context>>
    where
        Self: 'a;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatTypeParam)
    }
}

impl<'ast, 'context> IntoFormat<PyFormatContext<'context>> for TypeParam<'ast> {
    type Format = FormatOwnedWithRule<TypeParam<'ast>, FormatTypeParam, PyFormatContext<'context>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatTypeParam)
    }
}
