use ruff_formatter::{FormatOwnedWithRule, FormatRefWithRule};
use ruff_python_ast::TypeParam;

use crate::prelude::*;

pub(crate) mod type_param_param_spec;
pub(crate) mod type_param_type_var;
pub(crate) mod type_param_type_var_tuple;
pub(crate) mod type_params;

#[derive(Default)]
pub struct FormatTypeParam;

impl FormatRule<TypeParam, PyFormatContext<'_>> for FormatTypeParam {
    fn fmt(&self, item: &TypeParam, f: &mut PyFormatter) -> FormatResult<()> {
        match item {
            TypeParam::TypeVar(x) => x.format().fmt(f),
            TypeParam::TypeVarTuple(x) => x.format().fmt(f),
            TypeParam::ParamSpec(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> AsFormat<PyFormatContext<'ast>> for TypeParam {
    type Format<'a> = FormatRefWithRule<'a, TypeParam, FormatTypeParam, PyFormatContext<'ast>>;

    fn format(&self) -> Self::Format<'_> {
        FormatRefWithRule::new(self, FormatTypeParam)
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for TypeParam {
    type Format = FormatOwnedWithRule<TypeParam, FormatTypeParam, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatTypeParam)
    }
}
