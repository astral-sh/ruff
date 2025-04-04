use ruff_python_ast as ast;

use crate::semantic_index::SemanticIndex;
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::{
    declaration_type, KnownInstanceType, Type, TypeVarBoundOrConstraints, TypeVarInstance,
    UnionType,
};
use crate::Db;

/// A list of formal type variables for a generic function, class, or type alias.
#[salsa::tracked(debug)]
pub struct GenericContext<'db> {
    #[return_ref]
    pub(crate) variables: Box<[TypeVarInstance<'db>]>,
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
        Self::new(db, variables)
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

    pub(crate) fn signature(self, db: &'db dyn Db) -> Signature<'db> {
        let parameters = Parameters::new(
            self.variables(db)
                .iter()
                .map(|typevar| Self::parameter_from_typevar(db, *typevar)),
        );
        Signature::new(parameters, None)
    }

    fn parameter_from_typevar(db: &'db dyn Db, typevar: TypeVarInstance<'db>) -> Parameter<'db> {
        let mut parameter = Parameter::positional_only(Some(typevar.name(db).clone()));
        match typevar.bound_or_constraints(db) {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                // TODO: This should be a type form.
                parameter = parameter.with_annotated_type(bound);
            }
            Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                // TODO: This should be a new type variant where only these exact types are
                // assignable, and not subclasses of them.
                parameter = parameter
                    .with_annotated_type(UnionType::from_elements(db, constraints.iter(db)));
            }
            None => {}
        }
        parameter
    }

    pub(crate) fn default_specialization(self, db: &'db dyn Db) -> Specialization<'db> {
        let types = self
            .variables(db)
            .iter()
            .map(|typevar| typevar.default_ty(db).unwrap_or(Type::unknown()))
            .collect();
        self.specialize(db, types)
    }

    pub(crate) fn specialize(
        self,
        db: &'db dyn Db,
        types: Box<[Type<'db>]>,
    ) -> Specialization<'db> {
        Specialization::new(db, self, types)
    }
}

/// An assignment of a specific type to each type variable in a generic scope.
#[salsa::tracked(debug)]
pub struct Specialization<'db> {
    pub(crate) generic_context: GenericContext<'db>,
    #[return_ref]
    pub(crate) types: Box<[Type<'db>]>,
}

impl<'db> Specialization<'db> {
    /// Applies a specialization to this specialization. This is used, for instance, when a generic
    /// class inherits from a generic alias:
    ///
    /// ```py
    /// class A[T]: ...
    /// class B[U](A[U]): ...
    /// ```
    ///
    /// `B` is a generic class, whose MRO includes the generic alias `A[U]`, which specializes `A`
    /// with the specialization `{T: U}`. If `B` is specialized to `B[int]`, with specialization
    /// `{U: int}`, we can apply the second specialization to the first, resulting in `T: int`.
    /// That lets us produce the generic alias `A[int]`, which is the corresponding entry in the
    /// MRO of `B[int]`.
    pub(crate) fn apply_specialization(self, db: &'db dyn Db, other: Specialization<'db>) -> Self {
        let types = self
            .types(db)
            .into_iter()
            .map(|ty| ty.apply_specialization(db, other))
            .collect();
        Specialization::new(db, self.generic_context(db), types)
    }

    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        let types = self.types(db).iter().map(|ty| ty.normalized(db)).collect();
        Self::new(db, self.generic_context(db), types)
    }

    /// Returns the type that a typevar is specialized to, or None if the typevar isn't part of
    /// this specialization.
    pub(crate) fn get(self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) -> Option<Type<'db>> {
        self.generic_context(db)
            .variables(db)
            .into_iter()
            .zip(self.types(db))
            .find(|(var, _)| **var == typevar)
            .map(|(_, ty)| *ty)
    }
}
