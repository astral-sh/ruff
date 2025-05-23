use std::borrow::Cow;

use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use crate::semantic_index::SemanticIndex;
use crate::types::class::ClassType;
use crate::types::class_base::ClassBase;
use crate::types::instance::{NominalInstanceType, Protocol, ProtocolInstanceType};
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::{
    KnownInstanceType, Type, TypeMapping, TypeVarBoundOrConstraints, TypeVarInstance,
    TypeVarVariance, UnionType, declaration_type, todo_type,
};
use crate::{Db, FxOrderSet};

/// A list of formal type variables for a generic function, class, or type alias.
///
/// TODO: Handle nested generic contexts better, with actual parent links to the lexically
/// containing context.
///
/// # Ordering
/// Ordering is based on the context's salsa-assigned id and not on its values.
/// The id may change between runs, or when the context was garbage collected and recreated.
#[salsa::interned(debug)]
#[derive(PartialOrd, Ord)]
pub struct GenericContext<'db> {
    #[returns(ref)]
    pub(crate) variables: FxOrderSet<TypeVarInstance<'db>>,
    pub(crate) origin: GenericContextOrigin,
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
        Self::new(db, variables, GenericContextOrigin::TypeParameterList)
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
                    return None;
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
        Some(Self::new(
            db,
            variables,
            GenericContextOrigin::LegacyGenericFunction,
        ))
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
        Some(Self::new(db, variables, GenericContextOrigin::Inherited))
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
        self.specialize_partial(db, &vec![None; self.variables(db).len()])
    }

    #[allow(unused_variables)] // Only unused in release builds
    pub(crate) fn todo_specialization(
        self,
        db: &'db dyn Db,
        todo: &'static str,
    ) -> Specialization<'db> {
        let types = self
            .variables(db)
            .iter()
            .map(|typevar| typevar.default_ty(db).unwrap_or(todo_type!(todo)))
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
    /// match the number of typevars in the generic context. You must provide a specific type for
    /// each typevar; no defaults are used. (Use [`specialize_partial`](Self::specialize_partial)
    /// if you might not have types for every typevar.)
    pub(crate) fn specialize(
        self,
        db: &'db dyn Db,
        types: Box<[Type<'db>]>,
    ) -> Specialization<'db> {
        assert!(self.variables(db).len() == types.len());
        Specialization::new(db, self, types)
    }

    /// Creates a specialization of this generic context. Panics if the length of `types` does not
    /// match the number of typevars in the generic context. If any provided type is `None`, we
    /// will use the corresponding typevar's default type.
    pub(crate) fn specialize_partial(
        self,
        db: &'db dyn Db,
        types: &[Option<Type<'db>>],
    ) -> Specialization<'db> {
        let variables = self.variables(db);
        assert!(variables.len() == types.len());

        // Typevars can have other typevars as their default values, e.g.
        //
        // ```py
        // class C[T, U = T]: ...
        // ```
        //
        // If there is a mapping for `T`, we want to map `U` to that type, not to `T`. To handle
        // this, we repeatedly apply the specialization to itself, until we reach a fixed point.
        let mut expanded = vec![Type::unknown(); types.len()];
        for (idx, (ty, typevar)) in types.iter().zip(variables).enumerate() {
            if let Some(ty) = ty {
                expanded[idx] = *ty;
                continue;
            }

            let Some(default) = typevar.default_ty(db) else {
                continue;
            };

            // Typevars are only allowed to refer to _earlier_ typevars in their defaults. (This is
            // statically enforced for PEP-695 contexts, and is explicitly called out as a
            // requirement for legacy contexts.)
            let partial = PartialSpecialization {
                generic_context: self,
                types: Cow::Borrowed(&expanded[0..idx]),
            };
            let default =
                default.apply_type_mapping(db, &TypeMapping::PartialSpecialization(partial));
            expanded[idx] = default;
        }

        Specialization::new(db, self, expanded.into_boxed_slice())
    }

    pub(crate) fn normalized(self, db: &'db dyn Db) -> Self {
        let variables: FxOrderSet<_> = self
            .variables(db)
            .iter()
            .map(|ty| ty.normalized(db))
            .collect();
        Self::new(db, variables, self.origin(db))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum GenericContextOrigin {
    LegacyBase(LegacyGenericBase),
    Inherited,
    LegacyGenericFunction,
    TypeParameterList,
}

impl GenericContextOrigin {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyBase(base) => base.as_str(),
            Self::Inherited => "inherited",
            Self::LegacyGenericFunction => "legacy generic function",
            Self::TypeParameterList => "type parameter list",
        }
    }
}

