mod argnames;
mod parametrization;
mod pytest_test;

use crate::db::Db;
use ruff_python_ast::{self as ast};
use ty_module_resolver::{ModuleName, ResolverEnvironment, resolve_module_confident};

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
        }
    }
}

pub(crate) fn is_pytest_available<'db>(
    db: &'db dyn Db,
    importing_file: ResolverEnvironment<'db>,
) -> bool {
    resolve_module_confident(
        db,
        importing_file,
        &ModuleName::new_static("pytest").unwrap(),
    )
    .is_some()
}
