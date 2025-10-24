use std::cell::RefCell;
use std::fmt::Display;

use itertools::Itertools;
use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::{FileScopeId, NodeWithScopeKind, ScopeId};
use crate::semantic_index::{SemanticIndex, semantic_index};
use crate::types::class::ClassType;
use crate::types::class_base::ClassBase;
use crate::types::constraints::ConstraintSet;
use crate::types::instance::{Protocol, ProtocolInstanceType};
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::tuple::{TupleSpec, TupleType, walk_tuple_type};
use crate::types::visitor::{NonAtomicType, TypeKind, TypeVisitor, walk_non_atomic_type};
use crate::types::{
    ApplyTypeMappingVisitor, BoundTypeVarIdentity, BoundTypeVarInstance, ClassLiteral,
    FindLegacyTypeVarsVisitor, HasRelationToVisitor, IsDisjointVisitor, IsEquivalentVisitor,
    KnownClass, KnownInstanceType, MaterializationKind, NormalizedVisitor, Type, TypeContext,
    TypeMapping, TypeRelation, TypeVarBoundOrConstraints, TypeVarIdentity, TypeVarInstance,
    TypeVarKind, TypeVarVariance, UnionType, declaration_type, walk_bound_type_var_type,
};
use crate::{Db, FxIndexSet, FxOrderMap, FxOrderSet};

/// Returns an iterator of any generic context introduced by the given scope or any enclosing
/// scope.
pub(crate) fn enclosing_generic_contexts<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    scope: FileScopeId,
) -> impl Iterator<Item = GenericContext<'db>> {
    index
        .ancestor_scopes(scope)
        .filter_map(|(_, ancestor_scope)| ancestor_scope.node().generic_context(db, index))
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
    index: &SemanticIndex<'db>,
    containing_scope: FileScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    typevar: TypeVarInstance<'db>,
) -> Option<BoundTypeVarInstance<'db>> {
    // typing.Self is treated like a legacy typevar, but doesn't follow the same scoping rules. It is always bound to the outermost method in the containing class.
    if matches!(typevar.kind(db), TypeVarKind::TypingSelf) {
        for ((_, inner), (_, outer)) in index.ancestor_scopes(containing_scope).tuple_windows() {
            if outer.kind().is_class() {
                if let NodeWithScopeKind::Function(function) = inner.node() {
                    let definition = index.expect_single_definition(function);
                    return Some(typevar.with_binding_context(db, definition));
                }
            }
        }
    }
    enclosing_generic_contexts(db, index, containing_scope)
        .find_map(|enclosing_context| enclosing_context.binds_typevar(db, typevar))
        .or_else(|| {
            typevar_binding_context.map(|typevar_binding_context| {
                typevar.with_binding_context(db, typevar_binding_context)
            })
        })
}

/// Create a `typing.Self` type variable for a given class.
pub(crate) fn typing_self<'db>(
    db: &'db dyn Db,
    function_scope_id: ScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    class: ClassLiteral<'db>,
) -> Option<Type<'db>> {
    let index = semantic_index(db, function_scope_id.file(db));

    let identity = TypeVarIdentity::new(
        db,
        ast::name::Name::new_static("Self"),
        Some(class.definition(db)),
        TypeVarKind::TypingSelf,
    );
    let bounds = TypeVarBoundOrConstraints::UpperBound(Type::instance(
        db,
        class.identity_specialization(db),
    ));
    let typevar = TypeVarInstance::new(
        db,
        identity,
        Some(bounds.into()),
        // According to the [spec], we can consider `Self`
        // equivalent to an invariant type variable
        // [spec]: https://typing.python.org/en/latest/spec/generics.html#self
        Some(TypeVarVariance::Invariant),
        None,
    );

    bind_typevar(
        db,
        index,
        function_scope_id.file_scope_id(db),
        typevar_binding_context,
        typevar,
    )
    .map(Type::TypeVar)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum InferableTypeVars<'a, 'db> {
    None,
    One(&'a FxHashSet<BoundTypeVarIdentity<'db>>),
    Two(
        &'a InferableTypeVars<'a, 'db>,
        &'a InferableTypeVars<'a, 'db>,
    ),
}

impl<'db> BoundTypeVarInstance<'db> {
    pub(crate) fn is_inferable(
        self,
        db: &'db dyn Db,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> bool {
        match inferable {
            InferableTypeVars::None => false,
            InferableTypeVars::One(typevars) => typevars.contains(&self.identity(db)),
            InferableTypeVars::Two(left, right) => {
                self.is_inferable(db, *left) || self.is_inferable(db, *right)
            }
        }
    }
}

impl<'a, 'db> InferableTypeVars<'a, 'db> {
    pub(crate) fn merge(&'a self, other: Option<&'a InferableTypeVars<'a, 'db>>) -> Self {
        match other {
            Some(other) => InferableTypeVars::Two(self, other),
            None => *self,
        }
    }

