use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use crate::semantic_index::SemanticIndex;
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::{
    declaration_type, KnownInstanceType, Type, TypeVarBoundOrConstraints, TypeVarInstance,
    UnionType,
};
use crate::{Db, FxOrderSet};

/// A list of formal type variables for a generic function, class, or type alias.
///
/// TODO: Handle nested generic contexts better, with actual parent links to the lexically
/// containing context.
#[salsa::interned(debug)]
pub struct GenericContext<'db> {
    #[return_ref]
    pub(crate) variables: FxOrderSet<TypeVarInstance<'db>>,
}

impl<'db> GenericContext<'db> {
    /// Creates a generic context from a list of PEP-695 type parameters.
    pub(crate) fn from_type_params(
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        type_params_node: &ast::TypeParams,
    ) -> Self {
        let variables: FxOrderSet<_> = type_params_node
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

    /// Creates a generic context from the legacy `TypeVar`s that appear in a function parameter
    /// list.
    pub(crate) fn from_function_params(
        db: &'db dyn Db,
        parameters: &Parameters<'db>,
        return_type: Option<Type<'db>>,
    ) -> Option<Self> {
        let mut variables = FxOrderSet::default();
        for param in parameters {
            if let Some(ty) = param.annotated_type() {
                ty.find_legacy_typevars(db, &mut variables);
            }
            if let Some(ty) = param.default_type() {
                ty.find_legacy_typevars(db, &mut variables);
            }
        }
        if let Some(ty) = return_type {
            ty.find_legacy_typevars(db, &mut variables);
        }
        if variables.is_empty() {
            return None;
        }
        Some(Self::new(db, variables))
    }

    /// Creates a generic context from the legacy `TypeVar`s that appear in class's base class
    /// list.
    pub(crate) fn from_base_classes(
        db: &'db dyn Db,
        bases: impl Iterator<Item = Type<'db>>,
    ) -> Option<Self> {
        let mut variables = FxOrderSet::default();
        for base in bases {
            base.find_legacy_typevars(db, &mut variables);
        }
        if variables.is_empty() {
            return None;
        }
        Some(Self::new(db, variables))
    }

    pub(crate) fn len(self, db: &'db dyn Db) -> usize {
        self.variables(db).len()
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
        if let Some(default_ty) = typevar.default_ty(db) {
            parameter = parameter.with_default_type(default_ty);
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

    pub(crate) fn is_subset_of(self, db: &'db dyn Db, other: GenericContext<'db>) -> bool {
        self.variables(db).is_subset(other.variables(db))
    }

    /// Creates a specialization of this generic context. Panics if the length of `types` does not
    /// match the number of typevars in the generic context.
    pub(crate) fn specialize(
        self,
        db: &'db dyn Db,
        types: Box<[Type<'db>]>,
    ) -> Specialization<'db> {
        assert!(self.variables(db).len() == types.len());
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
        // TODO special-casing Unknown to mean "no mapping" is not right here, and can give
        // confusing/wrong results in cases where there was a mapping found for a typevar, and it
        // was of type Unknown. We should probably add a bitset or similar to Specialization that
        // explicitly tells us which typevars are mapped.
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
        let index = self
            .generic_context(db)
            .variables(db)
            .get_index_of(&typevar)?;
        Some(self.types(db)[index])
    }

    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, other: Specialization<'db>) -> bool {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return false;
        }

        for ((_typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            if matches!(self_type, Type::Dynamic(_)) || matches!(other_type, Type::Dynamic(_)) {
                return false;
            }

            // TODO: We currently treat all typevars as invariant. Once we track the actual
            // variance of each typevar, these checks should change:
            //   - covariant: verify that self_type <: other_type
            //   - contravariant: verify that other_type <: self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make subtyping false
            if !self_type.is_equivalent_to(db, *other_type) {
                return false;
            }
        }

        true
    }

    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Specialization<'db>) -> bool {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return false;
        }

        for ((_typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            if matches!(self_type, Type::Dynamic(_)) || matches!(other_type, Type::Dynamic(_)) {
                return false;
            }

            // TODO: We currently treat all typevars as invariant. Once we track the actual
            // variance of each typevar, these checks should change:
            //   - covariant: verify that self_type == other_type
            //   - contravariant: verify that other_type == self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make equivalence false
            if !self_type.is_equivalent_to(db, *other_type) {
                return false;
            }
        }

        true
    }

    pub(crate) fn is_assignable_to(self, db: &'db dyn Db, other: Specialization<'db>) -> bool {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return false;
        }

        for ((_typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            if matches!(self_type, Type::Dynamic(_)) || matches!(other_type, Type::Dynamic(_)) {
                continue;
            }

            // TODO: We currently treat all typevars as invariant. Once we track the actual
            // variance of each typevar, these checks should change:
            //   - covariant: verify that self_type <: other_type
            //   - contravariant: verify that other_type <: self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make assignability false
            if !self_type.is_gradual_equivalent_to(db, *other_type) {
                return false;
            }
        }

        true
    }

    pub(crate) fn is_gradual_equivalent_to(
        self,
        db: &'db dyn Db,
        other: Specialization<'db>,
    ) -> bool {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return false;
        }

        for ((_typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            // TODO: We currently treat all typevars as invariant. Once we track the actual
            // variance of each typevar, these checks should change:
            //   - covariant: verify that self_type == other_type
            //   - contravariant: verify that other_type == self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make equivalence false
            if !self_type.is_gradual_equivalent_to(db, *other_type) {
                return false;
            }
        }

        true
    }

    pub(crate) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        typevars: &mut FxOrderSet<TypeVarInstance<'db>>,
    ) {
        for ty in self.types(db) {
            ty.find_legacy_typevars(db, typevars);
        }
    }
}

