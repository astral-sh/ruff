use crate::Db;
use crate::types::{
    GenericContext, InferContext, KnownClass, Parameter, Parameters, Signature, Type, UnionType,
    infer::builder::post_inference::pytest::{
        argnames::{KnownArgnames, SingleArgname},
        pytest_test::CheckablePytestTest,
        request::Request,
    },
    tuple::TupleType,
};
use derive_more::Constructor;
use itertools::Itertools;
use ruff_python_ast::{self as ast};

#[derive(Clone, Copy, Constructor)]
/// The name and type expected in a test.
struct TestParameter<'db, 'ast> {
    name: &'ast ast::Identifier,
    ty: Type<'db>,
}

impl<'db, 'ast> TestParameter<'db, 'ast> {
    fn raw_name(&self) -> ast::name::Name {
        self.name().id().clone()
    }

    fn name(&self) -> &'ast ast::Identifier {
        self.name
    }

    fn ty(&self) -> Type<'db> {
        self.ty
    }
}

impl<'db, 'ast> From<Request<'db, 'ast>> for TestParameter<'db, 'ast> {
    fn from(request: Request<'db, 'ast>) -> Self {
        Self::new(request.name(), request.ty())
    }
}

#[derive(Clone)]
/// The parameters expected to the test in a `PartialSignature`.
enum TestParameters<'db, 'ast> {
    // Edge case with one argname as a string.
    Single(TestParameter<'db, 'ast>),
    Multiple(Vec<TestParameter<'db, 'ast>>),
}

#[derive(Clone)]
/// The parameters expected to the test in a `PartialSignature`.
pub(crate) struct PartialSignature<'db, 'ast> {
    test_name: &'ast ast::Identifier,
    generic_context: Option<GenericContext<'db>>,
    parameters: TestParameters<'db, 'ast>,
}

impl<'db, 'ast> PartialSignature<'db, 'ast> {
    pub(crate) fn single(test_name: &'ast ast::Identifier, request: Request<'db, 'ast>) -> Self {
        Self {
            test_name,
            generic_context: request.generic_context(),
            parameters: TestParameters::Single(TestParameter::from(request)),
        }
    }

    pub(crate) fn multiple(
        db: &'db dyn Db,
        test_name: &'ast ast::Identifier,
        requests: Vec<Request<'db, 'ast>>,
    ) -> Self {
        Self {
            test_name,
            generic_context: requests
                .iter()
                .map(Request::generic_context)
                .reduce(|left, right| GenericContext::merge_optional(db, left, right))
                .flatten(),
            parameters: TestParameters::Multiple(requests.into_iter().map_into().collect_vec()),
        }
    }

    pub(crate) fn test_name(&self) -> &'ast ast::Identifier {
        self.test_name
    }

    pub(crate) fn generic_context(&self) -> Option<GenericContext<'db>> {
        self.generic_context
    }
}

