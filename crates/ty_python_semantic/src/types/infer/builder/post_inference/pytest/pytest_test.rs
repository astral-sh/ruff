use crate::types::{
    Type,
    infer::{
        TypeInferenceBuilder,
        builder::post_inference::pytest::{parametrization::Parametrization, request::Request},
    },
};
use ruff_python_ast::{self as ast};

/// Representation of a pytest test that is necessary for type checking.
pub(crate) struct PytestTest<'db, 'ast> {
    name: ast::Identifier,
    requests: Vec<Request<'db, 'ast>>,
    parametrizations: Vec<Parametrization<'ast>>,
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(crate) fn build_pytest_test(&self, node: &'ast ast::StmtFunctionDef, ty: Type<'db>) {
        let _parametrizations = self.build_parametrizations(&node.decorator_list);
        let _requests = self.build_requests(node, &ty);
    }
}
