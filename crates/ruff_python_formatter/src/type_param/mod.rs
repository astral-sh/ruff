use ruff_formatter::FormatOwnedWithRule;
use ruff_python_ast::{TypeParam, TypeParamKind};

use crate::prelude::*;

pub(crate) mod type_param_param_spec;
pub(crate) mod type_param_type_var;
pub(crate) mod type_param_type_var_tuple;
pub(crate) mod type_params;

#[derive(Default)]
pub struct FormatTypeParam;

impl FormatRule<TypeParam<'_>, PyFormatContext<'_>> for FormatTypeParam {
    fn fmt(&self, item: &TypeParam<'_>, f: &mut PyFormatter) -> FormatResult<()> {
        let item = *item;
        match item.classify() {
            TypeParamKind::TypeVar(x) => x.format().fmt(f),
            TypeParamKind::TypeVarTuple(x) => x.format().fmt(f),
            TypeParamKind::ParamSpec(x) => x.format().fmt(f),
        }
    }
}

impl<'ast> IntoFormat<PyFormatContext<'ast>> for TypeParam<'_> {
    type Format = FormatOwnedWithRule<Self, FormatTypeParam, PyFormatContext<'ast>>;

    fn into_format(self) -> Self::Format {
        FormatOwnedWithRule::new(self, FormatTypeParam)
    }
}