    // Keep this around for debugging purposes
    #[expect(dead_code)]
    pub(crate) fn display(&self, db: &'db dyn Db) -> impl Display {
        fn find_typevars<'db>(
            result: &mut FxHashSet<BoundTypeVarIdentity<'db>>,
            inferable: &InferableTypeVars<'_, 'db>,
        ) {
            match inferable {
                InferableTypeVars::None => {}
                InferableTypeVars::One(typevars) => result.extend(typevars.iter().copied()),
                InferableTypeVars::Two(left, right) => {
                    find_typevars(result, left);
                    find_typevars(result, right);
                }
            }
        }

        let mut typevars = FxHashSet::default();
        find_typevars(&mut typevars, self);
        format!(
            "[{}]",
            typevars
                .into_iter()
                .map(|identity| identity.display(db))
                .format(", ")
        )
    }
}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub struct GenericContextTypeVar<'db> {
    bound_typevar: BoundTypeVarInstance<'db>,
    should_promote_literals: bool,
}

impl<'db> GenericContextTypeVar<'db> {
    fn new(bound_typevar: BoundTypeVarInstance<'db>) -> Self {
        Self {
            bound_typevar,
            should_promote_literals: false,
        }
    }

    fn promote_literals(mut self) -> Self {
        self.should_promote_literals = true;
        self
    }
}

/// A list of formal type variables for a generic function, class, or type alias.
///
/// # Ordering
/// Ordering is based on the context's salsa-assigned id and not on its values.
/// The id may change between runs, or when the context was garbage collected and recreated.
#[salsa::interned(debug, constructor=new_internal, heap_size=GenericContext::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct GenericContext<'db> {
    #[returns(ref)]
    variables_inner: FxOrderMap<BoundTypeVarIdentity<'db>, GenericContextTypeVar<'db>>,
}

