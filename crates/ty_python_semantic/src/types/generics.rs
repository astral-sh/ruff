use std::borrow::Cow;

use ruff_db::parsed::ParsedModuleRef;
use ruff_python_ast as ast;
use rustc_hash::FxHashMap;

use crate::semantic_index::SemanticIndex;
use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::{FileScopeId, NodeWithScopeKind};
use crate::types::class::ClassType;
use crate::types::class_base::ClassBase;
use crate::types::constraints::Constraints;
use crate::types::infer::infer_definition_types;
use crate::types::instance::{Protocol, ProtocolInstanceType};
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::tuple::{TupleSpec, TupleType, walk_tuple_type};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarInstance, HasRelationToVisitor, KnownClass,
    KnownInstanceType, NormalizedVisitor, Type, TypeMapping, TypeRelation,
    TypeVarBoundOrConstraints, TypeVarInstance, TypeVarVariance, UnionType, binding_type,
    declaration_type,
};
use crate::{Db, FxOrderSet};

/// Returns an iterator of any generic context introduced by the given scope or any enclosing
/// scope.
fn enclosing_generic_contexts<'db>(
    db: &'db dyn Db,
    module: &ParsedModuleRef,
    index: &SemanticIndex<'db>,
    scope: FileScopeId,
) -> impl Iterator<Item = GenericContext<'db>> {
    index
        .ancestor_scopes(scope)
        .filter_map(|(_, ancestor_scope)| match ancestor_scope.node() {
            NodeWithScopeKind::Class(class) => {
                let definition = index.expect_single_definition(class.node(module));
                binding_type(db, definition)
                    .into_class_literal()?
                    .generic_context(db)
            }
            NodeWithScopeKind::Function(function) => {
                let definition = index.expect_single_definition(function.node(module));
                infer_definition_types(db, definition)
                    .undecorated_type()
                    .expect("function should have undecorated type")
                    .into_function_literal()?
                    .last_definition_signature(db)
                    .generic_context
            }
            _ => None,
        })
}

/// Binds an unbound typevar.
///
/// When a typevar is first created, we will have a [`TypeVarInstance`] which does not have an
/// associated binding context. When the typevar is used in a generic class or function, we "bind"
/// it, adding the [`Definition`] of the generic class or function as its "binding context".
///
/// When an expression resolves to a typevar, our inferred type will refer to the unbound
/// [`TypeVarInstance`] from when the typevar was first created. This function walks the scopes
/// that enclosing the expression, looking for the innermost binding context that binds the
/// typevar.
///
/// If no enclosing scope has already bound the typevar, we might be in a syntactic position that
/// is about to bind it (indicated by a non-`None` `typevar_binding_context`), in which case we
/// bind the typevar with that new binding context.
pub(crate) fn bind_typevar<'db>(
    db: &'db dyn Db,
    module: &ParsedModuleRef,
    index: &SemanticIndex<'db>,
    containing_scope: FileScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    typevar: TypeVarInstance<'db>,
) -> Option<BoundTypeVarInstance<'db>> {
    enclosing_generic_contexts(db, module, index, containing_scope)
        .find_map(|enclosing_context| enclosing_context.binds_typevar(db, typevar))
        .or_else(|| {
            typevar_binding_context.map(|typevar_binding_context| {
                typevar.with_binding_context(db, typevar_binding_context)
            })
        })
}

/// A list of formal type variables for a generic function, class, or type alias.
///
/// TODO: Handle nested generic contexts better, with actual parent links to the lexically
/// containing context.
///
/// # Ordering
/// Ordering is based on the context's salsa-assigned id and not on its values.
/// The id may change between runs, or when the context was garbage collected and recreated.
#[salsa::interned(debug, heap_size=GenericContext::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct GenericContext<'db> {
    #[returns(ref)]
    pub(crate) variables: FxOrderSet<BoundTypeVarInstance<'db>>,
}

