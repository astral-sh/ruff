use ruff_python_ast as ast;

use crate::semantic_index::SemanticIndex;
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::{
    declaration_type, KnownInstanceType, Type, TypeVarBoundOrConstraints, TypeVarInstance,
    UnionType,
};
use crate::Db;

/// A list of formal type variables for a generic function, class, or type alias.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct GenericContext<'db> {
    variables: Box<[TypeVarInstance<'db>]>,
}

impl<'db> GenericContext<'db> {
    pub(crate) fn from_type_params(
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        type_params_node: &ast::TypeParams,
    ) -> Self {
        let variables = type_params_node
            .iter()
            .filter_map(|type_param| Self::variable_from_type_param(db, index, type_param))
            .collect();
        Self { variables }
    }

    fn variable_from_type_param(
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        type_param_node: &ast::TypeParam,
    ) -> Option<TypeVarInstance<'db>> {
        match type_param_node {
            ast::TypeParam::TypeVar(node) => {
                let definition = index.definition(node);
                let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
                    declaration_type(db, definition).inner_type()
                else {
                    panic!("typevar should be inferred as a TypeVarInstance");
                };
                Some(typevar)
            }
            // TODO: Support these!
            ast::TypeParam::ParamSpec(_) => None,
            ast::TypeParam::TypeVarTuple(_) => None,
        }
    }

    pub(crate) fn signature(&self, db: &'db dyn Db, class: Type<'db>) -> Signature<'db> {
        let parameters = Parameters::new(
            std::iter::once(Parameter::positional_only(None).with_annotated_type(class)).chain(
                self.variables
                    .iter()
                    .map(|typevar| Self::parameter_from_typevar(db, typevar)),
            ),
        );
        Signature::new(parameters, None)
    }

    fn parameter_from_typevar(db: &'db dyn Db, typevar: &TypeVarInstance<'db>) -> Parameter<'db> {
        let mut parameter = Parameter::positional_only(Some(typevar.name(db).clone()));
        match typevar.bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                parameter = parameter.with_annotated_type(bound);
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                parameter = parameter
                    .with_annotated_type(UnionType::from_elements(db, constraints.iter(db)));
            }
            None => {}
        }
        parameter
    }
}