pub(super) fn walk_generic_context<'db, V: super::visitor::TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    context: GenericContext<'db>,
    visitor: &V,
) {
    for bound_typevar in context.variables(db) {
        visitor.visit_bound_type_var_type(db, bound_typevar);
    }
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for GenericContext<'_> {}

impl<'db> GenericContext<'db> {
    fn from_variables(
        db: &'db dyn Db,
        variables: impl IntoIterator<Item = GenericContextTypeVar<'db>>,
    ) -> Self {
        Self::new_internal(
            db,
            variables
                .into_iter()
                .map(|variable| (variable.bound_typevar.identity(db), variable))
                .collect::<FxOrderMap<_, _>>(),
        )
    }

    /// Creates a generic context from a list of PEP-695 type parameters.
    pub(crate) fn from_type_params(
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        binding_context: Definition<'db>,
        type_params_node: &ast::TypeParams,
    ) -> Self {
        let variables = type_params_node.iter().filter_map(|type_param| {
            Self::variable_from_type_param(db, index, binding_context, type_param)
        });

        Self::from_typevar_instances(db, variables)
    }

    /// Creates a generic context from a list of `BoundTypeVarInstance`s.
    pub(crate) fn from_typevar_instances(
        db: &'db dyn Db,
        type_params: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
    ) -> Self {
        Self::from_variables(db, type_params.into_iter().map(GenericContextTypeVar::new))
    }

    /// Returns a copy of this generic context where we will promote literal types in any inferred
    /// specializations.
    pub(crate) fn promote_literals(self, db: &'db dyn Db) -> Self {
        Self::from_variables(
            db,
            self.variables_inner(db)
                .values()
                .map(|variable| variable.promote_literals()),
        )
    }

    /// Merge this generic context with another, returning a new generic context that
    /// contains type variables from both contexts.
    pub(crate) fn merge(self, db: &'db dyn Db, other: Self) -> Self {
        Self::from_variables(
            db,
            self.variables_inner(db)
                .values()
                .chain(other.variables_inner(db).values())
                .copied(),
        )
    }

    pub(crate) fn inferable_typevars(self, db: &'db dyn Db) -> InferableTypeVars<'db, 'db> {
        #[derive(Default)]
        struct CollectTypeVars<'db> {
            typevars: RefCell<FxHashSet<BoundTypeVarIdentity<'db>>>,
            seen_types: RefCell<FxIndexSet<NonAtomicType<'db>>>,
        }

        impl<'db> TypeVisitor<'db> for CollectTypeVars<'db> {
            fn should_visit_lazy_type_attributes(&self) -> bool {
                true
            }

            fn visit_bound_type_var_type(
                &self,
                db: &'db dyn Db,
                bound_typevar: BoundTypeVarInstance<'db>,
            ) {
                self.typevars
                    .borrow_mut()
                    .insert(bound_typevar.identity(db));
                walk_bound_type_var_type(db, bound_typevar, self);
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                match TypeKind::from(ty) {
                    TypeKind::Atomic => {}
                    TypeKind::NonAtomic(non_atomic_type) => {
                        if !self.seen_types.borrow_mut().insert(non_atomic_type) {
                            // If we have already seen this type, we can skip it.
                            return;
                        }
                        walk_non_atomic_type(db, non_atomic_type, self);
                    }
                }
            }
        }

        #[salsa::tracked(
            returns(ref),
            cycle_initial=inferable_typevars_cycle_initial,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn inferable_typevars_inner<'db>(
            db: &'db dyn Db,
            generic_context: GenericContext<'db>,
        ) -> FxHashSet<BoundTypeVarIdentity<'db>> {
            let visitor = CollectTypeVars::default();
            for bound_typevar in generic_context.variables(db) {
                visitor.visit_bound_type_var_type(db, bound_typevar);
            }
            visitor.typevars.into_inner()
        }

        // This ensures that salsa caches the FxHashSet, not the InferableTypeVars that wraps it.
        // (That way InferableTypeVars can contain references, and doesn't need to impl
        // salsa::Update.)
        InferableTypeVars::One(inferable_typevars_inner(db, self))
    }

    pub(crate) fn variables(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = BoundTypeVarInstance<'db>> + Clone {
        self.variables_inner(db)
            .values()
            .map(|variable| variable.bound_typevar)
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
        Some(Self::from_typevar_instances(db, variables))
    }

    pub(crate) fn merge_pep695_and_legacy(
        db: &'db dyn Db,
        pep695_generic_context: Option<Self>,
        legacy_generic_context: Option<Self>,
    ) -> Option<Self> {
        match (legacy_generic_context, pep695_generic_context) {
            (Some(legacy_ctx), Some(ctx)) => {
                if legacy_ctx
                    .variables(db)
                    .exactly_one()
                    .is_ok_and(|bound_typevar| bound_typevar.typevar(db).is_self(db))
                {
                    Some(legacy_ctx.merge(db, ctx))
                } else {
                    // TODO: Raise a diagnostic — mixing PEP 695 and legacy typevars is not allowed
                    Some(ctx)
                }
            }
            (left, right) => left.or(right),
        }
    }

    /// Creates a generic context from the legacy `TypeVar`s that appear in class's base class
    /// list.
    pub(crate) fn from_base_classes(
        db: &'db dyn Db,
        definition: Definition<'db>,
        bases: impl Iterator<Item = Type<'db>>,
    ) -> Option<Self> {
        let mut variables = FxOrderSet::default();
        for base in bases {
            base.find_legacy_typevars(db, Some(definition), &mut variables);
        }
        if variables.is_empty() {
            return None;
        }
        Some(Self::from_typevar_instances(db, variables))
    }

    pub(crate) fn len(self, db: &'db dyn Db) -> usize {
        self.variables_inner(db).len()
    }

    pub(crate) fn signature(self, db: &'db dyn Db) -> Signature<'db> {
        let parameters = Parameters::new(
            self.variables(db)
                .map(|typevar| Self::parameter_from_typevar(db, typevar)),
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
        let partial = self.specialize_partial(db, std::iter::repeat_n(None, self.len(db)));
        if known_class == Some(KnownClass::Tuple) {
            Specialization::new(
                db,
                self,
                partial.types(db),
                None,
                Some(TupleType::homogeneous(db, Type::unknown())),
            )
        } else {
            partial
        }
    }

    /// Returns a specialization of this generic context where each typevar is mapped to itself.
    pub(crate) fn identity_specialization(self, db: &'db dyn Db) -> Specialization<'db> {
        let types = self.variables(db).map(Type::TypeVar).collect();
        self.specialize(db, types)
    }

    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> Specialization<'db> {
        let types = vec![Type::unknown(); self.len(db)];
        self.specialize(db, types.into())
    }

    /// Returns a tuple type of the typevars introduced by this generic context.
    pub(crate) fn as_tuple(self, db: &'db dyn Db) -> Type<'db> {
        Type::heterogeneous_tuple(db, self.variables(db).map(Type::TypeVar))
    }

    pub(crate) fn is_subset_of(self, db: &'db dyn Db, other: GenericContext<'db>) -> bool {
        let other_variables = other.variables_inner(db);
        self.variables(db)
            .all(|bound_typevar| other_variables.contains_key(&bound_typevar.identity(db)))
    }

    pub(crate) fn binds_named_typevar(
        self,
        db: &'db dyn Db,
        name: &'db ast::name::Name,
    ) -> Option<BoundTypeVarInstance<'db>> {
        self.variables(db)
            .find(|self_bound_typevar| self_bound_typevar.typevar(db).name(db) == name)
    }

    pub(crate) fn binds_typevar(
        self,
        db: &'db dyn Db,
        typevar: TypeVarInstance<'db>,
    ) -> Option<BoundTypeVarInstance<'db>> {
        self.variables(db).find(|self_bound_typevar| {
            self_bound_typevar.typevar(db).identity(db) == typevar.identity(db)
        })
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
        assert!(self.len(db) == types.len());
        Specialization::new(db, self, types, None, None)
    }

    /// Creates a specialization of this generic context for the `tuple` class.
    pub(crate) fn specialize_tuple(
        self,
        db: &'db dyn Db,
        element_type: Type<'db>,
        tuple: TupleType<'db>,
    ) -> Specialization<'db> {
        Specialization::new(db, self, Box::from([element_type]), None, Some(tuple))
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
        assert!(self.len(db) == types.len());

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
                types: &expanded[0..idx],
            };
            let default = default.apply_type_mapping(
                db,
                &TypeMapping::PartialSpecialization(partial),
                TypeContext::default(),
            );
            expanded[idx] = default;
        }

        Specialization::new(db, self, expanded.into_boxed_slice(), None, None)
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let variables = self
            .variables(db)
            .map(|bound_typevar| bound_typevar.normalized_impl(db, visitor));

        Self::from_typevar_instances(db, variables)
    }

    fn heap_size(
        (variables,): &(FxOrderMap<BoundTypeVarIdentity<'db>, GenericContextTypeVar<'db>>,),
    ) -> usize {
        ruff_memory_usage::order_map_heap_size(variables)
    }
}

