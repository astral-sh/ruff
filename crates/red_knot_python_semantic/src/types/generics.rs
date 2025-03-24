use ruff_python_ast as ast;

use crate::semantic_index::SemanticIndex;
use crate::types::{declaration_type, KnownInstanceType, Type, TypeVarInstance};
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
                let definition = index.expect_single_definition(node);
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
}
