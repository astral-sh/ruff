use crate::types::{
    InferContext, KnownClass, Type,
    diagnostic::PYTEST_DUPLICATE_ARGNAME,
    infer::{
        TypeInferenceBuilder, builder::post_inference::pytest::parametrization::Parametrization,
    },
};
use ruff_db::diagnostic::{Annotation, SubDiagnostic, SubDiagnosticSeverity};
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

/// Representation of a pytest test.
/// It is only used when the argnames and argvalues should be checked, and for that purpose only.
#[expect(dead_code)]
pub(crate) struct CheckablePytestTest<'db, 'ast> {
    name: &'ast ast::Identifier,
    // Type is not used for now, but acts as a placeholder for when more detailed typing
    // information is added.
    ty: Type<'db>,
    parametrizations: Vec<Parametrization<'ast>>,
}

impl<'db, 'ast> CheckablePytestTest<'db, 'ast> {
    pub(crate) fn check_duplicate_argnames(&self, context: &InferContext<'db, 'ast>) {
        let mut argname_locations = FxHashMap::default();
        for parametrization in &self.parametrizations {
            for argname in parametrization.argnames() {
                // Refer to the first location, so do not insert unless a use unless it is new.
                if let Some(previous_range) = argname_locations.get(argname.name()) {
                    if let Some(builder) =
                        context.report_lint(&PYTEST_DUPLICATE_ARGNAME, argname.range())
                    {
                        let mut diagnostic = builder
                            .into_diagnostic(format!("Duplicate argname `{}`", argname.name()));
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("`{}` already used here", argname.name()),
                        );
                        sub.annotate(Annotation::primary(context.span(previous_range)));
                        diagnostic.sub(sub);
                    }
                } else {
                    argname_locations.insert(argname.name(), argname.range());
                }
            }
        }
    }
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(crate) fn build_pytest_test(
        &self,
        fn_def: &'ast ast::StmtFunctionDef,
        ty: Type<'db>,
    ) -> Option<CheckablePytestTest<'db, 'ast>> {
        // Type is not used for now, but will be when performing type checking.
        // Check parameterize decorators (argnames) unconditionally.
        let parametrizations = self.build_parametrizations(&fn_def.decorator_list);
        if self.has_only_non_skipping_pytest_decorators(fn_def) {
            Some(CheckablePytestTest {
                name: &fn_def.name,
                ty,
                parametrizations,
            })
        } else {
            None
        }
    }

    /// Checks that there are only `pytest.mark...` decorators and there is at least one and none
    /// of them are `pytest.mark.skip`.
    fn has_only_non_skipping_pytest_decorators(&self, fn_def: &ast::StmtFunctionDef) -> bool {
        let db = self.db();
        let env = self.program_environment();
        !fn_def.decorator_list.is_empty()
            && fn_def.decorator_list.iter().all(|decorator| {
                self.expression_type(&decorator.expression).is_subtype_of(
                    db,
                    env,
                    KnownClass::PytestMarkDecorator.to_instance(db, env),
                ) && !self.expression_type(&decorator.expression).is_subtype_of(
                    db,
                    env,
                    KnownClass::PytestSkipMarkDecorator.to_instance(db, env),
                )
            })
    }
}