fn inferable_typevars_cycle_initial<'db>(
    _db: &'db dyn Db,
    _self: GenericContext<'db>,
) -> FxHashSet<BoundTypeVarIdentity<'db>> {
    FxHashSet::default()
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
    /// The materialization kind of the specialization. For example, given an invariant
    /// generic type `A`, `Top[A[Any]]` is a supertype of all materializations of `A[Any]`,
    /// and is represented here with `Some(MaterializationKind::Top)`. Similarly,
    /// `Bottom[A[Any]]` is a subtype of all materializations of `A[Any]`, and is represented
    /// with `Some(MaterializationKind::Bottom)`.
    /// The `materialization_kind` field may be non-`None` only if the specialization contains
    /// dynamic types in invariant positions.
    pub(crate) materialization_kind: Option<MaterializationKind>,

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

#[expect(clippy::too_many_arguments)]
fn is_subtype_in_invariant_position<'db>(
    db: &'db dyn Db,
    derived_type: &Type<'db>,
    derived_materialization: MaterializationKind,
    base_type: &Type<'db>,
    base_materialization: MaterializationKind,
    inferable: InferableTypeVars<'_, 'db>,
    relation_visitor: &HasRelationToVisitor<'db>,
    disjointness_visitor: &IsDisjointVisitor<'db>,
) -> ConstraintSet<'db> {
    let derived_top = derived_type.top_materialization(db);
    let derived_bottom = derived_type.bottom_materialization(db);
    let base_top = base_type.top_materialization(db);
    let base_bottom = base_type.bottom_materialization(db);

    let is_subtype_of = |derived: Type<'db>, base: Type<'db>| {
        // TODO:
        // This should be removed and properly handled in the respective
        // `(Type::TypeVar(_), _) | (_, Type::TypeVar(_))` branch of
        // `Type::has_relation_to_impl`. Right now, we can not generally
        // return `ConstraintSet::from(true)` from that branch, as that
        // leads to union simplification, which means that we lose track
        // of type variables without recording the constraints under which
        // the relation holds.
        if matches!(base, Type::TypeVar(_)) || matches!(derived, Type::TypeVar(_)) {
            return ConstraintSet::from(true);
        }

        derived.has_relation_to_impl(
            db,
            base,
            inferable,
            TypeRelation::Subtyping,
            relation_visitor,
            disjointness_visitor,
        )
    };
    match (derived_materialization, base_materialization) {
        // `Derived` is a subtype of `Base` if the range of materializations covered by `Derived`
        // is a subset of the range covered by `Base`.
        (MaterializationKind::Top, MaterializationKind::Top) => {
            is_subtype_of(base_bottom, derived_bottom)
                .and(db, || is_subtype_of(derived_top, base_top))
        }
        // One bottom is a subtype of another if it covers a strictly larger set of materializations.
        (MaterializationKind::Bottom, MaterializationKind::Bottom) => {
            is_subtype_of(derived_bottom, base_bottom)
                .and(db, || is_subtype_of(base_top, derived_top))
        }
        // The bottom materialization of `Derived` is a subtype of the top materialization
        // of `Base` if there is some type that is both within the
        // range of types covered by derived and within the range covered by base, because if such a type
        // exists, it's a subtype of `Top[base]` and a supertype of `Bottom[derived]`.
        (MaterializationKind::Bottom, MaterializationKind::Top) => {
            (is_subtype_of(base_bottom, derived_bottom)
                .and(db, || is_subtype_of(derived_bottom, base_top)))
            .or(db, || {
                is_subtype_of(base_bottom, derived_top)
                    .and(db, || is_subtype_of(derived_top, base_top))
            })
            .or(db, || {
                is_subtype_of(base_top, derived_top)
                    .and(db, || is_subtype_of(derived_bottom, base_top))
            })
        }
        // A top materialization is a subtype of a bottom materialization only if both original
        // un-materialized types are the same fully static type.
        (MaterializationKind::Top, MaterializationKind::Bottom) => {
            is_subtype_of(derived_top, base_bottom)
                .and(db, || is_subtype_of(base_top, derived_bottom))
        }
    }
}

