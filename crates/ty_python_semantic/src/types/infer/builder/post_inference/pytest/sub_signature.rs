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
};
use ruff_python_ast::{self as ast};

#[derive(Clone, Copy)]
pub(crate) struct SubSignature<'db, 'ast> {
    test_name: &'ast ast::Identifier,
    name: &'ast ast::Identifier,
    ty: Type<'db>,
}

impl<'db, 'ast> SubSignature<'db, 'ast> {
    pub(crate) fn test_name(&self) -> &'ast ast::Identifier {
        self.test_name
    }
}

impl<'db, 'ast> TypeInferenceBuilder<'db, 'ast> {
    pub(crate) fn single_item_fn_type(&self, signature: SubSignature<'db, 'ast>) -> Type<'db> {
        let db = self.db();
        let parameters = [
            Parameter::positional_or_keyword(signature.name.id().clone())
                .with_annotated_type(signature.ty),
        ];
        let parameters = Parameters::standard(parameters);
        let signature = Signature::new(parameters, Type::none(db, self.program_environment()));
        Type::single_callable(db, signature)
    }
}

impl<'db, 'ast> CheckablePytestTest<'db, 'ast> {
    pub(crate) fn sub_signature(
        &self,
        argnames: &KnownArgnames,
    ) -> Option<SubSignature<'db, 'ast>> {
        match argnames {
            KnownArgnames::Single(argname) => self.single_item_test_signature(argname),
            KnownArgnames::Multiple(_) => {
                todo!()
            }
        }
    }

    fn single_item_test_signature(
        &self,
        argname: &SingleArgname,
    ) -> Option<SubSignature<'db, 'ast>> {
        if let Some(request) = self.request_for(argname.name()) {
            Some(SubSignature {
                test_name: self.name(),
                name: request.name(),
                ty: request.ty(),
            })
        } else {
            None
        }
    }

    fn request_for(&self, name: &str) -> Option<Request<'db, 'ast>> {
        self.requests()
            .iter()
            .find(|request| request.name() == name)
            .copied()
    }
}
