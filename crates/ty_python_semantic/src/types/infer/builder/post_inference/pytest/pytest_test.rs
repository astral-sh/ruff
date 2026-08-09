use crate::types::{
    KnownClass, Type,
    infer::{
        TypeInferenceBuilder,
        builder::post_inference::pytest::{parametrization::Parametrization, request::Request},
    },
};
use ruff_python_ast::{self as ast};

/// Representation of a pytest test.
/// It is only used when the argnames and argvalues should be checked, and for that purpose only.
pub(crate) struct CheckablePytestTest<'db, 'ast> {
    name: &'ast ast::Identifier,
    requests: Vec<Request<'db, 'ast>>,
    parametrizations: Vec<Parametrization<'ast>>,
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(crate) fn build_pytest_test(
        &self,
        fn_def: &'ast ast::StmtFunctionDef,
        ty: Type<'db>,
    ) -> Option<CheckablePytestTest<'db, 'ast>> {
        // Check parametrize decorators (argnames) unconditionally.
        let parametrizations = self.build_parametrizations(&fn_def.decorator_list);
        if self.has_only_pytest_decorators(fn_def)
            // Do not build the requests unless all the decorators are Pytest marks.
            // Otherwise, there may be false positives with transformation decorators.
            && let Some(requests) = self.build_requests(fn_def, &ty)
        {
            Some(CheckablePytestTest {
                name: &fn_def.name,
                requests,
                parametrizations,
            })
        } else {
            None
        }
    }

    /// Checks that there are only `pytest.mark...` decorators and there is at least one.
    fn has_only_pytest_decorators(&self, fn_def: &ast::StmtFunctionDef) -> bool {
        let db = self.db();
        let env = self.program_environment();
        !fn_def.decorator_list.is_empty()
            && fn_def.decorator_list.iter().all(|decorator| {
                self.expression_type(&decorator.expression).is_subtype_of(
                    db,
                    env,
                    KnownClass::PytestMarkDecorator.to_instance(db, env),
                )
            })
    }
}
