use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::fmt::Display;

use itertools::{Either, Itertools};
use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::semantic_index::definition::Definition;
use crate::semantic_index::scope::{FileScopeId, NodeWithScopeKind, ScopeId};
use crate::semantic_index::{SemanticIndex, semantic_index};
use crate::types::class::ClassType;
use crate::types::class_base::ClassBase;
use crate::types::constraints::{ConstraintSet, IteratorConstraintsExtension};
use crate::types::instance::{Protocol, ProtocolInstanceType};
use crate::types::signatures::Parameters;
use crate::types::tuple::{TupleSpec, TupleType, walk_tuple_type};
use crate::types::variance::VarianceInferable;
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance,
    ClassLiteral, FindLegacyTypeVarsVisitor, HasRelationToVisitor, IntersectionType,
    IsDisjointVisitor, IsEquivalentVisitor, KnownClass, KnownInstanceType, MaterializationKind,
    NormalizedVisitor, Type, TypeContext, TypeMapping, TypeRelation, TypeVarBoundOrConstraints,
    TypeVarIdentity, TypeVarInstance, TypeVarKind, TypeVarVariance, UnionType, declaration_type,
    walk_type_var_bounds,
};
use crate::{Db, FxOrderMap, FxOrderSet};

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
) -> Option<BoundTypeVarInstance<'db>> {
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
    pub(crate) fn merge(&'a self, other: &'a InferableTypeVars<'a, 'db>) -> Self {
        match (self, other) {
            (InferableTypeVars::None, other) | (other, InferableTypeVars::None) => *other,
            _ => InferableTypeVars::Two(self, other),
        }
    }

    // This is not an IntoIterator implementation because I have no desire to try to name the
    // iterator type.
    pub(crate) fn iter(self) -> impl Iterator<Item = BoundTypeVarIdentity<'db>> {
        match self {
            InferableTypeVars::None => Either::Left(Either::Left(std::iter::empty())),
            InferableTypeVars::One(typevars) => Either::Right(typevars.iter().copied()),
            InferableTypeVars::Two(left, right) => {
                let chained: Box<dyn Iterator<Item = BoundTypeVarIdentity<'db>>> =
                    Box::new(left.iter().chain(right.iter()));
                Either::Left(Either::Right(chained))
            }
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

/// A list of formal type variables for a generic function, class, or type alias.
///
/// # Ordering
/// Ordering is based on the context's salsa-assigned id and not on its values.
/// The id may change between runs, or when the context was garbage collected and recreated.
#[salsa::interned(debug, constructor=new_internal, heap_size=GenericContext::heap_size)]
#[derive(PartialOrd, Ord)]
pub struct GenericContext<'db> {
    #[returns(ref)]
    variables_inner: FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,
}