/// Whether two types encountered in an invariant position
/// have a relation (subtyping or assignability), taking into account
/// that the two types may come from a top or bottom materialization.
#[expect(clippy::too_many_arguments)]
fn has_relation_in_invariant_position<'db>(
    db: &'db dyn Db,
    derived_type: &Type<'db>,
    derived_materialization: Option<MaterializationKind>,
    base_type: &Type<'db>,
    base_materialization: Option<MaterializationKind>,
    inferable: InferableTypeVars<'_, 'db>,
    relation: TypeRelation,
    relation_visitor: &HasRelationToVisitor<'db>,
    disjointness_visitor: &IsDisjointVisitor<'db>,
) -> ConstraintSet<'db> {
    match (derived_materialization, base_materialization, relation) {
        // Top and bottom materializations are fully static types, so subtyping
        // is the same as assignability.
        (Some(derived_mat), Some(base_mat), _) => is_subtype_in_invariant_position(
            db,
            derived_type,
            derived_mat,
            base_type,
            base_mat,
            inferable,
            relation_visitor,
            disjointness_visitor,
        ),
        // Subtyping between invariant type parameters without a top/bottom materialization necessitates
        // checking the subtyping relation both ways: `A` must be a subtype of `B` *and* `B` must be a
        // subtype of `A`. The same applies to assignability.
        //
        // For subtyping between fully static types, this is the same as equivalence. However, we cannot
        // use `is_equivalent_to` (or `when_equivalent_to`) here, because we (correctly) understand
        // `list[Any]` as being equivalent to `list[Any]`, but we don't want `list[Any]` to be
        // considered a subtype of `list[Any]`. For assignability, we would have the opposite issue if
        // we simply checked for equivalence here: `Foo[Any]` should be considered assignable to
        // `Foo[list[Any]]` even if `Foo` is invariant, and even though `Any` is not equivalent to
        // `list[Any]`, because `Any` is assignable to `list[Any]` and `list[Any]` is assignable to
        // `Any`.
        (None, None, relation) => derived_type
            .has_relation_to_impl(
                db,
                *base_type,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            )
            .and(db, || {
                base_type.has_relation_to_impl(
                    db,
                    *derived_type,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                )
            }),
        // For gradual types, A <: B (subtyping) is defined as Top[A] <: Bottom[B]
        (None, Some(base_mat), TypeRelation::Subtyping | TypeRelation::Redundancy) => {
            is_subtype_in_invariant_position(
                db,
                derived_type,
                MaterializationKind::Top,
                base_type,
                base_mat,
                inferable,
                relation_visitor,
                disjointness_visitor,
            )
        }
        (Some(derived_mat), None, TypeRelation::Subtyping | TypeRelation::Redundancy) => {
            is_subtype_in_invariant_position(
                db,
                derived_type,
                derived_mat,
                base_type,
                MaterializationKind::Bottom,
                inferable,
                relation_visitor,
                disjointness_visitor,
            )
        }
        // And A <~ B (assignability) is Bottom[A] <: Top[B]
        (None, Some(base_mat), TypeRelation::Assignability) => is_subtype_in_invariant_position(
            db,
            derived_type,
            MaterializationKind::Bottom,
            base_type,
            base_mat,
            inferable,
            relation_visitor,
            disjointness_visitor,
        ),
        (Some(derived_mat), None, TypeRelation::Assignability) => is_subtype_in_invariant_position(
            db,
            derived_type,
            derived_mat,
            base_type,
            MaterializationKind::Top,
            inferable,
            relation_visitor,
            disjointness_visitor,
        ),
    }
}