impl std::fmt::Display for GenericContextOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum LegacyGenericBase {
    Generic,
    Protocol,
}

impl LegacyGenericBase {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "`typing.Generic`",
            Self::Protocol => "subscripted `typing.Protocol`",
        }
    }
}

impl std::fmt::Display for LegacyGenericBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<LegacyGenericBase> for GenericContextOrigin {
    fn from(base: LegacyGenericBase) -> Self {
        Self::LegacyBase(base)
    }
}

/// An assignment of a specific type to each type variable in a generic scope.
///
/// TODO: Handle nested specializations better, with actual parent links to the specialization of
/// the lexically containing context.
#[salsa::interned(debug)]
pub struct Specialization<'db> {
    pub(crate) generic_context: GenericContext<'db>,
    #[returns(deref)]
    pub(crate) types: Box<[Type<'db>]>,
}

impl<'db> Specialization<'db> {
    /// Returns the type that a typevar is mapped to, or None if the typevar isn't part of this
    /// mapping.
    pub(crate) fn get(self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) -> Option<Type<'db>> {
        let index = self
            .generic_context(db)
            .variables(db)
            .get_index_of(&typevar)?;
        self.types(db).get(index).copied()
    }

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
        self.apply_type_mapping(db, &TypeMapping::Specialization(other))
    }

    pub(crate) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        let types: Box<[_]> = self
            .types(db)
            .iter()
            .map(|ty| ty.apply_type_mapping(db, type_mapping))
            .collect();
        Specialization::new(db, self.generic_context(db), types)
    }

    /// Applies an optional specialization to this specialization.
    pub(crate) fn apply_optional_specialization(
        self,
        db: &'db dyn Db,
        other: Option<Specialization<'db>>,
    ) -> Self {
        if let Some(other) = other {
            self.apply_specialization(db, other)
        } else {
            self
        }
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
            .iter()
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

    pub(crate) fn is_subtype_of(self, db: &'db dyn Db, other: Specialization<'db>) -> bool {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return false;
        }

        for ((typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            if matches!(self_type, Type::Dynamic(_)) || matches!(other_type, Type::Dynamic(_)) {
                return false;
            }

            // Subtyping of each type in the specialization depends on the variance of the
            // corresponding typevar:
            //   - covariant: verify that self_type <: other_type
            //   - contravariant: verify that other_type <: self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make subtyping false
            let compatible = match typevar.variance(db) {
                TypeVarVariance::Invariant => self_type.is_equivalent_to(db, *other_type),
                TypeVarVariance::Covariant => self_type.is_subtype_of(db, *other_type),
                TypeVarVariance::Contravariant => other_type.is_subtype_of(db, *self_type),
                TypeVarVariance::Bivariant => true,
            };
            if !compatible {
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

        for ((typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            if matches!(self_type, Type::Dynamic(_)) || matches!(other_type, Type::Dynamic(_)) {
                return false;
            }

            // Equivalence of each type in the specialization depends on the variance of the
            // corresponding typevar:
            //   - covariant: verify that self_type == other_type
            //   - contravariant: verify that other_type == self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make equivalence false
            let compatible = match typevar.variance(db) {
                TypeVarVariance::Invariant
                | TypeVarVariance::Covariant
                | TypeVarVariance::Contravariant => self_type.is_equivalent_to(db, *other_type),
                TypeVarVariance::Bivariant => true,
            };
            if !compatible {
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

        for ((typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            if matches!(self_type, Type::Dynamic(_)) || matches!(other_type, Type::Dynamic(_)) {
                continue;
            }

            // Assignability of each type in the specialization depends on the variance of the
            // corresponding typevar:
            //   - covariant: verify that self_type <: other_type
            //   - contravariant: verify that other_type <: self_type
            //   - invariant: verify that self_type <: other_type AND other_type <: self_type
            //   - bivariant: skip, can't make assignability false
            let compatible = match typevar.variance(db) {
                TypeVarVariance::Invariant => {
                    self_type.is_assignable_to(db, *other_type)
                        && other_type.is_assignable_to(db, *self_type)
                }
                TypeVarVariance::Covariant => self_type.is_assignable_to(db, *other_type),
                TypeVarVariance::Contravariant => other_type.is_assignable_to(db, *self_type),
                TypeVarVariance::Bivariant => true,
            };
            if !compatible {
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

        for ((typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            // Equivalence of each type in the specialization depends on the variance of the
            // corresponding typevar:
            //   - covariant: verify that self_type == other_type
            //   - contravariant: verify that other_type == self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make equivalence false
            let compatible = match typevar.variance(db) {
                TypeVarVariance::Invariant
                | TypeVarVariance::Covariant
                | TypeVarVariance::Contravariant => {
                    self_type.is_gradual_equivalent_to(db, *other_type)
                }
                TypeVarVariance::Bivariant => true,
            };
            if !compatible {
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

/// A mapping between type variables and types.
///
/// You will usually use [`Specialization`] instead of this type. This type is used when we need to
/// substitute types for type variables before we have fully constructed a [`Specialization`].
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PartialSpecialization<'a, 'db> {
    generic_context: GenericContext<'db>,
    types: Cow<'a, [Type<'db>]>,
}

impl<'db> PartialSpecialization<'_, 'db> {
    /// Returns the type that a typevar is mapped to, or None if the typevar isn't part of this
    /// mapping.
    pub(crate) fn get(&self, db: &'db dyn Db, typevar: TypeVarInstance<'db>) -> Option<Type<'db>> {
        let index = self.generic_context.variables(db).get_index_of(&typevar)?;
        self.types.get(index).copied()
    }

    pub(crate) fn to_owned(&self) -> PartialSpecialization<'db, 'db> {
        PartialSpecialization {
            generic_context: self.generic_context,
            types: Cow::from(self.types.clone().into_owned()),
        }
    }

    pub(crate) fn normalized(&self, db: &'db dyn Db) -> PartialSpecialization<'db, 'db> {
        let generic_context = self.generic_context.normalized(db);
        let types: Cow<_> = self.types.iter().map(|ty| ty.normalized(db)).collect();

        PartialSpecialization {
            generic_context,
            types,
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
        if !matches!(formal, Type::ProtocolInstance(_))
            && !actual.is_never()
            && actual.is_subtype_of(self.db, formal)
        {
            return Ok(());
        }

        match (formal, actual) {
            (Type::TypeVar(typevar), ty) | (ty, Type::TypeVar(typevar)) => {
                match typevar.bound_or_constraints(self.db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        if !ty.is_assignable_to(self.db, bound) {
                            return Err(SpecializationError::MismatchedBound {
                                typevar,
                                argument: ty,
                            });
                        }
                        self.add_type_mapping(typevar, ty);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        for constraint in constraints.iter(self.db) {
                            if ty.is_assignable_to(self.db, *constraint) {
                                self.add_type_mapping(typevar, *constraint);
                                return Ok(());
                            }
                        }
                        return Err(SpecializationError::MismatchedConstraint {
                            typevar,
                            argument: ty,
                        });
                    }
                    _ => {
                        self.add_type_mapping(typevar, ty);
                    }
                }
            }

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

            (
                Type::NominalInstance(NominalInstanceType {
                    class: ClassType::Generic(formal_alias),
                    ..
                })
                // TODO: This will only handle classes that explicit implement a generic protocol
                // by listing it as a base class. To handle classes that implicitly implement a
                // generic protocol, we will need to check the types of the protocol members to be
                // able to infer the specialization of the protocol that the class implements.
                | Type::ProtocolInstance(ProtocolInstanceType {
                    inner: Protocol::FromClass(ClassType::Generic(formal_alias)),
                    ..
                }),
                Type::NominalInstance(NominalInstanceType {
                    class: actual_class,
                    ..
                }),
            ) => {
                let formal_origin = formal_alias.origin(self.db);
                for base in actual_class.iter_mro(self.db) {
                    let ClassBase::Class(ClassType::Generic(base_alias)) = base else {
                        continue;
                    };
                    if formal_origin != base_alias.origin(self.db) {
                        continue;
                    }
                    let formal_specialization = formal_alias.specialization(self.db).types(self.db);
                    let base_specialization = base_alias.specialization(self.db).types(self.db);
                    for (formal_ty, base_ty) in
                        formal_specialization.iter().zip(base_specialization)
                    {
                        self.infer(*formal_ty, *base_ty)?;
                    }
                    return Ok(());
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
