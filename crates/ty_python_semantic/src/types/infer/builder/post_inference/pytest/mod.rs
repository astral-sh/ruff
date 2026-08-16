mod argnames;
mod parametrization;
mod partial_signature;
mod pytest_test;
mod request;

use ruff_python_ast::{self as ast};

use crate::types::{Type, infer::TypeInferenceBuilder};

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    /// This is the only method exposed to and called by `builder.rs`.
    /// All other methods are encapsulated.
    /// This may change in the future if a different interface is needed.
    pub(crate) fn post_inference_pytest_check_function(
        &mut self,
        ty: Type<'db>,
        node: &'ast ast::StmtFunctionDef,
    ) {
        if let Some(test) = self.build_pytest_test(node, ty) {
            test.check_duplicate_argnames(&self.context);
            self.check_pytest_argvalues(&test);
        }
    }
}