impl<'db, 'ast> PartialSignature<'db, 'ast> {
    /// The function type for checking a single item at once.
    pub(crate) fn single_item_fn_type(&self, context: &InferContext<'db, 'ast>) -> Type<'db> {
        self.single_parameter_fn(context, self.item_parameter(context))
    }

    /// The function type for checking all the test cases at once.
    pub(crate) fn test_cases_fn_type(&self, context: &InferContext<'db, 'ast>) -> Type<'db> {
        self.single_parameter_fn(context, self.test_cases_parameter(context))
    }

    /// The function type for checking all the parameters as different arguments.
    /// The test case can only be checked as a function if there are multiple argnames.
    pub(crate) fn test_case_fn_type(&self, context: &InferContext<'db, 'ast>) -> Option<Type<'db>> {
        let parameters = self.test_case_parameters()?;
        Some(self.fn_type_from_parameters(context, parameters))
    }

    fn single_parameter_fn(
        &self,
        context: &InferContext<'db, 'ast>,
        parameter: Parameter<'db>,
    ) -> Type<'db> {
        self.fn_type_from_parameters(context, [parameter])
    }

    fn fn_type_from_parameters(
        &self,
        context: &InferContext<'db, 'ast>,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
    ) -> Type<'db> {
        let db = context.db();
        let env = context.program_environment();
        let parameters = Parameters::standard(parameters);
        let signature =
            Signature::new_generic(self.generic_context(), parameters, Type::none(db, env));
        Type::single_callable(db, signature)
    }

    /// The parameter of the function when each item is checked individually.
    /// If there is one argname, it is named and has that time.
    /// If there are multiple, it is converted to an unnamed tuple.
    fn item_parameter(&self, context: &InferContext<'db, 'ast>) -> Parameter<'db> {
        let parameter = match self.parameters {
            TestParameters::Single(parameter) => {
                Parameter::positional_or_keyword(parameter.raw_name())
            }
            TestParameters::Multiple(_) => Parameter::positional_only(None),
        };
        parameter.with_annotated_type(self.item_type(context))
    }

    /// The parameter of the function when the test case is checked with multiple arguments.
    /// Each argname corresponds to a unique argument.
    fn test_case_parameters(&self) -> Option<Vec<Parameter<'db>>> {
        if let TestParameters::Multiple(parameters) = &self.parameters {
            let parameters = parameters
                .iter()
                .map(|parameter| {
                    Parameter::positional_or_keyword(parameter.raw_name())
                        .with_annotated_type(parameter.ty())
                })
                .collect_vec();
            Some(parameters)
        } else {
            None
        }
    }

    /// The parameter of the function when everything is checked together.
    /// This is an iterable that accepts a tuple of the argnames or one item.
    fn test_cases_parameter(&self, context: &InferContext<'db, 'ast>) -> Parameter<'db> {
        let parameter = Parameter::positional_only(None);
        parameter.with_annotated_type(self.test_cases_type(context))
    }

    fn test_cases_type(&self, context: &InferContext<'db, 'ast>) -> Type<'db> {
        KnownClass::Iterable.to_specialized_instance(
            context.db(),
            context.program_environment(),
            &[self.item_type(context)],
        )
    }

    fn item_type(&self, context: &InferContext<'db, 'ast>) -> Type<'db> {
        match &self.parameters {
            TestParameters::Single(parameter) => Self::single_item_type(context, parameter),
            TestParameters::Multiple(parameters) => Self::tuple_item_type(context, parameters),
        }
    }

    fn single_item_type(
        context: &InferContext<'db, 'ast>,
        parameter: &TestParameter<'db, 'ast>,
    ) -> Type<'db> {
        Self::union_with_param_set(context, parameter.ty())
    }

    fn tuple_item_type(
        context: &InferContext<'db, 'ast>,
        parameter: &[TestParameter<'db, 'ast>],
    ) -> Type<'db> {
        Self::union_with_param_set(
            context,
            Type::tuple(TupleType::heterogeneous(
                context.db(),
                context.program_environment(),
                parameter.iter().map(TestParameter::ty),
            )),
        )
    }

    /// Union of a type with the `ParameterSet`.
    /// `ParameterSet` is universally accepted by tests.
    fn union_with_param_set(
        context: &InferContext<'db, 'ast>,
        ty: impl Into<Type<'db>>,
    ) -> Type<'db> {
        let db = context.db();
        let env = context.program_environment();
        UnionType::from_two_elements(
            db,
            env,
            ty.into(),
            KnownClass::PytestParameterSet.to_instance(db, env),
        )
    }
}

impl<'db, 'ast> CheckablePytestTest<'db, 'ast> {
    pub(crate) fn partial_signature(
        &self,
        db: &'db dyn Db,
        argnames: &KnownArgnames,
    ) -> Option<PartialSignature<'db, 'ast>> {
        match argnames {
            KnownArgnames::Single(argname) => self.single_item_test_signature(argname),
            KnownArgnames::Multiple(argnames) => {
                self.multiple_items_test_signature(db, argnames.iter())
            }
        }
    }

    fn single_item_test_signature(
        &self,
        argname: &SingleArgname,
    ) -> Option<PartialSignature<'db, 'ast>> {
        let request = self.request_for(argname.name())?;
        Some(PartialSignature::single(self.name(), request))
    }

    fn multiple_items_test_signature<'a>(
        &self,
        db: &'db dyn Db,
        argnames: impl Iterator<Item = &'a SingleArgname>,
    ) -> Option<PartialSignature<'db, 'ast>> {
        let requests = argnames
            .into_iter()
            .map(|argname| self.request_for(argname.name()))
            .collect::<Option<Vec<_>>>()?;
        Some(PartialSignature::multiple(db, self.name(), requests))
    }

    /// Lookup the request for a given argname.
    fn request_for(&self, argname: &str) -> Option<Request<'db, 'ast>> {
        self.requests()
            .iter()
            .find(|request| request.name() == argname)
            .copied()
    }
}