/// Performs type inference between parameter annotations and argument types, producing a
/// specialization of a generic function.
pub(crate) struct SpecializationBuilder<'db> {
    db: &'db dyn Db,
    types: FxHashMap<TypeVarInstance<'db>, Type<'db>>,
}

impl<'db> SpecializationBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            types: FxHashMap::default(),
        }
    }

    pub(crate) fn build(&mut self, generic_context: GenericContext<'db>) -> Specialization<'db> {
        let types: Box<[_]> = generic_context
            .variables(self.db)
            .iter()
            .map(|variable| {
                self.types
                    .get(variable)
                    .copied()
                    .unwrap_or(variable.default_ty(self.db).unwrap_or(Type::unknown()))
            })
            .collect();
        Specialization::new(self.db, generic_context, types)
    }

    fn add_type_mapping(&mut self, typevar: TypeVarInstance<'db>, ty: Type<'db>) {
        self.types
            .entry(typevar)
            .and_modify(|existing| {
                *existing = UnionType::from_elements(self.db, [*existing, ty]);
            })
            .or_insert(ty);
    }

    pub(crate) fn infer(
        &mut self,
        formal: Type<'db>,
        actual: Type<'db>,
    ) -> Result<(), SpecializationError<'db>> {
        // If the actual type is a subtype of the formal type, then return without adding any new
        // type mappings. (Note that if the formal type contains any typevars, this check will
        // fail, since no non-typevar types are assignable to a typevar. Also note that we are
        // checking _subtyping_, not _assignability_, so that we do specialize typevars to dynamic
        // argument types; and we have a special case for `Never`, which is a subtype of all types,
        // but which we also do want as a specialization candidate.)
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
        if !actual.is_never() && actual.is_subtype_of(self.db, formal) {
            return Ok(());
        }

        match (formal, actual) {
            (Type::TypeVar(typevar), _) => match typevar.bound_or_constraints(self.db) {
                Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                    if !actual.is_assignable_to(self.db, bound) {
                        return Err(SpecializationError::MismatchedBound {
                            typevar,
                            argument: actual,
                        });
                    }
                    self.add_type_mapping(typevar, actual);
                }
                Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                    for constraint in constraints.iter(self.db) {
                        if actual.is_assignable_to(self.db, *constraint) {
                            self.add_type_mapping(typevar, *constraint);
                            return Ok(());
                        }
                    }
                    return Err(SpecializationError::MismatchedConstraint {
                        typevar,
                        argument: actual,
                    });
                }
                _ => {
                    self.add_type_mapping(typevar, actual);
                }
            },

            (Type::Tuple(formal_tuple), Type::Tuple(actual_tuple)) => {
                let formal_elements = formal_tuple.elements(self.db);
                let actual_elements = actual_tuple.elements(self.db);
                if formal_elements.len() == actual_elements.len() {
                    for (formal_element, actual_element) in
                        formal_elements.iter().zip(actual_elements)
                    {
                        self.infer(*formal_element, *actual_element)?;
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
                    self.infer(positive, actual)?;
                }
            }

            // TODO: Add more forms that we can structurally induct into: type[C], callables
            _ => {}
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SpecializationError<'db> {
    MismatchedBound {
        typevar: TypeVarInstance<'db>,
        argument: Type<'db>,
    },
    MismatchedConstraint {
        typevar: TypeVarInstance<'db>,
        argument: Type<'db>,
    },
}

impl<'db> SpecializationError<'db> {
    pub(crate) fn typevar(&self) -> TypeVarInstance<'db> {
        match self {
            Self::MismatchedBound { typevar, .. } => *typevar,
            Self::MismatchedConstraint { typevar, .. } => *typevar,
        }
    }

    pub(crate) fn argument_type(&self) -> Type<'db> {
        match self {
            Self::MismatchedBound { argument, .. } => *argument,
            Self::MismatchedConstraint { argument, .. } => *argument,
        }
    }
}
