use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use crate::semantic_index::SemanticIndex;
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::{
    declaration_type, KnownInstanceType, Type, TypeVarBoundOrConstraints, TypeVarInstance,
    UnionBuilder, UnionType,
};
use crate::Db;

/// A list of formal type variables for a generic function, class, or type alias.
///
/// TODO: Handle nested generic contexts better, with actual parent links to the lexically
/// containing context.
#[salsa::interned(debug)]
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
        let variables: Box<[_]> = type_params_node
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
                // assignable, and not subclasses of them, nor a union of them.
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

    pub(crate) fn identity_specialization(self, db: &'db dyn Db) -> Specialization<'db> {
        let types = self
            .variables(db)
            .iter()
            .map(|typevar| Type::TypeVar(*typevar))
            .collect();
        self.specialize(db, types)
    }

    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> Specialization<'db> {
        let types = vec![Type::unknown(); self.variables(db).len()];
        self.specialize(db, types.into())
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
///
/// TODO: Handle nested specializations better, with actual parent links to the specialization of
/// the lexically containing context.
#[salsa::interned(debug)]
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
        let types: Box<[_]> = self
            .types(db)
            .into_iter()
            .map(|ty| ty.apply_specialization(db, other))
            .collect();
        Specialization::new(db, self.generic_context(db), types)
    }

    /// Combines two specializations of the same generic context. If either specialization maps a
    /// typevar to `Type::Unknown`, the other specialization's mapping is used. If both map the
    /// typevar to a known type, those types are unioned together.
    ///
    /// Panics if the two specializations are not for the same generic context.
    pub(crate) fn combine(self, db: &'db dyn Db, other: Self) -> Self {
        let generic_context = self.generic_context(db);
        assert!(other.generic_context(db) == generic_context);
        let types: Box<[_]> = self
            .types(db)
            .into_iter()
            .zip(other.types(db))
            .map(|(self_type, other_type)| match (self_type, other_type) {
                (unknown, known) | (known, unknown) if unknown.is_unknown() => *known,
                _ => UnionType::from_elements(db, [self_type, other_type]),
            })
            .collect();
        Specialization::new(db, self.generic_context(db), types)
    }

    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        let types: Box<[_]> = self.types(db).iter().map(|ty| ty.normalized(db)).collect();
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

/// Performs type inference between parameter annotations and argument types, producing a
/// specialization of a generic function.
pub(crate) struct SpecializationBuilder<'db> {
    db: &'db dyn Db,
    generic_context: GenericContext<'db>,
    types: FxHashMap<TypeVarInstance<'db>, UnionBuilder<'db>>,
}

impl<'db> SpecializationBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db, generic_context: GenericContext<'db>) -> Self {
        Self {
            db,
            generic_context,
            types: FxHashMap::default(),
        }
    }

    pub(crate) fn build(mut self) -> Specialization<'db> {
        let types: Box<[_]> = self
            .generic_context
            .variables(self.db)
            .iter()
            .map(|variable| {
                self.types
                    .remove(variable)
                    .map(UnionBuilder::build)
                    .unwrap_or(variable.default_ty(self.db).unwrap_or(Type::unknown()))
            })
            .collect();
        Specialization::new(self.db, self.generic_context, types)
    }

    fn add_type_mapping(&mut self, typevar: TypeVarInstance<'db>, ty: Type<'db>) {
        let builder = self
            .types
            .entry(typevar)
            .or_insert_with(|| UnionBuilder::new(self.db));
        builder.add_in_place(ty);
    }

    pub(crate) fn infer(&mut self, formal: Type<'db>, actual: Type<'db>) {
        // If the actual type is already assignable to the formal type, then return without adding
        // any new type mappings. (Note that if the formal type contains any typevars, this check
        // will fail, since no non-typevar types are assignable to a typevar.)
        //
        // In particular, this handles a case like
        //
        // ```py
        // def f[T](t: T | None): ...
        //
        // f(None)
        // ```
        //
        // without specializing `T` to `None`.
        if actual.is_assignable_to(self.db, formal) {
            return;
        }

        match (formal, actual) {
            (Type::TypeVar(typevar), _) => self.add_type_mapping(typevar, actual),

            (Type::Tuple(formal_tuple), Type::Tuple(actual_tuple)) => {
                let formal_elements = formal_tuple.elements(self.db);
                let actual_elements = actual_tuple.elements(self.db);
                if formal_elements.len() == actual_elements.len() {
                    for (formal_element, actual_element) in
                        formal_elements.iter().zip(actual_elements)
                    {
                        self.infer(*formal_element, *actual_element);
                    }
                }
            }

            (Type::Union(formal), _) => {
                // TODO: We haven't implemented a full unification solver yet. If typevars appear
                // in multiple union elements, we ideally want to express that _only one_ of them
                // needs to match, and that we should infer the smallest type mapping that allows
                // that.
                //
                // For now, we punt on handling multiple typevar elements. Instead, if _precisely
                // one_ union element _is_ a typevar (not _contains_ a typevar), then we go ahead
                // and add a mapping between that typevar and the actual type. (Note that we've
                // already handled above the case where the actual is assignable to a _non-typevar_
                // union element.)
                let mut typevars = formal.iter(self.db).filter_map(|ty| match ty {
                    Type::TypeVar(typevar) => Some(*typevar),
                    _ => None,
                });
                let typevar = typevars.next();
                let additional_typevars = typevars.next();
                if let (Some(typevar), None) = (typevar, additional_typevars) {
                    self.add_type_mapping(typevar, actual);
                }
            }

            (Type::Intersection(formal), _) => {
                // The actual type must be assignable to every (positive) element of the
                // formal intersection, so we must infer type mappings for each of them. (The
                // actual type must also be disjoint from every negative element of the
                // intersection, but that doesn't help us infer any type mappings.)
                for positive in formal.iter_positive(self.db) {
                    self.infer(positive, actual);
                }
            }

            // TODO: Add more forms that we can structurally induct into: type[C], callables
            _ => {}
        }
    }
}