impl<'db> Specialization<'db> {
    /// Restricts this specialization to only include the typevars in a generic context. If the
    /// specialization does not include all of those typevars, returns `None`.
    pub(crate) fn restrict(
        self,
        db: &'db dyn Db,
        generic_context: GenericContext<'db>,
    ) -> Option<Self> {
        let self_variables = self.generic_context(db).variables_inner(db);
        let self_types = self.types(db);
        let restricted_variables = generic_context.variables(db);
        let restricted_types: Option<Box<[_]>> = restricted_variables
            .map(|variable| {
                let index = self_variables.get_index_of(&variable.identity(db))?;
                self_types.get(index).copied()
            })
            .collect();
        Some(Self::new(
            db,
            generic_context,
            restricted_types?,
            self.materialization_kind(db),
            None,
        ))
    }

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
            .variables_inner(db)
            .get_index_of(&bound_typevar.identity(db))?;
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
        let new_specialization = self.apply_type_mapping(db, &TypeMapping::Specialization(other));
        match other.materialization_kind(db) {
            None => new_specialization,
            Some(materialization_kind) => new_specialization.materialize_impl(
                db,
                materialization_kind,
                &ApplyTypeMappingVisitor::default(),
            ),
        }
    }

    pub(crate) fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        self.apply_type_mapping_impl(db, type_mapping, &[], &ApplyTypeMappingVisitor::default())
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        if let TypeMapping::Materialize(materialization_kind) = type_mapping {
            return self.materialize_impl(db, *materialization_kind, visitor);
        }

        let types: Box<[_]> = self
            .types(db)
            .iter()
            .enumerate()
            .map(|(i, ty)| {
                let tcx = TypeContext::new(tcx.get(i).copied());
                ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
            })
            .collect();

        let tuple_inner = self.tuple_inner(db).and_then(|tuple| {
            tuple.apply_type_mapping_impl(db, type_mapping, TypeContext::default(), visitor)
        });

        Specialization::new(
            db,
            self.generic_context(db),
            types,
            self.materialization_kind(db),
            tuple_inner,
        )
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
        // TODO(jelle): specialization type?
        Specialization::new(db, self.generic_context(db), types, None, None)
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
        Self::new(
            db,
            context,
            types,
            self.materialization_kind(db),
            tuple_inner,
        )
    }

    pub(super) fn materialize_impl(
        self,
        db: &'db dyn Db,
        materialization_kind: MaterializationKind,
        visitor: &ApplyTypeMappingVisitor<'db>,
    ) -> Self {
        // The top and bottom materializations are fully static types already, so materializing them
        // further does nothing.
        if self.materialization_kind(db).is_some() {
            return self;
        }
        let mut has_dynamic_invariant_typevar = false;
        let types: Box<[_]> = self
            .generic_context(db)
            .variables(db)
            .zip(self.types(db))
            .map(|(bound_typevar, vartype)| {
                match bound_typevar.variance(db) {
                    TypeVarVariance::Bivariant => {
                        // With bivariance, all specializations are subtypes of each other,
                        // so any materialization is acceptable.
                        vartype.materialize(db, MaterializationKind::Top, visitor)
                    }
                    TypeVarVariance::Covariant => {
                        vartype.materialize(db, materialization_kind, visitor)
                    }
                    TypeVarVariance::Contravariant => {
                        vartype.materialize(db, materialization_kind.flip(), visitor)
                    }
                    TypeVarVariance::Invariant => {
                        let top_materialization =
                            vartype.materialize(db, MaterializationKind::Top, visitor);
                        if !vartype.is_equivalent_to(db, top_materialization) {
                            has_dynamic_invariant_typevar = true;
                        }
                        *vartype
                    }
                }
            })
            .collect();
        let tuple_inner = self.tuple_inner(db).and_then(|tuple| {
            // Tuples are immutable, so tuple element types are always in covariant position.
            tuple.apply_type_mapping_impl(
                db,
                &TypeMapping::Materialize(materialization_kind),
                TypeContext::default(),
                visitor,
            )
        });
        let new_materialization_kind = if has_dynamic_invariant_typevar {
            Some(materialization_kind)
        } else {
            None
        };
        Specialization::new(
            db,
            self.generic_context(db),
            types,
            new_materialization_kind,
            tuple_inner,
        )
    }

    pub(crate) fn has_relation_to_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        relation: TypeRelation,
        relation_visitor: &HasRelationToVisitor<'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
    ) -> ConstraintSet<'db> {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return ConstraintSet::from(false);
        }

        if let (Some(self_tuple), Some(other_tuple)) = (self.tuple_inner(db), other.tuple_inner(db))
        {
            return self_tuple.has_relation_to_impl(
                db,
                other_tuple,
                inferable,
                relation,
                relation_visitor,
                disjointness_visitor,
            );
        }

        let self_materialization_kind = self.materialization_kind(db);
        let other_materialization_kind = other.materialization_kind(db);

        let mut result = ConstraintSet::from(true);
        for ((bound_typevar, self_type), other_type) in (generic_context.variables(db))
            .zip(self.types(db))
            .zip(other.types(db))
        {
            // Subtyping/assignability of each type in the specialization depends on the variance
            // of the corresponding typevar:
            //   - covariant: verify that self_type <: other_type
            //   - contravariant: verify that other_type <: self_type
            //   - invariant: verify that self_type <: other_type AND other_type <: self_type
            //   - bivariant: skip, can't make subtyping/assignability false
            let compatible = match bound_typevar.variance(db) {
                TypeVarVariance::Invariant => has_relation_in_invariant_position(
                    db,
                    self_type,
                    self_materialization_kind,
                    other_type,
                    other_materialization_kind,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),
                TypeVarVariance::Covariant => self_type.has_relation_to_impl(
                    db,
                    *other_type,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),
                TypeVarVariance::Contravariant => other_type.has_relation_to_impl(
                    db,
                    *self_type,
                    inferable,
                    relation,
                    relation_visitor,
                    disjointness_visitor,
                ),
                TypeVarVariance::Bivariant => ConstraintSet::from(true),
            };
            if result.intersect(db, compatible).is_never_satisfied(db) {
                return result;
            }
        }

        result
    }

    pub(crate) fn is_equivalent_to_impl(
        self,
        db: &'db dyn Db,
        other: Specialization<'db>,
        inferable: InferableTypeVars<'_, 'db>,
        visitor: &IsEquivalentVisitor<'db>,
    ) -> ConstraintSet<'db> {
        if self.materialization_kind(db) != other.materialization_kind(db) {
            return ConstraintSet::from(false);
        }
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return ConstraintSet::from(false);
        }

        let mut result = ConstraintSet::from(true);
        for ((bound_typevar, self_type), other_type) in (generic_context.variables(db))
            .zip(self.types(db))
            .zip(other.types(db))
        {
            // Equivalence of each type in the specialization depends on the variance of the
            // corresponding typevar:
            //   - covariant: verify that self_type == other_type
            //   - contravariant: verify that other_type == self_type
            //   - invariant: verify that self_type == other_type
            //   - bivariant: skip, can't make equivalence false
            let compatible = match bound_typevar.variance(db) {
                TypeVarVariance::Invariant
                | TypeVarVariance::Covariant
                | TypeVarVariance::Contravariant => {
                    self_type.is_equivalent_to_impl(db, *other_type, inferable, visitor)
                }
                TypeVarVariance::Bivariant => ConstraintSet::from(true),
            };
            if result.intersect(db, compatible).is_never_satisfied(db) {
                return result;
            }
        }

        match (self.tuple_inner(db), other.tuple_inner(db)) {
            (Some(_), None) | (None, Some(_)) => return ConstraintSet::from(false),
            (None, None) => {}
            (Some(self_tuple), Some(other_tuple)) => {
                let compatible =
                    self_tuple.is_equivalent_to_impl(db, other_tuple, inferable, visitor);
                if result.intersect(db, compatible).is_never_satisfied(db) {
                    return result;
                }
            }
        }

        result
    }

    pub(crate) fn find_legacy_typevars_impl(
        self,
        db: &'db dyn Db,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        for ty in self.types(db) {
            ty.find_legacy_typevars_impl(db, binding_context, typevars, visitor);
        }
        // A tuple's specialization will include all of its element types, so we don't need to also
        // look in `self.tuple`.
    }

    /// Returns a copy of this specialization with the type at a given index replaced.
    pub(crate) fn with_replaced_type(
        self,
        db: &'db dyn Db,
        index: usize,
        new_type: Type<'db>,
    ) -> Self {
        let mut new_types: Box<[_]> = self.types(db).to_vec().into_boxed_slice();
        new_types[index] = new_type;

        Self::new(
            db,
            self.generic_context(db),
            new_types,
            self.materialization_kind(db),
            self.tuple_inner(db),
        )
    }
}