pub(super) fn walk_generic_context<'db, V: TypeVisitor<'db> + ?Sized>(
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
        Self::new_internal(
            db,
            type_params
                .into_iter()
                .map(|variable| (variable.identity(db), variable))
                .collect::<FxOrderMap<_, _>>(),
        )
    }

    /// Merge this generic context with another, returning a new generic context that
    /// contains type variables from both contexts.
    pub(crate) fn merge(self, db: &'db dyn Db, other: Self) -> Self {
        Self::from_typevar_instances(
            db,
            self.variables_inner(db)
                .values()
                .chain(other.variables_inner(db).values())
                .copied(),
        )
    }

    pub(crate) fn merge_optional(
        db: &'db dyn Db,
        left: Option<Self>,
        right: Option<Self>,
    ) -> Option<Self> {
        match (left, right) {
            (None, None) => None,
            (Some(one), None) | (None, Some(one)) => Some(one),
            (Some(left), Some(right)) => Some(left.merge(db, right)),
        }
    }

    pub(crate) fn remove_self(
        self,
        db: &'db dyn Db,
        binding_context: Option<BindingContext<'db>>,
    ) -> Self {
        Self::from_typevar_instances(
            db,
            self.variables(db).filter(|bound_typevar| {
                !(bound_typevar.typevar(db).is_self(db)
                    && binding_context.is_none_or(|binding_context| {
                        bound_typevar.binding_context(db) == binding_context
                    }))
            }),
        )
    }

    /// Returns the typevars that are inferable in this generic context. This set might include
    /// more typevars than the ones directly bound by the generic context. For instance, consider a
    /// method of a generic class:
    ///
    /// ```py
    /// class C[A]:
    ///     def method[T](self, t: T):
    /// ```
    ///
    /// In this example, `method`'s generic context binds `Self` and `T`, but its inferable set
    /// also includes `A@C`. This is needed because at each call site, we need to infer the
    /// specialized class instance type whose method is being invoked.
    pub(crate) fn inferable_typevars(self, db: &'db dyn Db) -> InferableTypeVars<'db, 'db> {
        #[derive(Default)]
        struct CollectTypeVars<'db> {
            typevars: RefCell<FxHashSet<BoundTypeVarIdentity<'db>>>,
            recursion_guard: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for CollectTypeVars<'db> {
            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_bound_type_var_type(
                &self,
                db: &'db dyn Db,
                bound_typevar: BoundTypeVarInstance<'db>,
            ) {
                self.typevars
                    .borrow_mut()
                    .insert(bound_typevar.identity(db));
                let typevar = bound_typevar.typevar(db);
                if let Some(bound_or_constraints) = typevar.bound_or_constraints(db) {
                    walk_type_var_bounds(db, bound_or_constraints, self);
                }
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
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
        self.variables_inner(db).values().copied()
    }

    /// Returns `true` if this generic context contains exactly one `ParamSpec` and no other type
    /// variables.
    ///
    /// For example:
    /// ```py
    /// class Foo[**P]: ...  # true
    /// class Bar[T, **P]: ...  # false
    /// class Baz[T]: ...  # false
    /// ```
    pub(crate) fn exactly_one_paramspec(self, db: &'db dyn Db) -> bool {
        self.variables(db)
            .exactly_one()
            .is_ok_and(|bound_typevar| bound_typevar.is_paramspec(db))
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
            ast::TypeParam::ParamSpec(node) => {
                let definition = index.expect_single_definition(node);
                let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
                    declaration_type(db, definition).inner_type()
                else {
                    return None;
                };
                Some(typevar.with_binding_context(db, binding_context))
            }
            // TODO: Support this!
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
    /// match the number of typevars in the generic context.
    ///
    /// You must provide a specific type for each typevar; no defaults are used. (Use
    /// [`specialize_partial`](Self::specialize_partial) if you might not have types for every
    /// typevar.)
    ///
    /// The types you provide should not mention any of the typevars in this generic context;
    /// otherwise, you will be left with a partial specialization. (Use
    /// [`specialize_recursive`](Self::specialize_recursive) if your types might mention typevars
    /// in this generic context.)
    pub(crate) fn specialize(
        self,
        db: &'db dyn Db,
        types: Box<[Type<'db>]>,
    ) -> Specialization<'db> {
        assert_eq!(self.len(db), types.len());
        Specialization::new(db, self, types, None, None)
    }

    /// Creates a specialization of this generic context. Panics if the length of `types` does not
    /// match the number of typevars in the generic context.
    ///
    /// If any provided type is `None`, we will use the corresponding typevar's default type. You
    /// are allowed to provide types that mention the typevars in this generic context.
    pub(crate) fn specialize_recursive<I>(self, db: &'db dyn Db, types: I) -> Specialization<'db>
    where
        I: IntoIterator<Item = Option<Type<'db>>>,
        I::IntoIter: ExactSizeIterator,
    {
        fn specialize_recursive_impl<'db>(
            db: &'db dyn Db,
            context: GenericContext<'db>,
            mut types: Box<[Type<'db>]>,
        ) -> Specialization<'db> {
            let len = types.len();
            loop {
                let mut any_changed = false;
                for i in 0..len {
                    let partial = PartialSpecialization {
                        generic_context: context,
                        types: &types,
                        // Don't recursively substitute type[i] in itself. Ideally, we could instead
                        // check if the result is self-referential after we're done applying the
                        // partial specialization. But when we apply a paramspec, we don't use the
                        // callable that it maps to directly; we create a new callable that reuses
                        // parts of it. That means we can't look for the previous type directly.
                        // Instead we use this to skip specializing the type in itself in the first
                        // place.
                        skip: Some(i),
                    };
                    let updated = types[i].apply_type_mapping(
                        db,
                        &TypeMapping::PartialSpecialization(partial),
                        TypeContext::default(),
                    );
                    if updated != types[i] {
                        types[i] = updated;
                        any_changed = true;
                    }
                }

                if !any_changed {
                    return Specialization::new(db, context, types, None, None);
                }
            }
        }

        let types = self.fill_in_defaults(db, types);
        specialize_recursive_impl(db, self, types)
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

    fn fill_in_defaults<I>(self, db: &'db dyn Db, types: I) -> Box<[Type<'db>]>
    where
        I: IntoIterator<Item = Option<Type<'db>>>,
        I::IntoIter: ExactSizeIterator,
    {
        let types = types.into_iter();
        let variables = self.variables(db);
        assert_eq!(self.len(db), types.len());

        // Typevars can have other typevars as their default values, e.g.
        //
        // ```py
        // class C[T, U = T]: ...
        // ```
        //
        // If there is a mapping for `T`, we want to map `U` to that type, not to `T`. To handle
        // this, we repeatedly apply the specialization to itself, until we reach a fixed point.
        let mut expanded = Vec::with_capacity(types.len());
        for typevar in variables.clone() {
            if typevar.is_paramspec(db) {
                expanded.push(Type::paramspec_value_callable(db, Parameters::unknown()));
            } else {
                expanded.push(Type::unknown());
            }
        }

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
                skip: None,
            };
            let default = default.apply_type_mapping(
                db,
                &TypeMapping::PartialSpecialization(partial),
                TypeContext::default(),
            );
            expanded[idx] = default;
        }

        expanded.into_boxed_slice()
    }

    /// Creates a specialization of this generic context. Panics if the length of `types` does not
    /// match the number of typevars in the generic context. If any provided type is `None`, we
    /// will use the corresponding typevar's default type.
    pub(crate) fn specialize_partial<I>(self, db: &'db dyn Db, types: I) -> Specialization<'db>
    where
        I: IntoIterator<Item = Option<Type<'db>>>,
        I::IntoIter: ExactSizeIterator,
    {
        Specialization::new(db, self, self.fill_in_defaults(db, types), None, None)
    }

    pub(crate) fn normalized_impl(self, db: &'db dyn Db, visitor: &NormalizedVisitor<'db>) -> Self {
        let variables = self
            .variables(db)
            .map(|bound_typevar| bound_typevar.normalized_impl(db, visitor));

        Self::from_typevar_instances(db, variables)
    }

    fn heap_size(
        (variables,): &(FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>,),
    ) -> usize {
        ruff_memory_usage::order_map_heap_size(variables)
    }
}

fn inferable_typevars_cycle_initial<'db>(
    _db: &'db dyn Db,
    _id: salsa::Id,
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

impl Display for LegacyGenericBase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An assignment of a specific type to each type variable in a generic scope.
///
/// TODO: Handle nested specializations better, with actual parent links to the specialization of
/// the lexically containing context.
///
/// # Ordering
/// Ordering is based on the context's salsa-assigned id and not on its values.
/// The id may change between runs, or when the context was garbage collected and recreated.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
#[derive(PartialOrd, Ord)]
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

pub(super) fn walk_specialization<'db, V: TypeVisitor<'db> + ?Sized>(
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
        // `Type::has_relation_to_impl`. Right now, we cannot generally
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
            is_subtype_of(base_bottom, derived_bottom)
                .and(db, || is_subtype_of(derived_bottom, base_top))
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
    relation: TypeRelation<'db>,
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
        (
            None,
            Some(base_mat),
            TypeRelation::Subtyping | TypeRelation::Redundancy | TypeRelation::SubtypingAssuming(_),
        ) => is_subtype_in_invariant_position(
            db,
            derived_type,
            MaterializationKind::Top,
            base_type,
            base_mat,
            inferable,
            relation_visitor,
            disjointness_visitor,
        ),
        (
            Some(derived_mat),
            None,
            TypeRelation::Subtyping | TypeRelation::Redundancy | TypeRelation::SubtypingAssuming(_),
        ) => is_subtype_in_invariant_position(
            db,
            derived_type,
            derived_mat,
            base_type,
            MaterializationKind::Bottom,
            inferable,
            relation_visitor,
            disjointness_visitor,
        ),
        // And A <~ B (assignability) is Bottom[A] <: Top[B]
        (
            None,
            Some(base_mat),
            TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability,
        ) => is_subtype_in_invariant_position(
            db,
            derived_type,
            MaterializationKind::Bottom,
            base_type,
            base_mat,
            inferable,
            relation_visitor,
            disjointness_visitor,
        ),
        (
            Some(derived_mat),
            None,
            TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability,
        ) => is_subtype_in_invariant_position(
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
            .zip(self.generic_context(db).variables(db))
            .enumerate()
            .map(|(i, (ty, typevar))| {
                let tcx = TypeContext::new(tcx.get(i).copied());
                if typevar.variance(db).is_covariant() {
                    ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                } else {
                    ty.apply_type_mapping_impl(db, &type_mapping.flip(), tcx, visitor)
                }
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
        assert_eq!(other.generic_context(db), generic_context);
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

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let types = if nested {
            self.types(db)
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, div, true))
                .collect::<Option<Box<[_]>>>()?
        } else {
            self.types(db)
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, div, true)
                        .unwrap_or(div)
                })
                .collect::<Box<[_]>>()
        };
        let tuple_inner = match self.tuple_inner(db) {
            Some(tuple) => Some(tuple.recursive_type_normalized_impl(db, div, nested)?),
            None => None,
        };
        let context = self.generic_context(db);
        Some(Self::new(
            db,
            context,
            types,
            self.materialization_kind(db),
            tuple_inner,
        ))
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
        relation: TypeRelation<'db>,
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

        let types = itertools::izip!(
            generic_context.variables(db),
            self.types(db),
            other.types(db)
        );

        types.when_all(db, |(bound_typevar, self_type, other_type)| {
            // Subtyping/assignability of each type in the specialization depends on the variance
            // of the corresponding typevar:
            //   - covariant: verify that self_type <: other_type
            //   - contravariant: verify that other_type <: self_type
            //   - invariant: verify that self_type <: other_type AND other_type <: self_type
            //   - bivariant: skip, can't make subtyping/assignability false
            match bound_typevar.variance(db) {
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
            }
        })
    }

    pub(crate) fn is_disjoint_from(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
    ) -> ConstraintSet<'db> {
        self.is_disjoint_from_impl(
            db,
            other,
            inferable,
            &IsDisjointVisitor::default(),
            &HasRelationToVisitor::default(),
        )
    }

    pub(crate) fn is_disjoint_from_impl(
        self,
        db: &'db dyn Db,
        other: Self,
        inferable: InferableTypeVars<'_, 'db>,
        disjointness_visitor: &IsDisjointVisitor<'db>,
        relation_visitor: &HasRelationToVisitor<'db>,
    ) -> ConstraintSet<'db> {
        let generic_context = self.generic_context(db);
        if generic_context != other.generic_context(db) {
            return ConstraintSet::from(true);
        }

        if let (Some(self_tuple), Some(other_tuple)) = (self.tuple_inner(db), other.tuple_inner(db))
        {
            return self_tuple.is_disjoint_from_impl(
                db,
                other_tuple,
                inferable,
                disjointness_visitor,
                relation_visitor,
            );
        }

        let types = itertools::izip!(
            generic_context.variables(db),
            self.types(db),
            other.types(db)
        );

        types.when_all(
            db,
            |(bound_typevar, self_type, other_type)| match bound_typevar.variance(db) {
                // TODO: This check can lead to false negatives.
                //
                // For example, `Foo[int]` and `Foo[bool]` are disjoint, even though `bool` is a subtype
                // of `int`. However, given two non-inferable type variables `T` and `U`, `Foo[T]` and
                // `Foo[U]` should not be considered disjoint, as `T` and `U` could be specialized to the
                // same type. We don't currently have a good typing relationship to represent this.
                TypeVarVariance::Invariant => self_type.is_disjoint_from_impl(
                    db,
                    *other_type,
                    inferable,
                    disjointness_visitor,
                    relation_visitor,
                ),

                // If `Foo[T]` is covariant in `T`, `Foo[Never]` is a subtype of `Foo[A]` and `Foo[B]`
                TypeVarVariance::Covariant => ConstraintSet::from(false),

                // If `Foo[T]` is contravariant in `T`, `Foo[A | B]` is a subtype of `Foo[A]` and `Foo[B]`
                TypeVarVariance::Contravariant => ConstraintSet::from(false),

                // If `Foo[T]` is bivariant in `T`, `Foo[A]` and `Foo[B]` are mutual subtypes.
                TypeVarVariance::Bivariant => ConstraintSet::from(false),
            },
        )
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
        for ((bound_typevar, self_type), other_type) in generic_context
            .variables(db)
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
}