pub(super) fn walk_generic_context<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    visitor: &V,
) {
    for bound_typevar in context.variables(db) {
        visitor.visit_bound_type_var_type(db, *bound_typevar);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for GenericContext<'_> {}

impl<'db> GenericContext<'db> {
    /// Creates a generic context from a list of PEP-695 type parameters.
    pub(crate) fn from_type_params(
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        binding_context: Definition<'db>,
        type_params_node: &ast::TypeParams,
    ) -> Self {
        let variables: FxOrderSet<_> = type_params_node
            .iter()
            .filter_map(|type_param| {
                Self::variable_from_type_param(db, index, binding_context, type_param)
            })
            .collect();
        Self::new(db, variables)
    }

    fn variable_from_type_param(
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        binding_context: Definition<'db>,
        type_param_node: &ast::TypeParam,
    ) -> Option<BoundTypeVarInstance<'db>> {
        match type_param_node {
            ast::TypeParam::TypeVar(node) => {
                let definition = index.expect_single_definition(node);
                let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
                    declaration_type(db, definition).inner_type()
                else {
                    return None;
                };
                Some(typevar.with_binding_context(db, binding_context))
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
        definition: Definition<'db>,
        parameters: &Parameters<'db>,
        return_type: Option<Type<'db>>,
    ) -> Option<Self> {
        // Find all of the legacy typevars mentioned in the function signature.
        let mut variables = FxOrderSet::default();
        for param in parameters {
            if let Some(ty) = param.annotated_type() {
                ty.find_legacy_typevars(db, Some(definition), &mut variables);
            }
            if let Some(ty) = param.default_type() {
                ty.find_legacy_typevars(db, Some(definition), &mut variables);
            }
        }
        if let Some(ty) = return_type {
            ty.find_legacy_typevars(db, Some(definition), &mut variables);
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
            base.find_legacy_typevars(db, None, &mut variables);
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

    fn parameter_from_typevar(
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> Parameter<'db> {
        let typevar = bound_typevar.typevar(db);
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
                    .with_annotated_type(UnionType::from_elements(db, constraints.elements(db)));
            }
            None => {}
        }
        if let Some(default_ty) = bound_typevar.default_type(db) {
            parameter = parameter.with_default_type(default_ty);
        }
        parameter
    }

    pub(crate) fn default_specialization(
        self,
        db: &'db dyn Db,
        known_class: Option<KnownClass>,
    ) -> Specialization<'db> {
        let partial =
            self.specialize_partial(db, std::iter::repeat_n(None, self.variables(db).len()));
        if known_class == Some(KnownClass::Tuple) {
            Specialization::new(
                db,
                self,
                partial.types(db),
                Some(TupleType::homogeneous(db, Type::unknown())),
            )
        } else {
            partial
        }
    }

    /// Returns a specialization of this generic context where each typevar is mapped to itself.
    /// (And in particular, to an _inferable_ version of itself, since this will be used in our
    /// class constructor invocation machinery to infer a specialization for the class from the
    /// arguments passed to its constructor.)
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

    /// Returns a tuple type of the typevars introduced by this generic context.
    pub(crate) fn as_tuple(self, db: &'db dyn Db) -> Type<'db> {
        Type::heterogeneous_tuple(
            db,
            self.variables(db)
                .iter()
                .map(|typevar| Type::TypeVar(*typevar)),
        )
    }

    pub(crate) fn is_subset_of(self, db: &'db dyn Db, other: GenericContext<'db>) -> bool {
        self.variables(db).is_subset(other.variables(db))
    }

    pub(crate) fn binds_typevar(
        self,
        db: &'db dyn Db,
        typevar: TypeVarInstance<'db>,
    ) -> Option<BoundTypeVarInstance<'db>> {
        self.variables(db)
            .iter()
            .find(|self_bound_typevar| self_bound_typevar.typevar(db) == typevar)
            .copied()
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
        Specialization::new(db, self, types, None)
    }

    /// Creates a specialization of this generic context for the `tuple` class.
    pub(crate) fn specialize_tuple(
        self,
        db: &'db dyn Db,
        element_type: Type<'db>,
        tuple: TupleType<'db>,
    ) -> Specialization<'db> {
        Specialization::new(db, self, Box::from([element_type]), Some(tuple))
    }

    /// Creates a specialization of this generic context. Panics if the length of `types` does not
    /// match the number of typevars in the generic context. If any provided type is `None`, we
    /// will use the corresponding typevar's default type.
    pub(crate) fn specialize_partial<I>(self, db: &'db dyn Db, types: I) -> Specialization<'db>
    where
        I: IntoIterator<Item = Option<Type<'db>>>,
        I::IntoIter: ExactSizeIterator,
    {
        let types = types.into_iter();
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
        for (idx, (ty, typevar)) in types.zip(variables).enumerate() {
            if let Some(ty) = ty {
                expanded[idx] = ty;
                continue;
            }

            let Some(default) = typevar.default_type(db) else {
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

        Specialization::new(db, self, expanded.into_boxed_slice(), None)
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let variables: FxOrderSet<_> = self
            .variables(db)
            .iter()
            .map(|bound_typevar| bound_typevar.normalized_impl(db, visitor))
            .collect();
        Self::new(db, variables)
    }

    fn heap_size((variables,): &(FxOrderSet<BoundTypeVarInstance<'db>>,)) -> usize {
        ruff_memory_usage::order_set_heap_size(variables)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(super) enum LegacyGenericBase {
    Generic,
    Protocol,
}

impl LegacyGenericBase {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "Generic",
            Self::Protocol => "Protocol",
        }
    }
}

impl std::fmt::Display for LegacyGenericBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An assignment of a specific type to each type variable in a generic scope.
///
/// TODO: Handle nested specializations better, with actual parent links to the specialization of
/// the lexically containing context.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct Specialization<'db> {
    pub(crate) generic_context: GenericContext<'db>,
    #[returns(deref)]
    pub(crate) types: Box<[Type<'db>]>,

    /// For specializations of `tuple`, we also store more detailed information about the tuple's
    /// elements, above what the class's (single) typevar can represent.
    tuple_inner: Option<TupleType<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for Specialization<'_> {}

pub(super) fn walk_specialization<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    specialization: Specialization<'db>,
    visitor: &V,
) {
    walk_generic_context(db, specialization.generic_context(db), visitor);
    for ty in specialization.types(db) {
        visitor.visit_type(db, *ty);
    }
    if let Some(tuple) = specialization.tuple_inner(db) {
        walk_tuple_type(db, tuple, visitor);
    }
}

impl<'db> Specialization<'db> {
    /// Returns the tuple spec for a specialization of the `tuple` class.
    pub(crate) fn tuple(self, db: &'db dyn Db) -> Option<&'db TupleSpec<'db>> {
        self.tuple_inner(db).map(|tuple_type| tuple_type.tuple(db))
    }

    /// Returns the type that a typevar is mapped to, or None if the typevar isn't part of this
    /// mapping.
    pub(crate) fn get(
        self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> Option<Type<'db>> {
        let index = self
            .generic_context(db)
            .variables(db)
            .get_index_of(&bound_typevar)?;
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
        self.apply_type_mapping_impl(db, type_mapping, &ApplyTypeMappingVisitor::default())
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        let types: Box<[_]> = self
            .types(db)
            .iter()
            .map(|ty| ty.apply_type_mapping_impl(db, type_mapping, visitor))
            .collect();
        let tuple_inner = self
            .tuple_inner(db)
            .and_then(|tuple| tuple.apply_type_mapping_impl(db, type_mapping, visitor));
        Specialization::new(db, self.generic_context(db), types, tuple_inner)
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
        // TODO: Combine the tuple specs too
        Specialization::new(db, self.generic_context(db), types, None)
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let types: Box<[_]> = self
            .types(db)
            .iter()
            .map(|ty| ty.normalized_impl(db, visitor))
            .collect();
        let tuple_inner = self
            .tuple_inner(db)
            .and_then(|tuple| tuple.normalized_impl(db, visitor));
        let context = self.generic_context(db).normalized_impl(db, visitor);
        Self::new(db, context, types, tuple_inner)
    }

    pub(super) fn materialize(self, db: &'db dyn Db, variance: TypeVarVariance) -> Self {
        let types: Box<[_]> = self
            .generic_context(db)
            .variables(db)
            .into_iter()
            .zip(self.types(db))
            .map(|(bound_typevar, vartype)| {
                let variance = match bound_typevar.typevar(db).variance(db) {
                    TypeVarVariance::Invariant => TypeVarVariance::Invariant,
                    TypeVarVariance::Covariant => variance,
                    TypeVarVariance::Contravariant => variance.flip(),
                    TypeVarVariance::Bivariant => unreachable!(),
                };
                vartype.materialize(db, variance)
            })
            .collect();
        let tuple_inner = self.tuple_inner(db).and_then(|tuple| {
            // Tuples are immutable, so tuple element types are always in covariant position.
            tuple.materialize(db, variance)
        });
        Specialization::new(db, self.generic_context(db), types, tuple_inner)
    }

    pub(crate) fn has_relation_to_impl<C: Constraints<'db>>(
        self,
        db: &'db dyn Db,
        other: Self,
        relation: TypeRelation,
        visitor: &HasRelationToVisitor<'db, C>,
    ) -> C {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return C::never(db);
        }

        if let (Some(self_tuple), Some(other_tuple)) = (self.tuple_inner(db), other.tuple_inner(db))
        {
            return self_tuple.has_relation_to_impl(db, other_tuple, relation, visitor);
        }

        let mut result = C::always(db);
        for ((bound_typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            if self_type.is_dynamic() || other_type.is_dynamic() {
                match relation {
                    TypeRelation::Assignability => continue,
                    TypeRelation::Subtyping => return C::never(db),
                }
            }

            // Subtyping/assignability of each type in the specialization depends on the variance
            // of the corresponding typevar:
            //   - covariant: verify that self_type <: other_type
            //   - contravariant: verify that other_type <: self_type
            //   - invariant: verify that self_type <: other_type AND other_type <: self_type
            //   - bivariant: skip, can't make subtyping/assignability false
            let compatible = match bound_typevar.typevar(db).variance(db) {
                TypeVarVariance::Invariant => match relation {
                    TypeRelation::Subtyping => {
                        C::from_bool(db, self_type.is_equivalent_to(db, *other_type))
                    }
                    TypeRelation::Assignability => C::from_bool(
                        db,
                        self_type.is_assignable_to(db, *other_type)
                            && other_type.is_assignable_to(db, *self_type),
                    ),
                },
                TypeVarVariance::Covariant => {
                    self_type.has_relation_to_impl(db, *other_type, relation, visitor)
                }
                TypeVarVariance::Contravariant => {
                    other_type.has_relation_to_impl(db, *self_type, relation, visitor)
                }
                TypeVarVariance::Bivariant => C::always(db),
            };
            if result.intersect(db, compatible) {
                return result;
            }
        }

        result
    }

    pub(crate) fn is_equivalent_to(self, db: &'db dyn Db, other: Specialization<'db>) -> bool {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return false;
        }

        for ((bound_typevar, self_type), other_type) in (generic_context.variables(db).into_iter())
            .zip(self.types(db))
            .zip(other.types(db))
        {
            // Equivalence of each type in the specialization depends on the variance of the
            // corresponding typevar:
            //   - covariant: verify that self_type == other_type
            //   - contravariant: verify that other_type == self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make equivalence false
            let compatible = match bound_typevar.typevar(db).variance(db) {
                TypeVarVariance::Invariant
                | TypeVarVariance::Covariant
                | TypeVarVariance::Contravariant => self_type.is_equivalent_to(db, *other_type),
                TypeVarVariance::Bivariant => true,
            };
            if !compatible {
                return false;
            }
        }

        match (self.tuple_inner(db), other.tuple_inner(db)) {
            (Some(_), None) | (None, Some(_)) => return false,
            (None, None) => {}
            (Some(self_tuple), Some(other_tuple)) => {
                if !self_tuple.is_equivalent_to(db, other_tuple) {
                    return false;
                }
            }
        }

        true
    }

    pub(crate) fn find_legacy_typevars(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
    ) {
        for ty in self.types(db) {
            ty.find_legacy_typevars(db, binding_context, typevars);
        }
        // A tuple's specialization will include all of its element types, so we don't need to also
        // look in `self.tuple`.
    }
}

/// A mapping between type variables and types.
///
/// You will usually use [`Specialization`] instead of this type. This type is used when we need to
/// substitute types for type variables before we have fully constructed a [`Specialization`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub struct PartialSpecialization<'a, 'db> {
    generic_context: GenericContext<'db>,
    types: Cow<'a, [Type<'db>]>,
}

pub(super) fn walk_partial_specialization<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    specialization: &PartialSpecialization<'_, 'db>,
    visitor: &V,
) {
    walk_generic_context(db, specialization.generic_context, visitor);
    for ty in &*specialization.types {
        visitor.visit_type(db, *ty);
    }
}