/// A mapping between type variables and types.
///
/// You will usually use [`Specialization`] instead of this type. This type is used when we need to
/// substitute types for type variables before we have fully constructed a [`Specialization`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub struct PartialSpecialization<'a, 'db> {
    generic_context: GenericContext<'db>,
    types: &'a [Type<'db>],
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
            .variables_inner(db)
            .get_index_of(&bound_typevar.identity(db))?;
        self.types.get(index).copied()
    }
}

/// Performs type inference between parameter annotations and argument types, producing a
/// specialization of a generic function.
pub(crate) struct SpecializationBuilder<'db> {
    db: &'db dyn Db,
    inferable: InferableTypeVars<'db, 'db>,
    types: FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,
}

impl<'db> SpecializationBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db, inferable: InferableTypeVars<'db, 'db>) -> Self {
        Self {
            db,
            inferable,
            types: FxHashMap::default(),
        }
    }

    pub(crate) fn build(
        &mut self,
        generic_context: GenericContext<'db>,
        tcx: TypeContext<'db>,
    ) -> Specialization<'db> {
        let tcx_specialization = tcx
            .annotation
            .and_then(|annotation| annotation.specialization_of(self.db, None));

        let types =
            (generic_context.variables_inner(self.db).iter()).map(|(identity, variable)| {
                let mut ty = self.types.get(identity).copied();

                // When inferring a specialization for a generic class typevar from a constructor call,
                // promote any typevars that are inferred as a literal to the corresponding instance type.
                if variable.should_promote_literals {
                    let tcx = tcx_specialization.and_then(|specialization| {
                        specialization.get(self.db, variable.bound_typevar)
                    });

                    ty = ty.map(|ty| ty.promote_literals(self.db, TypeContext::new(tcx)));
                }

                ty
            });

        // TODO Infer the tuple spec for a tuple type
        generic_context.specialize_partial(self.db, types)
    }

    fn add_type_mapping(&mut self, bound_typevar: BoundTypeVarInstance<'db>, ty: Type<'db>) {
        self.types
            .entry(bound_typevar.identity(self.db))
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
            && actual
                .when_subtype_of(self.db, formal, self.inferable)
                .is_always_satisfied(self.db)
        {
            return Ok(());
        }

        // Remove the union elements that are not related to `formal`.
        //
        // For example, if `formal` is `list[T]` and `actual` is `list[int] | None`, we want to specialize `T`
        // to `int`.
        let actual = actual.filter_disjoint_elements(self.db, formal, self.inferable);

        match (formal, actual) {
            // TODO: We haven't implemented a full unification solver yet. If typevars appear in
            // multiple union elements, we ideally want to express that _only one_ of them needs to
            // match, and that we should infer the smallest type mapping that allows that.
            //
            // For now, we punt on fully handling multiple typevar elements. Instead, we handle two
            // common cases specially:
            (Type::Union(formal_union), Type::Union(actual_union)) => {
                // First, if both formal and actual are unions, and precisely one formal union
                // element _is_ a typevar (not _contains_ a typevar), then we remove any actual
                // union elements that are a subtype of the formal (as a whole), and map the formal
                // typevar to any remaining actual union elements.
                //
                // In particular, this handles cases like
                //
                // ```py
                // def f[T](t: T | None) -> T: ...
                // def g[T](t: T | int | None) -> T | int: ...
                //
                // def _(x: str | None):
                //     reveal_type(f(x))  # revealed: str
                //
                // def _(y: str | int | None):
                //     reveal_type(g(x))  # revealed: str | int
                // ```
                // We do not handle cases where the `formal` types contain other types that contain type variables
                // to prevent incorrect specialization: e.g. `T = int | list[int]` for `formal: T | list[T], actual: int | list[int]`
                // (the correct specialization is `T = int`).
                let types_have_typevars = formal_union
                    .elements(self.db)
                    .iter()
                    .filter(|ty| ty.has_typevar(self.db));
                let Ok(Type::TypeVar(formal_bound_typevar)) = types_have_typevars.exactly_one()
                else {
                    return Ok(());
                };
                if (actual_union.elements(self.db).iter()).any(|ty| ty.is_type_var()) {
                    return Ok(());
                }
                let remaining_actual =
                    actual_union.filter(self.db, |ty| !ty.is_subtype_of(self.db, formal));
                if remaining_actual.is_never() {
                    return Ok(());
                }
                self.add_type_mapping(*formal_bound_typevar, remaining_actual);
            }
            (Type::Union(formal), _) => {
                // Second, if the formal is a union, and precisely one union element _is_ a typevar (not
                // _contains_ a typevar), then we add a mapping between that typevar and the actual
                // type. (Note that we've already handled above the case where the actual is
                // assignable to any _non-typevar_ union element.)
                let bound_typevars =
                    (formal.elements(self.db).iter()).filter_map(|ty| ty.as_typevar());
                if let Ok(bound_typevar) = bound_typevars.exactly_one() {
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

            (Type::TypeVar(bound_typevar), ty) | (ty, Type::TypeVar(bound_typevar))
                if bound_typevar.is_inferable(self.db, self.inferable) =>
            {
                match bound_typevar.typevar(self.db).bound_or_constraints(self.db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        if !ty
                            .when_assignable_to(self.db, bound, self.inferable)
                            .is_always_satisfied(self.db)
                        {
                            return Err(SpecializationError::MismatchedBound {
                                bound_typevar,
                                argument: ty,
                            });
                        }
                        self.add_type_mapping(bound_typevar, ty);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        for constraint in constraints.elements(self.db) {
                            if ty
                                .when_assignable_to(self.db, *constraint, self.inferable)
                                .is_always_satisfied(self.db)
                            {
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