/// A mapping between type variables and types.
///
/// You will usually use [`Specialization`] instead of this type. This type is used when we need to
/// substitute types for type variables before we have fully constructed a [`Specialization`].
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize)]
pub struct PartialSpecialization<'a, 'db> {
    generic_context: GenericContext<'db>,
    types: &'a [Type<'db>],
    /// An optional typevar to _not_ substitute when applying the specialization. We use this to
    /// avoid recursively substituting a type inside of itself.
    skip: Option<usize>,
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
        if self.skip.is_some_and(|skip| skip == index) {
            return Some(Type::Never);
        }
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

/// An assignment from a bound type variable to a given type, along with the variance of the outermost
/// type with respect to the type variable.
pub(crate) type TypeVarAssignment<'db> = (BoundTypeVarIdentity<'db>, TypeVarVariance, Type<'db>);

impl<'db> SpecializationBuilder<'db> {
    pub(crate) fn new(db: &'db dyn Db, inferable: InferableTypeVars<'db, 'db>) -> Self {
        Self {
            db,
            inferable,
            types: FxHashMap::default(),
        }
    }

    /// Returns the current set of type mappings for this specialization.
    pub(crate) fn type_mappings(&self) -> &FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>> {
        &self.types
    }

    /// Map the types that have been assigned in this specialization.
    pub(crate) fn mapped(
        &self,
        generic_context: GenericContext<'db>,
        f: impl Fn(BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>, Type<'db>) -> Type<'db>,
    ) -> Self {
        let mut types = self.types.clone();
        for (identity, variable) in generic_context.variables_inner(self.db) {
            if let Some(ty) = types.get_mut(identity) {
                *ty = f(*identity, *variable, *ty);
            }
        }

        Self {
            db: self.db,
            inferable: self.inferable,
            types,
        }
    }

    pub(crate) fn build(&mut self, generic_context: GenericContext<'db>) -> Specialization<'db> {
        let types = generic_context
            .variables_inner(self.db)
            .iter()
            .map(|(identity, _)| self.types.get(identity).copied());

        // TODO Infer the tuple spec for a tuple type
        generic_context.specialize_recursive(self.db, types)
    }

    fn add_type_mapping(
        &mut self,
        bound_typevar: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
        variance: TypeVarVariance,
        mut f: impl FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
    ) {
        let identity = bound_typevar.identity(self.db);
        let Some(ty) = f((identity, variance, ty)) else {
            return;
        };

        match self.types.entry(identity) {
            Entry::Occupied(mut entry) => {
                // TODO: The spec says that when a ParamSpec is used multiple times in a signature,
                // the type checker can solve it to a common behavioral supertype. We don't
                // implement that yet so in case there are multiple ParamSpecs, use the
                // specialization from the first occurrence.
                // https://github.com/astral-sh/ty/issues/1778
                // https://github.com/astral-sh/ruff/pull/21445#discussion_r2591510145
                if bound_typevar.is_paramspec(self.db) {
                    return;
                }
                *entry.get_mut() = UnionType::from_elements(self.db, [*entry.get(), ty]);
            }
            Entry::Vacant(entry) => {
                entry.insert(ty);
            }
        }
    }

    /// Finds all of the valid specializations of a constraint set, and adds their type mappings to
    /// the specialization that this builder is building up.
    ///
    /// `formal` should be the top-level formal parameter type that we are inferring. This is used
    /// by our literal promotion logic, which needs to know which typevars are affected by each
    /// argument, and the variance of those typevars in the corresponding parameter.
    ///
    /// TODO: This is a stopgap! Eventually, the builder will maintain a single constraint set for
    /// the main specialization that we are building, and [`build`][Self::build] will build the
    /// specialization directly from that constraint set. This method lets us migrate to that brave
    /// new world incrementally, by using the new constraint set mechanism piecemeal for certain
    /// type comparisons.
    fn add_type_mappings_from_constraint_set(
        &mut self,
        formal: Type<'db>,
        constraints: ConstraintSet<'db>,
        mut f: impl FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
    ) {
        #[derive(Default)]
        struct Bounds<'db> {
            lower: FxOrderSet<Type<'db>>,
            upper: FxOrderSet<Type<'db>>,
        }

        // Sort the constraints in each path by their `source_order`s, to ensure that we construct
        // any unions or intersections in our type mappings in a stable order. Constraints might
        // come out of `PathAssignment`s with identical `source_order`s, but if they do, those
        // "tied" constraints will still be ordered in a stable way. So we need a stable sort to
        // retain that stable per-tie ordering.
        let constraints = constraints.limit_to_valid_specializations(self.db);
        let mut sorted_paths = Vec::new();
        constraints.for_each_path(self.db, |path| {
            let mut path: Vec<_> = path.positive_constraints().collect();
            path.sort_by_key(|(_, source_order)| *source_order);
            sorted_paths.push(path);
        });
        sorted_paths.sort_by(|path1, path2| {
            let source_orders1 = path1.iter().map(|(_, source_order)| *source_order);
            let source_orders2 = path2.iter().map(|(_, source_order)| *source_order);
            source_orders1.cmp(source_orders2)
        });

        let mut mappings: FxHashMap<BoundTypeVarInstance<'db>, Bounds<'db>> = FxHashMap::default();
        for path in sorted_paths {
            mappings.clear();
            for (constraint, _) in path {
                let typevar = constraint.typevar(self.db);
                let lower = constraint.lower(self.db);
                let upper = constraint.upper(self.db);
                let bounds = mappings.entry(typevar).or_default();
                bounds.lower.insert(lower);
                bounds.upper.insert(upper);

                if let Type::TypeVar(lower_bound_typevar) = lower {
                    let bounds = mappings.entry(lower_bound_typevar).or_default();
                    bounds.upper.insert(Type::TypeVar(typevar));
                }

                if let Type::TypeVar(upper_bound_typevar) = upper {
                    let bounds = mappings.entry(upper_bound_typevar).or_default();
                    bounds.lower.insert(Type::TypeVar(typevar));
                }
            }

            for (bound_typevar, bounds) in mappings.drain() {
                let variance = formal.variance_of(self.db, bound_typevar);
                let upper = IntersectionType::from_elements(self.db, bounds.upper);
                if !upper.is_object() {
                    self.add_type_mapping(bound_typevar, upper, variance, &mut f);
                    continue;
                }
                let lower = UnionType::from_elements(self.db, bounds.lower);
                if !lower.is_never() {
                    self.add_type_mapping(bound_typevar, lower, variance, &mut f);
                }
            }
        }
    }

    /// Infer type mappings for the specialization based on a given type and its declared type.
    pub(crate) fn infer(
        &mut self,
        formal: Type<'db>,
        actual: Type<'db>,
    ) -> Result<(), SpecializationError<'db>> {
        self.infer_map(formal, actual, |(_, _, ty)| Some(ty))
    }

