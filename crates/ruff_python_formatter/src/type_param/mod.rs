use ruff_formatter::FormatOwnedWithRule;
use ruff_python_ast::TypeParam;
use ruff_python_ast::flat;

use crate::prelude::*;

pub(crate) mod type_param_param_spec;
pub(crate) mod type_param_type_var;
pub(crate) mod type_param_type_var_tuple;
pub(crate) mod type_params;

#[derive(Default)]
pub struct FormatTypeParam;

impl FormatRule<flat::View<'_, TypeParam>, PyFormatContext<'_>> for FormatTypeParam {
    fn fmt(&self, item: &flat::View<'_, TypeParam>, f: &mut PyFormatter) -> FormatResult<()> {
        let item = *item;
        match item.classify() {
            flat::TypeParamKind::TypeVar(x) => x.format().fmt(f),
            flat::TypeParamKind::TypeVarTuple(x) => x.format().fmt(f),
            flat::TypeParamKind::ParamSpec(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for flat::View<'_, TypeParam> {
    type Format = FormatOwnedWithRule<Self, FormatTypeParam, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatTypeParam)
    }
}
