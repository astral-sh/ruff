use crate::types::{
    CallArguments, KnownClass, Type, TypeContext,
    call::{CallDiagnosticOverride, CallErrorKind},
    diagnostic::{PYTEST_DUPLICATE_ARGNAME, PYTEST_PARAM_MISMATCHED_TYPE},
    infer::{
        TypeInferenceBuilder,
        builder::{
            ArgumentsIter,
            post_inference::pytest::{
                parametrization::Parametrization, request::Request, sub_signature::SubSignature,
            },
        },
    },
};
use itertools::Itertools;
use ruff_db::diagnostic::{Annotation, SubDiagnostic, SubDiagnosticSeverity};
use ruff_python_ast::{self as ast, AtomicNodeIndex};
use ruff_text_size::Ranged;
use rustc_hash::FxHashMap;
use std::debug_assert_matches;

/// Representation of a pytest test.
/// It is only used when the argnames and argvalues should be checked, and for that purpose only.
pub(crate) struct CheckablePytestTest<'db, 'ast> {
    name: &'ast ast::Identifier,
    requests: Vec<Request<'db, 'ast>>,
    parametrizations: Vec<Parametrization<'ast>>,
}

impl<'db, 'ast> CheckablePytestTest<'db, 'ast> {
    pub(crate) fn name(&self) -> &'ast ast::Identifier {
        self.name
    }

    pub(crate) fn requests(&self) -> &[Request<'db, 'ast>] {
        &self.requests
    }
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(crate) fn build_pytest_test(
        &self,
        fn_def: &'ast ast::StmtFunctionDef,
        ty: Type<'db>,
    ) -> Option<CheckablePytestTest<'db, 'ast>> {
        // Check parameterize decorators (argnames) unconditionally.
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

    pub(crate) fn check_duplicate_argnames(&self, test: &CheckablePytestTest) {
        let mut argname_locations = FxHashMap::default();
        for parametrization in &test.parametrizations {
            for argname in parametrization.argnames() {
                if let Some(previous_range) =
                    argname_locations.insert(argname.name(), argname.range())
                {
                    if let Some(builder) = self
                        .context
                        .report_lint(&PYTEST_DUPLICATE_ARGNAME, argname.range())
                    {
                        let mut diagnostic = builder
                            .into_diagnostic(format!("Duplicate argname `{}`", argname.name()));
                        let mut sub = SubDiagnostic::new(
                            SubDiagnosticSeverity::Info,
                            format_args!("`{}` already used here", argname.name()),
                        );
                        sub.annotate(Annotation::primary(self.context.span(previous_range)));
                        diagnostic.sub(sub);
                    }
                }
            }
        }
    }

    pub(crate) fn check_pytest_test(&mut self, test: &CheckablePytestTest<'db, 'ast>) {
        for parametrization in &test.parametrizations {
            self.check_parametrization(parametrization, test);
        }
    }

    fn check_parametrization(
        &mut self,
        parametrization: &Parametrization<'ast>,
        test: &CheckablePytestTest<'db, 'ast>,
    ) {
        if let Some(sub_signature) = test.sub_signature(self.db(), parametrization.argnames()) {
            self.check_argvalues_against(&sub_signature, parametrization.argvalues());
        }
    }

    fn check_argvalues_against(
        &mut self,
        sub_signature: &SubSignature<'db, 'ast>,
        argvalues: &'ast ast::Expr,
    ) {
        if let Some(list) = argvalues.as_list_expr() {
            self.check_argvalue_items(sub_signature, &list.elts);
        } else if let Some(tuple) = argvalues.as_tuple_expr() {
            self.check_argvalue_items(sub_signature, &tuple.elts);
        } else {
            self.check_argvalue_test_cases(sub_signature, argvalues);
        }
    }

    fn check_argvalue_items(
        &mut self,
        sub_signature: &SubSignature<'db, 'ast>,
        argvalues: &'ast [ast::Expr],
    ) {
        let signature = self.single_item_fn_type(sub_signature);
        for argvalue in argvalues {
            if let Some(args) = argvalue.as_tuple_expr()
                && let Some(signature) = self.test_case_fn_type(sub_signature)
            {
                self.check_pytest_fn_call(
                    sub_signature.test_name(),
                    signature,
                    &Self::multiple_argvalue_arguments(args),
                );
            } else {
                self.check_pytest_fn_call(
                    sub_signature.test_name(),
                    signature,
                    &Self::single_argvalue_argument(argvalue),
                );
            }
        }
    }

    fn check_argvalue_test_cases(
        &mut self,
        sub_signature: &SubSignature<'db, 'ast>,
        argvalues: &'ast ast::Expr,
    ) {
        let signature = self.test_cases_fn_type(sub_signature);
        self.check_pytest_fn_call(
            sub_signature.test_name(),
            signature,
            &Self::single_argvalue_argument(argvalues),
        );
    }

    fn single_argvalue_argument(argvalue: &'ast ast::Expr) -> ast::Arguments {
        ast::Arguments {
            range: argvalue.range(),
            node_index: AtomicNodeIndex::default(),
            args: Box::new([argvalue.to_owned()]),
            keywords: Default::default(),
        }
    }

    fn multiple_argvalue_arguments(argvalues: &'ast ast::ExprTuple) -> ast::Arguments {
        ast::Arguments {
            range: argvalues.range(),
            node_index: AtomicNodeIndex::default(),
            args: argvalues.elts.clone().into_boxed_slice(),
            keywords: Default::default(),
        }
    }

    /// Check the synthetic function call with the default type-checking machinery.
    /// The diagnostic contains relevant information, as well as type errors.
    /// Type variables are included in the context, but they are bound to the original definition.
    /// As a result, they always default to `object`, so are effectively ignored.
    fn check_pytest_fn_call(
        &mut self,
        test_name: &ast::Identifier,
        fn_type: Type<'db>,
        arguments: &ast::Arguments,
    ) {
        let mut call_arguments = CallArguments::from_arguments(arguments, |_, _| unreachable!());
        let mut bindings = self.bindings_for_call(fn_type).match_parameters(
            self.db(),
            self.program_environment(),
            &call_arguments,
        );
        let bindings_result = self.infer_and_check_argument_types(
            ArgumentsIter::from_ast(arguments),
            &mut call_arguments,
            &mut |builder, (_, expr, _tcx)| builder.expression_type(expr),
            &mut bindings,
            TypeContext::default(),
        );
        if bindings_result.is_err() {
            debug_assert_matches!(bindings_result, Err(CallErrorKind::BindingError));
            bindings.report_diagnostics_with_override(
                &self.context,
                arguments.into(),
                &CallDiagnosticOverride {
                    lint: &PYTEST_PARAM_MISMATCHED_TYPE,
                    message: format!("Invalid parameter passed to {test_name}."),
                    info: "",
                    argument_ranges: &arguments
                        .iter_source_order()
                        .map(|arg| arg.range())
                        .collect_vec(),
                },
            );
        }
    }
}