    /// Infer type mappings for the specialization based on a given type and its declared type.
    ///
    /// The provided function will be called before any type mappings are created, and can
    /// optionally modify the inferred type, or filter out the type mapping entirely.
    pub(crate) fn infer_map(
        &mut self,
        formal: Type<'db>,
        actual: Type<'db>,
        mut f: impl FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
    ) -> Result<(), SpecializationError<'db>> {
        self.infer_map_impl(formal, actual, TypeVarVariance::Covariant, &mut f)
    }

    fn infer_map_impl(
        &mut self,
        formal: Type<'db>,
        actual: Type<'db>,
        polarity: TypeVarVariance,
        mut f: &mut dyn FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
    ) -> Result<(), SpecializationError<'db>> {
        // TODO: Eventually, the builder will maintain a constraint set, instead of a hash-map of
        // type mappings, to represent the specialization that we are building up. At that point,
        // this method will just need to compare `actual ≤ formal`, using constraint set
        // assignability, and AND the result into the constraint set we are building.
        //
        // To make progress on that migration, we use constraint set assignability whenever
        // possible when adding any new heuristics here. See the `Callable` clause below for an
        // example.

        if formal == actual {
            return Ok(());
        }

        // Remove the union elements from `actual` that are not related to `formal`, and vice
        // versa.
        //
        // For example, if `formal` is `list[T]` and `actual` is `list[int] | None`, we want to
        // specialize `T` to `int`, and so ignore the `None`.
        let actual = actual.filter_disjoint_elements(self.db, formal, self.inferable);
        let formal = formal.filter_disjoint_elements(self.db, actual, self.inferable);

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
                if actual_union
                    .elements(self.db)
                    .iter()
                    .any(|ty| ty.is_type_var())
                {
                    return Ok(());
                }
                let remaining_actual =
                    actual_union.filter(self.db, |ty| !ty.is_subtype_of(self.db, formal));
                if remaining_actual.is_never() {
                    return Ok(());
                }
                self.add_type_mapping(*formal_bound_typevar, remaining_actual, polarity, f);
            }
            (Type::Union(union_formal), _) => {
                // Second, if the formal is a union, and the actual type is assignable to precisely
                // one union element, then we don't add any type mapping. This handles a case like
                //
                // ```py
                // def f[T](t: T | None) -> T: ...
                //
                // reveal_type(f(None))  # revealed: Unknown
                // ```
                //
                // without specializing `T` to `None`.
                if !actual.is_never() {
                    let assignable_elements = union_formal.elements(self.db).iter().filter(|ty| {
                        actual
                            .when_subtype_of(self.db, **ty, self.inferable)
                            .is_always_satisfied(self.db)
                    });
                    if assignable_elements.exactly_one().is_ok() {
                        return Ok(());
                    }
                }

                let mut bound_typevars = union_formal
                    .elements(self.db)
                    .iter()
                    .filter_map(|ty| ty.as_typevar());

                // TODO:
                // Handling more than one bare typevar is something that we can't handle yet.
                if bound_typevars.nth(1).is_some() {
                    return Ok(());
                }

                // Finally, if there are no bare typevars, we try to infer type mappings by
                // checking against each union element. This handles cases like
                // ```py
                // def f[T](t: P[T] | Q[T]) -> T: ...
                //
                // reveal_type(f(P[str]()))  # revealed: str
                // reveal_type(f(Q[int]()))  # revealed: int
                // ```
                let mut first_error = None;
                let mut found_matching_element = false;
                for formal_element in union_formal.elements(self.db) {
                    let result = self.infer_map_impl(*formal_element, actual, polarity, &mut f);
                    if let Err(err) = result {
                        first_error.get_or_insert(err);
                    } else {
                        // The recursive call to `infer_map_impl` may succeed even if the actual type is
                        // not assignable to the formal element.
                        if !actual
                            .when_assignable_to(self.db, *formal_element, self.inferable)
                            .is_never_satisfied(self.db)
                        {
                            found_matching_element = true;
                        }
                    }
                }

                if !found_matching_element && let Some(error) = first_error {
                    return Err(error);
                }
            }

            (Type::Intersection(formal), _) => {
                // The actual type must be assignable to every (positive) element of the
                // formal intersection, so we must infer type mappings for each of them. (The
                // actual type must also be disjoint from every negative element of the
                // intersection, but that doesn't help us infer any type mappings.)
                for positive in formal.iter_positive(self.db) {
                    self.infer_map_impl(positive, actual, polarity, f)?;
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
                        self.add_type_mapping(bound_typevar, ty, polarity, f);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(constraints)) => {
                        // Prefer an exact match first.
                        for constraint in constraints.elements(self.db) {
                            if ty == *constraint {
                                self.add_type_mapping(bound_typevar, ty, polarity, f);
                                return Ok(());
                            }
                        }

                        for constraint in constraints.elements(self.db) {
                            if ty
                                .when_assignable_to(self.db, *constraint, self.inferable)
                                .is_always_satisfied(self.db)
                            {
                                self.add_type_mapping(bound_typevar, *constraint, polarity, f);
                                return Ok(());
                            }
                        }
                        return Err(SpecializationError::MismatchedConstraint {
                            bound_typevar,
                            argument: ty,
                        });
                    }
                    _ => self.add_type_mapping(bound_typevar, ty, polarity, f),
                }
            }

            (Type::SubclassOf(subclass_of), ty) | (ty, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                let formal_instance = Type::TypeVar(subclass_of.into_type_var().unwrap());
                if let Some(actual_instance) = ty.to_instance(self.db) {
                    return self.infer_map_impl(formal_instance, actual_instance, polarity, f);
                }
            }

            (formal, Type::ProtocolInstance(actual_protocol)) => {
                // TODO: This will only handle protocol classes that explicit inherit
                // from other generic protocol classes by listing it as a base class.
                // To handle classes that implicitly implement a generic protocol, we
                // will need to check the types of the protocol members to be able to
                // infer the specialization of the protocol that the class implements.
                if let Some(actual_nominal) = actual_protocol.as_nominal_type() {
                    return self.infer_map_impl(
                        formal,
                        Type::NominalInstance(actual_nominal),
                        polarity,
                        f,
                    );
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
                    for (formal_element, actual_element) in formal_tuple
                        .all_elements()
                        .iter()
                        .zip(actual_tuple.all_elements())
                    {
                        let variance = TypeVarVariance::Covariant.compose(polarity);
                        self.infer_map_impl(*formal_element, *actual_element, variance, &mut f)?;
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
                        inner: Protocol::FromClass(class),
                        ..
                    }) => class.into_generic_alias(),
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
                        let generic_context = formal_alias
                            .specialization(self.db)
                            .generic_context(self.db)
                            .variables(self.db);
                        let formal_specialization =
                            formal_alias.specialization(self.db).types(self.db);
                        let base_specialization = base_alias.specialization(self.db).types(self.db);
                        for (typevar, formal_ty, base_ty) in itertools::izip!(
                            generic_context,
                            formal_specialization,
                            base_specialization
                        ) {
                            let variance = typevar.variance_with_polarity(self.db, polarity);
                            self.infer_map_impl(*formal_ty, *base_ty, variance, &mut f)?;
                        }
                        return Ok(());
                    }
                }
            }

            (Type::Callable(formal_callable), _) => {
                let Some(actual_callables) = actual.try_upcast_to_callable(self.db) else {
                    return Ok(());
                };

                let formal_callable = formal_callable.signatures(self.db);
                let formal_is_single_paramspec = formal_callable.is_single_paramspec().is_some();

                for actual_callable in actual_callables.as_slice() {
                    if formal_is_single_paramspec {
                        let when = actual_callable
                            .signatures(self.db)
                            .when_constraint_set_assignable_to(
                                self.db,
                                formal_callable,
                                self.inferable,
                            );
                        self.add_type_mappings_from_constraint_set(formal, when, &mut f);
                    } else {
                        for actual_signature in &actual_callable.signatures(self.db).overloads {
                            let when = actual_signature
                                .when_constraint_set_assignable_to_signatures(
                                    self.db,
                                    formal_callable,
                                    self.inferable,
                                );
                            self.add_type_mappings_from_constraint_set(formal, when, &mut f);
                        }
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