impl<'db> PartialSpecialization<'_, 'db> {
    /// Returns the type that a typevar is mapped to, or None if the typevar isn't part of this
    /// mapping.
    pub(crate) fn get(
        &self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> Option<Type<'db>> {
        let index = self
            .generic_context
            .variables(db)
            .get_index_of(&bound_typevar)?;
        self.types.get(index).copied()
    }

    pub(crate) fn to_owned(&self) -> PartialSpecialization<'db, 'db> {
        PartialSpecialization {
            generic_context: self.generic_context,
            types: Cow::from(self.types.clone().into_owned()),
        }
    }

    pub(crate) fn normalized_impl(
        &self,
        db: &'db dyn Db,
        visitor: &NormalizedVisitor<'db>,
    ) -> PartialSpecialization<'db, 'db> {
        let generic_context = self.generic_context.normalized_impl(db, visitor);
        let types: Cow<_> = self
            .types
            .iter()
            .map(|ty| ty.normalized_impl(db, visitor))
            .collect();

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
    types: FxHashMap<BoundTypeVarInstance<'db>, Type<'db>>,
}

impl<'db> SpecializationBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db) -> Self {
        Self {
            db,
            types: FxHashMap::default(),
        }
    }

    pub(crate) fn build(&mut self, generic_context: GenericContext<'db>) -> Specialization<'db> {
        let types = generic_context
            .variables(self.db)
            .iter()
            .map(|variable| self.types.get(variable).copied());
        // TODO Infer the tuple spec for a tuple type
        generic_context.specialize_partial(self.db, types)
    }

    fn add_type_mapping(&mut self, bound_typevar: BoundTypeVarInstance<'db>, ty: Type<'db>) {
        self.types
            .entry(bound_typevar)
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
        if formal == actual {
            return Ok(());
        }

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
                let mut bound_typevars =
                    formal.elements(self.db).iter().filter_map(|ty| match ty {
                        Type::TypeVar(bound_typevar) => Some(*bound_typevar),
                        _ => None,
                    });
                let bound_typevar = bound_typevars.next();
                let additional_bound_typevars = bound_typevars.next();
                if let (Some(bound_typevar), None) = (bound_typevar, additional_bound_typevars) {
                    self.add_type_mapping(bound_typevar, actual);
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

            (Type::TypeVar(bound_typevar), ty) | (ty, Type::TypeVar(bound_typevar)) => {
                match bound_typevar.typevar(self.db).bound_or_constraints(self.db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        if !ty.is_assignable_to(self.db, bound) {
                            return Err(SpecializationError::MismatchedBound {
                                bound_typevar,
                                argument: ty,
                            });
                        }
                        self.add_type_mapping(bound_typevar, ty);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        for constraint in constraints.elements(self.db) {
                            if ty.is_assignable_to(self.db, *constraint) {
                                self.add_type_mapping(bound_typevar, *constraint);
                                return Ok(());
                            }
                        }
                        return Err(SpecializationError::MismatchedConstraint {
                            bound_typevar,
                            argument: ty,
                        });
                    }
                    _ => {
                        self.add_type_mapping(bound_typevar, ty);
                    }
                }
            }

            (formal, Type::NominalInstance(actual_nominal)) => {
                // Special case: `formal` and `actual` are both tuples.
                if let (Some(formal_tuple), Some(actual_tuple)) = (
                    formal.tuple_instance_spec(self.db),
                    actual_nominal.tuple_spec(self.db),
                ) {
                    let Some(most_precise_length) =
                        formal_tuple.len().most_precise(actual_tuple.len())
                    else {
                        return Ok(());
                    };
                    let Ok(formal_tuple) = formal_tuple.resize(self.db, most_precise_length) else {
                        return Ok(());
                    };
                    let Ok(actual_tuple) = actual_tuple.resize(self.db, most_precise_length) else {
                        return Ok(());
                    };
                    for (formal_element, actual_element) in
                        formal_tuple.all_elements().zip(actual_tuple.all_elements())
                    {
                        self.infer(*formal_element, *actual_element)?;
                    }
                    return Ok(());
                }

                // Extract formal_alias if this is a generic class
                let formal_alias = match formal {
                    Type::NominalInstance(formal_nominal) => {
                        formal_nominal.class(self.db).into_generic_alias()
                    }
                    // TODO: This will only handle classes that explicit implement a generic protocol
                    // by listing it as a base class. To handle classes that implicitly implement a
                    // generic protocol, we will need to check the types of the protocol members to be
                    // able to infer the specialization of the protocol that the class implements.
                    Type::ProtocolInstance(ProtocolInstanceType {
                        inner: Protocol::FromClass(ClassType::Generic(alias)),
                        ..
                    }) => Some(alias),
                    _ => None,
                };

                if let Some(formal_alias) = formal_alias {
                    let formal_origin = formal_alias.origin(self.db);
                    for base in actual_nominal.class(self.db).iter_mro(self.db) {
                        let ClassBase::Class(ClassType::Generic(base_alias)) = base else {
                            continue;
                        };
                        if formal_origin != base_alias.origin(self.db) {
                            continue;
                        }
                        let formal_specialization =
                            formal_alias.specialization(self.db).types(self.db);
                        let base_specialization = base_alias.specialization(self.db).types(self.db);
                        for (formal_ty, base_ty) in
                            formal_specialization.iter().zip(base_specialization)
                        {
                            self.infer(*formal_ty, *base_ty)?;
                        }
                        return Ok(());
                    }
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
        bound_typevar: BoundTypeVarInstance<'db>,
        argument: Type<'db>,
    },
    MismatchedConstraint {
        bound_typevar: BoundTypeVarInstance<'db>,
        argument: Type<'db>,
    },
}

impl<'db> SpecializationError<'db> {
    pub(crate) fn bound_typevar(&self) -> BoundTypeVarInstance<'db> {
        match self {
            Self::MismatchedBound { bound_typevar, .. } => *bound_typevar,
            Self::MismatchedConstraint { bound_typevar, .. } => *bound_typevar,
        }
    }

    pub(crate) fn argument_type(&self) -> Type<'db> {
        match self {
            Self::MismatchedBound { argument, .. } => *argument,
            Self::MismatchedConstraint { argument, .. } => *argument,
        }
    }
}
