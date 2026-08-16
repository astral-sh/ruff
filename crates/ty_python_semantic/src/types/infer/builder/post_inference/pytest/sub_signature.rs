use crate::types::{
    Parameter, Parameters, Signature, Type,
    infer::{
        TypeInferenceBuilder,
        builder::post_inference::pytest::{
            argnames::{KnownArgnames, SingleArgname},
            pytest_test::CheckablePytestTest,
            request::Request,
        },
    },
    tuple::TupleType,
};
use derive_more::Constructor;
use itertools::Itertools;
use ruff_python_ast::{self as ast};

#[derive(Clone, Copy, Constructor)]
struct TestParameter<'db, 'ast> {
    name: &'ast ast::Identifier,
    ty: Type<'db>,
}

impl<'db, 'ast> TestParameter<'db, 'ast> {
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
enum TestParameters<'db, 'ast> {
    Single(TestParameter<'db, 'ast>),
    Multiple(Vec<TestParameter<'db, 'ast>>),
}

#[derive(Clone)]
pub(crate) struct SubSignature<'db, 'ast> {
    test_name: &'ast ast::Identifier,
    parameters: TestParameters<'db, 'ast>,
}

impl<'db, 'ast> SubSignature<'db, 'ast> {
    pub(crate) fn single(test_name: &'ast ast::Identifier, request: Request<'db, 'ast>) -> Self {
        Self {
            test_name,
            parameters: TestParameters::Single(TestParameter::from(request)),
        }
    }

    pub(crate) fn multiple(
        test_name: &'ast ast::Identifier,
        requests: Vec<Request<'db, 'ast>>,
    ) -> Self {
        Self {
            test_name,
            parameters: TestParameters::Multiple(requests.into_iter().map_into().collect_vec()),
        }
    }

    pub(crate) fn test_name(&self) -> &'ast ast::Identifier {
        self.test_name
    }
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(crate) fn single_item_fn_type(&self, signature: &SubSignature<'db, 'ast>) -> Type<'db> {
        let db = self.db();
        let parameters = [self.item_parameter(signature)];
        let parameters = Parameters::standard(parameters);
        let signature = Signature::new(parameters, Type::none(db, self.program_environment()));
        Type::single_callable(db, signature)
    }

    fn item_parameter(&self, signature: &SubSignature<'db, 'ast>) -> Parameter<'db> {
        let parameter = match signature.parameters {
            TestParameters::Single(parameter) => {
                Parameter::positional_or_keyword(parameter.name().id().clone())
            }
            TestParameters::Multiple(_) => Parameter::positional_only(None),
        };
        parameter.with_annotated_type(self.item_type(signature))
    }

    fn item_type(&self, signature: &SubSignature<'db, 'ast>) -> Type<'db> {
        match &signature.parameters {
            TestParameters::Single(parameter) => self.single_item_type(parameter),
            TestParameters::Multiple(parameters) => self.tuple_item_type(parameters),
        }
    }

    fn single_item_type(&self, parameter: &TestParameter<'db, 'ast>) -> Type<'db> {
        parameter.ty()
    }

    fn tuple_item_type(&self, parameter: &[TestParameter<'db, 'ast>]) -> Type<'db> {
        Type::tuple(TupleType::heterogeneous(
            self.db(),
            self.program_environment(),
            parameter.iter().map(TestParameter::ty),
        ))
    }
}

impl<'db, 'ast> CheckablePytestTest<'db, 'ast> {
    pub(crate) fn sub_signature(
        &self,
        argnames: &KnownArgnames,
    ) -> Option<SubSignature<'db, 'ast>> {
        match argnames {
            KnownArgnames::Single(argname) => self.single_item_test_signature(argname),
            KnownArgnames::Multiple(argnames) => {
                self.multiple_items_test_signature(argnames.iter())
            }
        }
    }

    fn single_item_test_signature(
        &self,
        argname: &SingleArgname,
    ) -> Option<SubSignature<'db, 'ast>> {
        let request = self.request_for(argname.name())?;
        Some(SubSignature::single(self.name(), request))
    }

    fn multiple_items_test_signature<'a>(
        &self,
        argnames: impl Iterator<Item = &'a SingleArgname>,
    ) -> Option<SubSignature<'db, 'ast>> {
        let requests = argnames
            .into_iter()
            .map(|argname| self.request_for(argname.name()))
            .collect::<Option<Vec<_>>>()?;
        Some(SubSignature::multiple(self.name(), requests))
    }

    fn request_for(&self, name: &str) -> Option<Request<'db, 'ast>> {
        self.requests()
            .iter()
            .find(|request| request.name() == name)
            .copied()
    }
}
