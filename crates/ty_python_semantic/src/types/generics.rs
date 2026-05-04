use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry;
use std::fmt::Display;

use itertools::{Either, Itertools};
use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::callable::walk_callable_type;
use crate::types::class::ClassType;
use crate::types::class_base::ClassBase;
use crate::types::constraints::{
    ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension, OwnedConstraintSet,
    Solutions,
};
use crate::types::relation::{
    DisjointnessChecker, HasRelationToVisitor, IsDisjointVisitor, TypeRelation, TypeRelationChecker,
};
use crate::types::signatures::{CallableSignature, Parameters, SignatureRelationVisitor};
use crate::types::tuple::{TupleSpec, TupleType, walk_tuple_type};
use crate::types::type_alias::{walk_manual_pep_695_type_alias, walk_pep_695_type_alias};
use crate::types::typevar::{
    BoundTypeVarIdentity, TypeVarIdentity, TypeVarInstance, walk_type_var_bounds,
};
use crate::types::variance::VarianceInferable;
use crate::types::visitor::{TypeCollector, TypeVisitor, walk_type_with_recursion_guard};
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, CallableType, CallableTypes,
    ClassLiteral, FindLegacyTypeVarsVisitor, IntersectionType, KnownClass, KnownInstanceType,
    MaterializationKind, Type, TypeAliasType, TypeContext, TypeMapping, TypeVarBoundOrConstraints,
    TypeVarKind, TypeVarVariance, UnionAccumulator, UnionType, binding_type, declaration_type,
    infer_definition_types,
};
use crate::{Db, FxIndexMap, FxOrderMap, FxOrderSet};
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::node_key::NodeKey;
use ty_python_core::scope::{FileScopeId, NodeWithScopeKey, NodeWithScopeKind, ScopeId};
use ty_python_core::{SemanticIndex, semantic_index};

/// Returns an iterator of any generic context introduced by the given scope or any enclosing
/// scope.
pub(crate) fn enclosing_generic_contexts<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    scope: FileScopeId,
) -> impl Iterator<Item = GenericContext<'db>> {
    index
        .ancestor_scopes(scope)
        .filter_map(|(_, ancestor_scope)| GenericContext::of_node(db, ancestor_scope.node(), index))
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
    // typing.Self is treated like a legacy typevar, but doesn't follow the same scoping rules. It
    // is always bound to the outermost method in the nearest enclosing class. The walk looks for a
    // (function, class) pair in the scope hierarchy. The caller (`typing_self`) is responsible for
    // ensuring that `containing_scope` starts from the function body scope rather than the scope
    // where the function is defined, so that the function itself appears in the ancestor chain.
    //
    // We also match `FunctionTypeParameters` as a valid inner scope because for generic methods
    // (e.g., `def foo[T](self) -> Self`), the type-params scope sits between the function body
    // and the class body in the ancestor chain.
    if matches!(typevar.kind(db), TypeVarKind::TypingSelf) {
        for ((_, inner), (_, outer)) in index.ancestor_scopes(containing_scope).tuple_windows() {
            if outer.kind().is_class() {
                match inner.node() {
                    NodeWithScopeKind::Function(function)
                    | NodeWithScopeKind::FunctionTypeParameters(function) => {
                        let definition = index.expect_single_definition(function);
                        return Some(typevar.with_binding_context(db, definition));
                    }
                    _ => {}
                }
            }
        }

        // Handle `Self` directly in class body annotations (not inside a method).
        let scope = index.scope(containing_scope);
        if let Some(class_node) = scope.node().as_class() {
            let definition = index.expect_single_definition(class_node);
            return Some(typevar.with_binding_context(db, definition));
        }
    }
    // Walk ancestor scopes, tracking whether we've crossed a class scope boundary.
    // Class-scoped type variables are not visible from inner class scopes.
    let mut crossed_class_scope = false;
    for (_, ancestor_scope) in index.ancestor_scopes(containing_scope) {
        let is_class_scope = ancestor_scope.kind().is_class();
        // If we've already crossed a class boundary, skip class-scoped generic contexts.
        // This prevents inner classes from accessing type parameters of outer classes.
        if (!is_class_scope || !crossed_class_scope)
            && let Some(generic_context) = GenericContext::of_node(db, ancestor_scope.node(), index)
            && let Some(bound) = generic_context.binds_typevar(db, typevar)
        {
            return Some(bound);
        }
        if is_class_scope {
            crossed_class_scope = true;
        }
    }
    typevar_binding_context
        .map(|typevar_binding_context| typevar.with_binding_context(db, typevar_binding_context))
}

/// Create a `typing.Self` type variable for a given class.
pub(crate) fn typing_self<'db>(
    db: &'db dyn Db,
    scope_id: ScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    class: ClassLiteral<'db>,
) -> Option<BoundTypeVarInstance<'db>> {
    let file = scope_id.file(db);
    let index = semantic_index(db, file);

    let identity = TypeVarIdentity::new(
        db,
        ast::name::Name::new_static("Self"),
        // `Self` has a different upper bound dependent on the containing class,
        // so pointing to the definition of the symbol `typing.Self` itself is
        // not useful here. We could point to the class definition, but the full
        // range of the class definition is much larger than the full range of a
        // TypeVar would usually be, which leads to bugs like
        // https://github.com/astral-sh/ty/issues/2514. So we just pass `None`
        // for the definition field here.
        None,
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

    // The `bind_typevar` Self loop walks ancestor scopes looking for a (function, class) pair.
    // For this to work correctly, the walk must start from the function's own body scope, not the
    // scope where the function is defined (e.g., the class body), so that the function itself
    // appears in the ancestor chain. When `typevar_binding_context` is a function definition, we
    // use the function's body scope; otherwise we fall back to the passed-in scope.
    //
    // For example, given:
    //
    // ```python
    // class Outer:
    //     def method(self) -> None:
    //         class Inner:
    //             def get(self) -> Self: ...
    // ```
    //
    // Starting from `get`'s body scope, the ancestor chain is:
    //
    //   get body -> Inner class body -> method body -> Outer class body -> module
    //
    // The first (function, class) pair found is (get, Inner) -- correct.
    //
    // If we instead started from the scope where `get` is defined (i.e., the Inner class body),
    // the chain would be:
    //
    //   Inner class body -> method body -> Outer class body -> module
    //
    // and the first match would be (method, Outer) -- wrong.
    let containing_scope = typevar_binding_context
        .and_then(|def| {
            let DefinitionKind::Function(func_ref) = def.kind(db) else {
                return None;
            };
            Some(
                index.node_scope_by_key(NodeWithScopeKey::Function(NodeKey::from_node_ref(
                    func_ref,
                ))),
            )
        })
        .unwrap_or_else(|| scope_id.file_scope_id(db));

    bind_typevar(
        db,
        index,
        containing_scope,
        typevar_binding_context,
        typevar,
    )
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq, get_size2::GetSize, salsa::Update)]
pub(crate) enum InferableTypeVars<'db> {
    None,
    Some(InferableTypeVarsInner<'db>),
}

impl<'db> InferableTypeVars<'db> {
    pub(crate) fn from_typevars(
        db: &'db dyn Db,
        typevars: FxOrderSet<BoundTypeVarIdentity<'db>>,
    ) -> Self {
        if typevars.is_empty() {
            return InferableTypeVars::None;
        }
        Self::Some(InferableTypeVarsInner::new_internal(db, typevars))
    }
}

#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct InferableTypeVarsInner<'db> {
    #[returns(ref)]
    inferable: FxOrderSet<BoundTypeVarIdentity<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for InferableTypeVarsInner<'_> {}

impl<'db> BoundTypeVarIdentity<'db> {
    pub(crate) fn is_inferable(self, db: &'db dyn Db, inferable: InferableTypeVars<'db>) -> bool {
        match inferable {
            InferableTypeVars::None => false,
            InferableTypeVars::Some(inner) => inner.inferable(db).contains(&self),
        }
    }
}

impl<'db> BoundTypeVarInstance<'db> {
    pub(crate) fn is_inferable(self, db: &'db dyn Db, inferable: InferableTypeVars<'db>) -> bool {
        self.identity(db).is_inferable(db, inferable)
    }
}

#[salsa::tracked]
impl<'db> InferableTypeVars<'db> {
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    pub(crate) fn merge(self, db: &'db dyn Db, other: Self) -> Self {
        match (self, other) {
            (InferableTypeVars::None, other) | (other, InferableTypeVars::None) => other,
            (InferableTypeVars::Some(self_inner), InferableTypeVars::Some(other_inner)) => {
                let merged = self_inner.inferable(db) | other_inner.inferable(db);
                Self::Some(InferableTypeVarsInner::new_internal(db, merged))
            }
        }
    }

    // This is not an IntoIterator implementation because I have no desire to try to name the
    // iterator type.
    pub(crate) fn iter(
        self,
        db: &'db dyn Db,
    ) -> impl Iterator<Item = BoundTypeVarIdentity<'db>> + 'db {
        match self {
            InferableTypeVars::None => Either::Left(std::iter::empty()),
            InferableTypeVars::Some(inner) => Either::Right(inner.inferable(db).iter().copied()),
        }
    }

    // Keep this around for debugging purposes
    #[expect(dead_code)]
    pub(crate) fn display(&self, db: &'db dyn Db) -> impl Display {
        format!(
            "[{}]",
            self.iter(db)
                .map(|identity| identity.display(db))
                .format(", ")
        )
    }
}

/// A list of formal type variables for a generic function, class, or type alias.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
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

    pub(crate) fn of_node(
        db: &'db dyn Db,
        node: &NodeWithScopeKind,
        index: &SemanticIndex<'db>,
    ) -> Option<Self> {
        match node {
            NodeWithScopeKind::Class(class) => {
                let definition = index.expect_single_definition(class);
                binding_type(db, definition)
                    .as_class_literal()?
                    .generic_context(db)
            }
            NodeWithScopeKind::Function(function) => {
                let definition = index.expect_single_definition(function);
                infer_definition_types(db, definition)
                    .function_type(definition)?
                    .last_definition_signature(db)
                    .generic_context
            }
            NodeWithScopeKind::TypeAlias(type_alias) => {
                let definition = index.expect_single_definition(type_alias);
                binding_type(db, definition)
                    .as_type_alias()?
                    .as_pep_695_type_alias()?
                    .generic_context(db)
            }
            _ => None,
        }
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
    pub(crate) fn inferable_typevars(self, db: &'db dyn Db) -> InferableTypeVars<'db> {
        #[derive(Default)]
        struct CollectTypeVars<'db> {
            typevars: RefCell<FxOrderSet<BoundTypeVarIdentity<'db>>>,
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
            cycle_initial=|_, _, _| InferableTypeVars::None,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn inferable_typevars_inner<'db>(
            db: &'db dyn Db,
            generic_context: GenericContext<'db>,
        ) -> InferableTypeVars<'db> {
            let visitor = CollectTypeVars::default();
            for bound_typevar in generic_context.variables(db) {
                visitor.visit_bound_type_var_type(db, bound_typevar);
            }
            InferableTypeVars::from_typevars(db, visitor.typevars.into_inner())
        }

        inferable_typevars_inner(db, self)
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
        return_type: Type<'db>,
    ) -> Option<Self> {
        // Find all of the legacy typevars mentioned in the function signature.
        let mut variables = FxOrderSet::default();
        for param in parameters {
            param
                .annotated_type()
                .find_legacy_typevars(db, Some(definition), &mut variables);
            if let Some(ty) = param.default_type() {
                ty.find_legacy_typevars(db, Some(definition), &mut variables);
            }
        }
        return_type.find_legacy_typevars(db, Some(definition), &mut variables);

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

    pub(crate) fn remove_callable_only_typevars(
        db: &'db dyn Db,
        generic_context: Option<Self>,
        parameters: &Parameters<'db>,
        return_type: Type<'db>,
        function_definition: Definition<'db>,
    ) -> (Option<Self>, Type<'db>) {
        #[derive(Default)]
        struct TypeVarLocations<'db> {
            /// The set of typevars that appear somewhere other than in a `Callable` in the return
            /// type.
            found_outside_callable_return: FxHashSet<BoundTypeVarInstance<'db>>,
            /// A map containing all of the `Callable`s in the return type, along with the typevars
            /// that appear in each. (Note that at this point, we have not yet determined if those
            /// typevars _only_ appear there.)
            found_inside_callable_return:
                FxHashMap<CallableType<'db>, FxOrderSet<BoundTypeVarInstance<'db>>>,
        }

        impl<'db> TypeVarLocations<'db> {
            /// Returns a set of all of the typevars that _only_ appear in a `Callable` in the
            /// return type, along with a "replacement map" for those `Callable`s. (The key of the
            /// map will be a `Callable` as it originally appears in the return type — i.e., with
            /// no generic context. The corresponding value will be the updated `Callable` with a
            /// generic context.)
            fn finalize(
                self,
                db: &'db dyn Db,
                function_definition: Definition<'db>,
            ) -> (
                FxHashSet<BoundTypeVarInstance<'db>>,
                FxHashMap<CallableType<'db>, CallableType<'db>>,
            ) {
                let mut found_only_inside_callable_return = FxHashSet::default();
                let replacements = self
                    .found_inside_callable_return
                    .into_iter()
                    .filter_map(|(callable, mut bound_typevars)| {
                        // Only keep typevars that appear _only_ in this callable and are
                        // actually bound by this function. If we renamed typevars bound by an
                        // enclosing generic context (e.g., class typevars in a method), we'd
                        // disconnect them from class specialization.
                        bound_typevars.retain(|bound_typevar| {
                            !self.found_outside_callable_return.contains(bound_typevar)
                                && bound_typevar.binding_context(db).definition()
                                    == Some(function_definition)
                        });
                        if bound_typevars.is_empty() {
                            return None;
                        }

                        // We're going to use this later to trim the function's generic context. So
                        // it's important that we do this first, so that we're tracking the
                        // original, not-yet-renamed typevars.
                        found_only_inside_callable_return.extend(bound_typevars.iter().copied());

                        // Then create a new typevar, with a 'return suffix, for each of the
                        // typevars that only appear in this callable, and update the callable's
                        // signature (and generic context) to use those new typevars.
                        let typevar_replacements: FxIndexMap<_, _> = bound_typevars
                            .iter()
                            .map(|bound_typevar| {
                                (*bound_typevar, bound_typevar.with_name_suffix(db, "return"))
                            })
                            .collect();
                        let apply = ApplySpecialization::ReturnCallables(&typevar_replacements);
                        let signatures = callable.signatures(db).apply_type_mapping_impl(
                            db,
                            &TypeMapping::ApplySpecialization(apply),
                            TypeContext::default(),
                            &ApplyTypeMappingVisitor::default(),
                        );
                        let generic_context = GenericContext::from_typevar_instances(
                            db,
                            typevar_replacements.values().copied(),
                        );
                        let signatures =
                            signatures.with_inherited_generic_context(db, generic_context);
                        let replacement = CallableType::new(db, signatures, callable.kind(db));

                        Some((callable, replacement))
                    })
                    .collect();

                (found_only_inside_callable_return, replacements)
            }
        }

        /// A visitor that walks through the parameter and return type annotations, recording
        /// whether each typevar appears inside and/or outside of a return type `Callable`.
        #[derive(Default)]
        struct FindTypeVarLocations<'db> {
            locations: RefCell<TypeVarLocations<'db>>,
            recursion_guard: TypeCollector<'db>,
            in_return_type: bool,
            in_callable_type: Cell<Option<CallableType<'db>>>,
        }

        impl<'db> TypeVisitor<'db> for FindTypeVarLocations<'db> {
            fn should_visit_lazy_type_attributes(&self) -> bool {
                false
            }

            fn visit_bound_type_var_type(
                &self,
                db: &'db dyn Db,
                bound_typevar: BoundTypeVarInstance<'db>,
            ) {
                let bound_typevar = if bound_typevar.is_paramspec(db) {
                    bound_typevar.without_paramspec_attr(db)
                } else {
                    bound_typevar
                };

                let mut locations = self.locations.borrow_mut();
                if self.in_return_type
                    && let Some(callable) = self.in_callable_type.get()
                {
                    locations
                        .found_inside_callable_return
                        .entry(callable)
                        .or_default()
                        .insert(bound_typevar);
                } else {
                    locations
                        .found_outside_callable_return
                        .insert(bound_typevar);
                }
            }

            fn visit_callable_type(&self, db: &'db dyn Db, callable: CallableType<'db>) {
                // Note: We only consider the outermost Callables in the return type.
                if self.in_return_type && self.in_callable_type.get().is_none() {
                    self.in_callable_type.set(Some(callable));
                    walk_callable_type(db, callable, self);
                    self.in_callable_type.set(None);
                } else {
                    walk_callable_type(db, callable, self);
                }
            }

            fn visit_type_alias_type(&self, db: &'db dyn Db, type_alias: TypeAliasType<'db>) {
                // The default implementation would do this for us if we returned `true` from
                // `should_visit_lazy_type_attributes`. However, this is the _only_ lazy type
                // attribute that we want to recurse into, so we do it by hand.
                match type_alias {
                    TypeAliasType::PEP695(type_alias) => {
                        walk_pep_695_type_alias(db, type_alias, self);
                    }
                    TypeAliasType::ManualPEP695(type_alias) => {
                        walk_manual_pep_695_type_alias(db, type_alias, self);
                    }
                }
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        // If the function in question is not generic, then there are no typevars, and we don't
        // have to worry about which ones appear in return type Callables.
        let Some(generic_context) = generic_context else {
            return (None, return_type);
        };

        // Find whether each typevar appears inside and/or outside a return type Callable.
        let mut find_typevar_locations = FindTypeVarLocations::default();
        for param in parameters {
            find_typevar_locations.visit_type(db, param.annotated_type());
        }
        find_typevar_locations.in_return_type = true;
        find_typevar_locations.visit_type(db, return_type);

        // Then update those return type Callables to be generic, with their generic context
        // containing the typevars that don't appear outside any return type Callable.
        let (found_only_inside_callable_return, replacements) = find_typevar_locations
            .locations
            .into_inner()
            .finalize(db, function_definition);
        let type_mapping = TypeMapping::RescopeReturnCallables(&replacements);
        let return_type = return_type.apply_type_mapping(db, &type_mapping, TypeContext::default());

        // And lastly remove those typevars from the function's generic context.
        let mut kept_typevars = generic_context
            .variables(db)
            .filter(|bound_typevar| !found_only_inside_callable_return.contains(bound_typevar))
            .peekable();
        let generic_context = if kept_typevars.peek().is_none() {
            None
        } else {
            Some(GenericContext::from_typevar_instances(db, kept_typevars))
        };

        (generic_context, return_type)
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
        let types: Vec<Type> = self.variables(db).map(Type::TypeVar).collect();
        self.specialize(db, types)
    }

    pub(crate) fn unknown_specialization(self, db: &'db dyn Db) -> Specialization<'db> {
        match self.len(db) {
            0 => self.specialize(db, &[]),
            1 => self.specialize(db, &[Type::unknown(); 1]),
            2 => self.specialize(db, &[Type::unknown(); 2]),
            len => self.specialize(db, vec![Type::unknown(); len]),
        }
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
    pub(crate) fn specialize<'t, T>(self, db: &'db dyn Db, types: T) -> Specialization<'db>
    where
        T: Into<Cow<'t, [Type<'db>]>>,
        'db: 't,
    {
        let types = types.into();

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
        let types = self.fill_in_defaults(db, types);
        self.specialize_from_types_recursive(db, types)
    }

    /// Builds a specialization and recursively resolves references between the chosen types.
    fn specialize_from_types_recursive(
        self,
        db: &'db dyn Db,
        mut types: Box<[Type<'db>]>,
    ) -> Specialization<'db> {
        let len = types.len();
        let variables = self.variables(db).collect_vec();
        loop {
            let mut any_changed = false;
            for i in 0..len {
                // Preserve identity mappings for unresolved type variables.
                if types[i] == Type::TypeVar(variables[i]) {
                    continue;
                }

                let specialization = ApplySpecialization::Partial {
                    generic_context: self,
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
                    &TypeMapping::ApplySpecialization(specialization),
                    TypeContext::default(),
                );
                if updated != types[i] {
                    types[i] = updated;
                    any_changed = true;
                }
            }

            if !any_changed {
                return Specialization::new(db, self, types, None, None);
            }
        }
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
            let specialization = ApplySpecialization::Partial {
                generic_context: self,
                types: &expanded[0..idx],
                skip: None,
            };
            let default = default.apply_type_mapping(
                db,
                &TypeMapping::ApplySpecialization(specialization),
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
        let new_specialization = self.apply_type_mapping(
            db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Specialization(other)),
        );
        match other.materialization_kind(db) {
            None => new_specialization,
            Some(materialization_kind) => new_specialization.materialize_impl(
                db,
                materialization_kind,
                &ApplyTypeMappingVisitor::default(),
            ),
        }
    }

    pub(crate) fn with_materialization_kind(
        self,
        db: &'db dyn Db,
        materialization_kind: Option<MaterializationKind>,
    ) -> Self {
        Specialization::new(
            db,
            self.generic_context(db),
            self.types(db),
            materialization_kind,
            self.tuple_inner(db),
        )
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
        // was of type Unknown. It's also wrong in case a typevar has a default, in which case it
        // may fail to specialize, but not end up as `Unknown`. We should add a bitset or similar
        // to Specialization that explicitly tells us which typevars are mapped.
        let types: Box<[_]> = self
            .types(db)
            .iter()
            .zip(other.types(db))
            .map(|(self_type, other_type)| match (self_type, other_type) {
                (unknown, known) | (known, unknown) if unknown.is_unknown() => *known,
                _ => UnionType::from_two_elements(db, *self_type, *other_type),
            })
            .collect();
        // TODO: Combine the tuple specs too
        // TODO(jelle): specialization type?
        Specialization::new(db, self.generic_context(db), types, None, None)
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
                match specialization_variance(db, bound_typevar) {
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
                        if !visitor.is_equivalent_to_materialization(
                            db,
                            *vartype,
                            top_materialization,
                        ) {
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

    pub(crate) fn is_disjoint_from<'c>(
        self,
        db: &'db dyn Db,
        other: Self,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::default();
        let checker = DisjointnessChecker::new(
            constraints,
            inferable,
            &relation_visitor,
            &disjointness_visitor,
            &signature_relation_visitor,
            &materialization_visitor,
        );
        checker.check_specialization_pair(db, self, other)
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

impl<'c, 'db> TypeRelationChecker<'_, 'c, 'db> {
    pub(super) fn check_specialization_pair(
        &self,
        db: &'db dyn Db,
        source: Specialization<'db>,
        target: Specialization<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let generic_context = source.generic_context(db);
        if generic_context != target.generic_context(db) {
            return self.never();
        }

        if let (Some(source_tuple), Some(target_tuple)) =
            (source.tuple_inner(db), target.tuple_inner(db))
        {
            return self.check_tuple_type_pair(db, source_tuple, target_tuple);
        }

        let source_materialization_kind = source.materialization_kind(db);
        let target_materialization_kind = target.materialization_kind(db);

        let types = itertools::izip!(
            generic_context.variables(db),
            source.types(db),
            target.types(db)
        );

        types.when_all(
            db,
            self.constraints,
            |(bound_typevar, source_type, target_type)| {
                // Subtyping/assignability of each type in the specialization depends on the variance
                // of the corresponding typevar:
                //   - covariant: verify that source_type <: target_type
                //   - contravariant: verify that target_type <: source_type
                //   - invariant: verify that source_type <: target_type AND target_type <: source_type
                //   - bivariant: skip, can't make subtyping/assignability false
                match specialization_variance(db, bound_typevar) {
                    TypeVarVariance::Invariant => self.check_relation_in_invariant_position(
                        db,
                        *source_type,
                        source_materialization_kind,
                        *target_type,
                        target_materialization_kind,
                    ),
                    TypeVarVariance::Covariant => {
                        self.check_type_pair(db, *source_type, *target_type)
                    }
                    TypeVarVariance::Contravariant => {
                        self.check_type_pair(db, *target_type, *source_type)
                    }
                    TypeVarVariance::Bivariant => self.always(),
                }
            },
        )
    }

    /// Whether two types encountered in an invariant position
    /// have a relation (subtyping or assignability), taking into account
    /// that the two types may come from a top or bottom materialization.
    fn check_relation_in_invariant_position(
        &self,
        db: &'db dyn Db,
        source_type: Type<'db>,
        source_materialization: Option<MaterializationKind>,
        target_type: Type<'db>,
        target_materialization: Option<MaterializationKind>,
    ) -> ConstraintSet<'db, 'c> {
        match (
            source_materialization,
            target_materialization,
            self.relation,
        ) {
            // Top and bottom materializations are fully static types, so subtyping
            // is the same as assignability.
            (Some(source_mat), Some(target_mat), _) => self.check_subtyping_in_invariant_position(
                db,
                source_type,
                source_mat,
                target_type,
                target_mat,
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
            (None, None, _) => {
                self.check_type_pair(db, target_type, source_type)
                    .and(db, self.constraints, || {
                        self.check_type_pair(db, source_type, target_type)
                    })
            }
            // For gradual types, A <: B (subtyping) is defined as Top[A] <: Bottom[B]
            (
                None,
                Some(target_mat),
                TypeRelation::Subtyping
                | TypeRelation::Redundancy { .. }
                | TypeRelation::SubtypingAssuming,
            ) => self.check_subtyping_in_invariant_position(
                db,
                source_type,
                MaterializationKind::Top,
                target_type,
                target_mat,
            ),
            (
                Some(source_mat),
                None,
                TypeRelation::Subtyping
                | TypeRelation::Redundancy { .. }
                | TypeRelation::SubtypingAssuming,
            ) => self.check_subtyping_in_invariant_position(
                db,
                source_type,
                source_mat,
                target_type,
                MaterializationKind::Bottom,
            ),
            // And A <~ B (assignability) is Bottom[A] <: Top[B]
            (
                None,
                Some(target_mat),
                TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability,
            ) => self.check_subtyping_in_invariant_position(
                db,
                source_type,
                MaterializationKind::Bottom,
                target_type,
                target_mat,
            ),
            (
                Some(source_mat),
                None,
                TypeRelation::Assignability | TypeRelation::ConstraintSetAssignability,
            ) => self.check_subtyping_in_invariant_position(
                db,
                source_type,
                source_mat,
                target_type,
                MaterializationKind::Top,
            ),
        }
    }

    fn check_subtyping_in_invariant_position(
        &self,
        db: &'db dyn Db,
        source_type: Type<'db>,
        source_materialization: MaterializationKind,
        target_type: Type<'db>,
        target_materialization: MaterializationKind,
    ) -> ConstraintSet<'db, 'c> {
        let source_top =
            source_type.materialize(db, MaterializationKind::Top, self.materialization_visitor);
        let source_bottom = source_type.materialize(
            db,
            MaterializationKind::Bottom,
            self.materialization_visitor,
        );
        let target_top =
            target_type.materialize(db, MaterializationKind::Top, self.materialization_visitor);
        let target_bottom = target_type.materialize(
            db,
            MaterializationKind::Bottom,
            self.materialization_visitor,
        );

        let is_subtype_of = |source: Type<'db>, target: Type<'db>| {
            // TODO:
            // This should be removed and properly handled in the respective
            // `(Type::TypeVar(_), _) | (_, Type::TypeVar(_))` branch of
            // `TypeRelationChecker::check_type_pair`. Right now, we cannot generally
            // return `self.always()` from that branch, as that leads to union
            // simplification, which means that we lose track of type variables
            // without recording the constraints under which the relation holds.
            if matches!(target, Type::TypeVar(_)) || matches!(source, Type::TypeVar(_)) {
                return self.always();
            }

            self.check_type_pair(db, source, target)
        };
        match (source_materialization, target_materialization) {
            // `source` is a subtype of `target` if the range of materializations covered by `source`
            // is a subset of the range covered by `target`.
            (MaterializationKind::Top, MaterializationKind::Top) => {
                is_subtype_of(target_bottom, source_bottom).and(db, self.constraints, || {
                    is_subtype_of(source_top, target_top)
                })
            }
            // One bottom is a subtype of another if it covers a strictly larger set of materializations.
            (MaterializationKind::Bottom, MaterializationKind::Bottom) => {
                is_subtype_of(source_bottom, target_bottom).and(db, self.constraints, || {
                    is_subtype_of(target_top, source_top)
                })
            }
            // The bottom materialization of `source` is a subtype of the top materialization
            // of `target` if there is some type that is both within the
            // range of types covered by derived and within the range covered by base, because if such a type
            // exists, it's a subtype of `Top[target]` and a supertype of `Bottom[source]`.
            (MaterializationKind::Bottom, MaterializationKind::Top) => {
                is_subtype_of(target_bottom, source_bottom)
                    .and(db, self.constraints, || {
                        is_subtype_of(source_bottom, target_top)
                    })
                    .or(db, self.constraints, || {
                        is_subtype_of(target_bottom, source_top).and(db, self.constraints, || {
                            is_subtype_of(source_top, target_top)
                        })
                    })
                    .or(db, self.constraints, || {
                        is_subtype_of(target_top, source_top).and(db, self.constraints, || {
                            is_subtype_of(source_bottom, target_top)
                        })
                    })
            }
            // A top materialization is a subtype of a bottom materialization only if both original
            // un-materialized types are the same fully static type.
            (MaterializationKind::Top, MaterializationKind::Bottom) => {
                is_subtype_of(source_top, target_bottom).and(db, self.constraints, || {
                    is_subtype_of(target_top, source_bottom)
                })
            }
        }
    }
}

fn specialization_variance<'db>(
    db: &'db dyn Db,
    bound_typevar: BoundTypeVarInstance<'db>,
) -> TypeVarVariance {
    let variance = bound_typevar.variance(db);
    if bound_typevar.is_paramspec(db) {
        // `ParamSpec` specializations are represented as callable-shaped values. Their relation
        // and materialization already use callable parameter contravariance, so flip the generic
        // variance here to avoid applying that direction twice.
        variance.flip()
    } else {
        variance
    }
}

impl<'c, 'db> DisjointnessChecker<'_, 'c, 'db> {
    pub(super) fn check_specialization_pair(
        &self,
        db: &'db dyn Db,
        left: Specialization<'db>,
        right: Specialization<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let generic_context = left.generic_context(db);
        if generic_context != right.generic_context(db) {
            return self.always();
        }

        if let (Some(left_tuple), Some(right_tuple)) = (left.tuple_inner(db), right.tuple_inner(db))
        {
            return self.check_tuple_type_pair(db, left_tuple, right_tuple);
        }

        let types = itertools::izip!(
            generic_context.variables(db),
            left.types(db),
            right.types(db)
        );

        types.when_all(
            db,
            self.constraints,
            |(bound_typevar, left_type, right_type)| match bound_typevar.variance(db) {
                // TODO: This check can lead to false negatives.
                //
                // For example, `Foo[int]` and `Foo[bool]` are disjoint, even though `bool` is a subtype
                // of `int`. However, given two non-inferable type variables `T` and `U`, `Foo[T]` and
                // `Foo[U]` should not be considered disjoint, as `T` and `U` could be specialized to the
                // same type. We don't currently have a good typing relationship to represent this.
                TypeVarVariance::Invariant => self.check_type_pair(db, *left_type, *right_type),

                // If `Foo[T]` is covariant in `T`, `Foo[Never]` is a subtype of `Foo[A]` and `Foo[B]`
                TypeVarVariance::Covariant => self.never(),

                // If `Foo[T]` is contravariant in `T`, `Foo[A | B]` is a subtype of `Foo[A]` and `Foo[B]`
                TypeVarVariance::Contravariant => self.never(),

                // If `Foo[T]` is bivariant in `T`, `Foo[A]` and `Foo[B]` are mutual subtypes.
                TypeVarVariance::Bivariant => self.never(),
            },
        )
    }
}

/// A mapping between type variables and types.
///
/// You will usually use [`Specialization`] instead of this type. This type is used when we need to
/// substitute types for type variables before we have fully constructed a [`Specialization`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, get_size2::GetSize)]
pub enum ApplySpecialization<'a, 'db> {
    Specialization(Specialization<'db>),
    Partial {
        generic_context: GenericContext<'db>,
        types: &'a [Type<'db>],
        /// An optional typevar to _not_ substitute when applying the specialization. We use this to
        /// avoid recursively substituting a type inside of itself.
        skip: Option<usize>,
    },
    ReturnCallables(&'a FxIndexMap<BoundTypeVarInstance<'db>, BoundTypeVarInstance<'db>>),
    /// Maps a single typevar to a concrete type. Used by the constraint set's sequent map to
    /// substitute a typevar nested inside another constraint's bound.
    Single(BoundTypeVarInstance<'db>, Type<'db>),
}

impl<'db> ApplySpecialization<'_, 'db> {
    /// Returns the type that a typevar is mapped to, or None if the typevar isn't part of this
    /// mapping.
    pub(crate) fn get(
        &self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
    ) -> Option<Type<'db>> {
        match self {
            ApplySpecialization::Specialization(specialization) => {
                specialization.get(db, bound_typevar)
            }
            ApplySpecialization::Partial {
                generic_context,
                types,
                skip,
            } => {
                let index = generic_context
                    .variables_inner(db)
                    .get_index_of(&bound_typevar.identity(db))?;
                if skip.is_some_and(|skip| skip == index) {
                    return Some(Type::Never);
                }
                types.get(index).copied()
            }
            ApplySpecialization::ReturnCallables(replacements) => {
                replacements.get(&bound_typevar).copied().map(Type::TypeVar)
            }
            ApplySpecialization::Single(typevar, ty) => {
                if bound_typevar.is_same_typevar_as(db, *typevar) {
                    Some(*ty)
                } else {
                    None
                }
            }
        }
    }

    /// Convert this specialization mapping to a concrete specialization over its own generic
    /// context, preserving skipped type variables in partial specializations as identity mappings.
    pub(crate) fn as_specialization(self, db: &'db dyn Db) -> Option<Specialization<'db>> {
        match self {
            ApplySpecialization::Specialization(specialization) => Some(specialization),
            ApplySpecialization::Partial {
                generic_context,
                types,
                skip,
            } => Some(
                generic_context.specialize(
                    db,
                    generic_context
                        .variables(db)
                        .enumerate()
                        .map(|(index, bound_typevar)| {
                            if skip.is_some_and(|skip| skip == index) {
                                Type::TypeVar(bound_typevar)
                            } else {
                                types
                                    .get(index)
                                    .copied()
                                    .unwrap_or(Type::TypeVar(bound_typevar))
                            }
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
            ApplySpecialization::ReturnCallables(_) | ApplySpecialization::Single(_, _) => None,
        }
    }
}

impl<'db> Type<'db> {
    pub(crate) fn substitute_one_typevar(
        self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping(
            db,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Single(
                bound_typevar,
                replacement,
            )),
            TypeContext::default(),
        )
    }
}

/// Performs type inference between parameter annotations and argument types, producing a
/// specialization of a generic function.
pub(crate) struct SpecializationBuilder<'db, 'c> {
    db: &'db dyn Db,
    constraints: &'c ConstraintSetBuilder<'db>,
    inferable: InferableTypeVars<'db>,
    types: FxHashMap<BoundTypeVarIdentity<'db>, UnionAccumulator<'db>>,
}

/// An assignment from a bound type variable to a given type, along with the variance of the outermost
/// type with respect to the type variable.
pub(crate) type TypeVarAssignment<'db> = (BoundTypeVarIdentity<'db>, TypeVarVariance, Type<'db>);

impl<'db, 'c> SpecializationBuilder<'db, 'c> {
    pub(crate) fn new(
        db: &'db dyn Db,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: InferableTypeVars<'db>,
    ) -> Self {
        Self {
            db,
            constraints,
            inferable,
            types: FxHashMap::default(),
        }
    }

    /// Build a specialization, using a caller-provided hook to select the solution for each
    /// typevar.
    ///
    /// The `choose` hook is called for each typevar on the generic context with the typevar's
    /// materialized lower and upper bounds. Currently, both bounds are set to the inferred type
    /// (representing an equality constraint). Unmapped typevars have bounds of `None,` and fallback
    /// to their default specialization if an alternative default type is not chosen.
    ///
    /// The hook should return `Some(ty)` to use `ty` as the specialization for this typevar, or
    /// `None` to use the inferred type unchanged.
    pub(crate) fn build_with(
        &mut self,
        generic_context: GenericContext<'db>,
        mut choose: impl FnMut(
            BoundTypeVarInstance<'db>,
            Option<(Type<'db>, Type<'db>)>,
        ) -> Option<Type<'db>>,
    ) -> Specialization<'db> {
        let db = self.db;
        let types = generic_context
            .variables_inner(db)
            .iter()
            .map(|(identity, variable)| {
                match self.types.get_mut(identity) {
                    Some(mapped_ty) => {
                        let mapped_ty = mapped_ty.get_or_build(db);
                        // The typevar was inferred — present both bounds as the inferred type.
                        let chosen = choose(*variable, Some((mapped_ty, mapped_ty)));
                        Some(chosen.unwrap_or(mapped_ty))
                    }
                    None => choose(*variable, None),
                }
            });

        generic_context.specialize_recursive(db, types)
    }

    /// Insert a type mapping for a bound typevar.
    ///
    /// If a mapping already exists for this typevar, the new type is unioned with the existing
    /// one.
    pub(crate) fn insert_type_mapping(
        &mut self,
        bound_typevar: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) {
        let identity = bound_typevar.identity(self.db);
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

                entry.get_mut().add(self.db, ty);
            }
            Entry::Vacant(entry) => {
                entry.insert(UnionAccumulator::new(ty));
            }
        }
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
        self.insert_type_mapping(bound_typevar, ty);
    }

    /// Finds all of the valid specializations of a constraint set, and adds their type mappings to
    /// the specialization that this builder is building up.
    ///
    /// `formal` should be the top-level formal parameter type that we are inferring. This is used
    /// by our promotion logic, which needs to know which typevars are affected by each
    /// argument, and the variance of those typevars in the corresponding parameter.
    ///
    /// TODO: This is a stopgap! Eventually, the builder will maintain a single constraint set for
    /// the main specialization that we are building, and [`build_with`][Self::build_with] will
    /// build the specialization directly from that constraint set. This method lets us migrate to
    /// that brave new world incrementally, by using the new constraint set mechanism piecemeal for
    /// certain type comparisons.
    fn add_type_mappings_from_constraint_set(
        &mut self,
        formal: Type<'db>,
        set: ConstraintSet<'db, 'c>,
        mut f: impl FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
    ) -> Result<(), ()> {
        let set = set.remove_noninferable(self.db, self.constraints, self.inferable);
        let solutions = match set.solutions(self.db, self.constraints) {
            Solutions::Unsatisfiable => return Err(()),
            Solutions::Unconstrained => return Ok(()),
            Solutions::Constrained(solutions) => solutions,
        };
        for solution in solutions {
            for binding in solution {
                let variance = formal.variance_of(self.db, binding.bound_typevar);
                self.add_type_mapping(binding.bound_typevar, binding.solution, variance, &mut f);
            }
        }
        Ok(())
    }

    fn add_type_mappings_from_owned_constraint_set(
        &mut self,
        formal: Type<'db>,
        set: &'db OwnedConstraintSet<'db>,
        f: impl FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
    ) -> Result<(), ()> {
        let set = self.constraints.load(self.db, set);
        self.add_type_mappings_from_constraint_set(formal, set, f)
    }

    /// Infer type mappings by comparing formal callable signatures against actual callables.
    fn infer_from_callable_signature(
        &mut self,
        formal: Type<'db>,
        formal_signature: &CallableSignature<'db>,
        actual_callables: &CallableTypes<'db>,
        f: &mut dyn FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
    ) -> Result<(), ()> {
        let formal_is_single_paramspec = formal_signature.is_single_paramspec().is_some();

        for actual_callable in actual_callables.as_slice() {
            if formal_is_single_paramspec {
                let when = actual_callable
                    .signatures(self.db)
                    .when_constraint_set_assignable_to(self.db, formal_signature, self.constraints);
                self.add_type_mappings_from_constraint_set(formal, when, &mut *f)?;
            } else {
                // An overloaded actual callable is compatible with the formal signature if at
                // least one of its overloads is. We collect type mappings from all satisfiable
                // overloads, and only report an error if none of them are satisfiable.
                let mut any_satisfiable = false;
                for actual_signature in &actual_callable.signatures(self.db).overloads {
                    let when = actual_signature.when_constraint_set_assignable_to_signatures(
                        self.db,
                        formal_signature,
                        self.constraints,
                    );
                    if self
                        .add_type_mappings_from_constraint_set(formal, when, &mut *f)
                        .is_ok()
                    {
                        any_satisfiable = true;
                    }
                }
                if !any_satisfiable {
                    return Err(());
                }
            }
        }
        Ok(())
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
        self.infer_map_impl(
            formal,
            actual,
            TypeVarVariance::Covariant,
            &mut f,
            &mut FxHashSet::default(),
        )
    }

    fn infer_map_impl(
        &mut self,
        formal: Type<'db>,
        actual: Type<'db>,
        polarity: TypeVarVariance,
        mut f: &mut dyn FnMut(TypeVarAssignment<'db>) -> Option<Type<'db>>,
        seen: &mut FxHashSet<(Type<'db>, Type<'db>)>,
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

        // Avoid infinite recursion
        if !seen.insert((formal, actual)) {
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
            // Expand PEP 695 type aliases in the formal type.
            // This is necessary for solving generics like `def head[T](my_list: MyList[T]) -> T`.
            (Type::TypeAlias(alias), _) => {
                return self.infer_map_impl(alias.value_type(self.db), actual, polarity, f, seen);
            }

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
                // If the formal is a union and the actual is a bare inferable TypeVar in an
                // invariant position, record the whole union as the mapping. Invariant matching is
                // equality-like; probing individual union elements below can leave spurious
                // partial mappings from non-matching elements. For example, while comparing
                // `ClassSelector[T]` with `ClassSelector[CT | None]`, descending into `None`
                // would map `T` to `None` before `CT` is solved from another argument.
                if let Type::TypeVar(actual_typevar) = actual
                    && actual_typevar.is_inferable(self.db, self.inferable)
                    && matches!(polarity, TypeVarVariance::Invariant)
                {
                    self.add_type_mapping(actual_typevar, formal, polarity, f);
                    return Ok(());
                }

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
                            .when_subtype_of(self.db, **ty, self.constraints, self.inferable)
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
                    let result =
                        self.infer_map_impl(*formal_element, actual, polarity, &mut f, seen);
                    if let Err(err) = result {
                        first_error.get_or_insert(err);
                    } else {
                        // The recursive call to `infer_map_impl` may succeed even if the actual type is
                        // not assignable to the formal element.
                        if !actual
                            .when_assignable_to(
                                self.db,
                                *formal_element,
                                self.constraints,
                                self.inferable,
                            )
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

            (Type::TypeVar(bound_typevar), ty) | (ty, Type::TypeVar(bound_typevar))
                if bound_typevar.is_inferable(self.db, self.inferable) =>
            {
                match bound_typevar.typevar(self.db).bound_or_constraints(self.db) {
                    Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                        if polarity.is_contravariant() {
                            // In a contravariant position, the formal type variable is a subtype of
                            // the actual type (`T <: ty`). Since we also have the upper bound
                            // constraint `T <: bound`, we just need to ensure that the intersection
                            // of `ty` and `bound` is non-empty. Since `Never` is always a valid
                            // intersection if the types are disjoint, we don't need to perform any
                            // check here.
                            self.add_type_mapping(
                                bound_typevar,
                                IntersectionType::from_two_elements(self.db, bound, ty),
                                polarity,
                                f,
                            );
                            return Ok(());
                        }
                        if !ty
                            .when_assignable_to(self.db, bound, self.constraints, self.inferable)
                            .is_always_satisfied(self.db)
                        {
                            return Err(SpecializationError::MismatchedBound {
                                bound_typevar,
                                argument: ty,
                            });
                        }
                        self.add_type_mapping(bound_typevar, ty, polarity, f);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(typevar_constraints)) => {
                        // Prefer an exact match first.
                        for constraint in typevar_constraints.elements(self.db) {
                            if ty == *constraint {
                                self.add_type_mapping(bound_typevar, ty, polarity, f);
                                return Ok(());
                            }
                        }

                        // If `ty` is itself a constrained TypeVar, check whether each
                        // of its constraints is equivalent to at least one constraint of
                        // the formal TypeVar. This handles the case where two TypeVars
                        // with identical constraint sets are used across function
                        // boundaries.
                        //
                        // We require equivalence rather than assignability to maintain
                        // soundness: constrained TypeVars allow narrowing via
                        // `isinstance` checks inside the function body, so a constraint
                        // that is a strict subtype (e.g. `bool` vs `int`) would allow
                        // the callee to return a widened type that violates the caller's
                        // constraint.
                        if let Type::TypeVar(actual_typevar) = ty
                            && let Some(actual_constraints) =
                                actual_typevar.typevar(self.db).constraints(self.db)
                        {
                            let all_satisfied =
                                actual_constraints.iter().all(|actual_constraint| {
                                    typevar_constraints.elements(self.db).iter().any(
                                        |formal_constraint| {
                                            actual_constraint
                                                .is_equivalent_to(self.db, *formal_constraint)
                                        },
                                    )
                                });
                            if all_satisfied {
                                self.add_type_mapping(bound_typevar, ty, polarity, f);
                                return Ok(());
                            }
                        }

                        for constraint in typevar_constraints.elements(self.db) {
                            let is_satisfied = if polarity.is_contravariant() {
                                constraint
                                    .when_assignable_to(
                                        self.db,
                                        ty,
                                        self.constraints,
                                        self.inferable,
                                    )
                                    .is_always_satisfied(self.db)
                            } else {
                                ty.when_assignable_to(
                                    self.db,
                                    *constraint,
                                    self.constraints,
                                    self.inferable,
                                )
                                .is_always_satisfied(self.db)
                            };

                            if is_satisfied {
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

            (Type::Intersection(formal_intersection), _) => {
                // The actual type must be assignable to every (positive) element of the
                // formal intersection, so we must infer type mappings for each of them. (The
                // actual type must also be disjoint from every negative element of the
                // intersection, but that doesn't help us infer any type mappings.)
                for positive in formal_intersection.iter_positive(self.db) {
                    self.infer_map_impl(positive, actual, polarity, f, seen)?;
                }
            }
            (_, Type::Intersection(actual_intersection)) => {
                // Try to infer type mappings by checking against each intersection element. This
                // is the dual of the `union_formal` arm above, and it handles cases like:
                //
                // ```py
                // def f[T](t: P[T]) -> T: ...
                //
                // def _(x: P[str] & Q[str]):
                //     reveal_type(f(x))  # revealed: str
                // ```
                //
                // It's important that this arm comes after the `TypeVar` arm above, so that a bare
                // typevar bound to an intersection gets the whole thing.
                //
                // It's sufficient for one intersection element to satisfy the constraints here.
                // They don't all have to.
                let mut first_error = None;
                let mut found_matching_element = false;
                for positive in actual_intersection.iter_positive(self.db) {
                    let result = self.infer_map_impl(formal, positive, polarity, f, seen);
                    if let Err(err) = result {
                        // TODO: `infer_map_impl` can have side effects even in the error case, so
                        // to be fully correct here we'd need to snapshot `self.types` before each
                        // call and roll it back if we get an error. The `Union` arm has the same
                        // issue above.
                        first_error.get_or_insert(err);
                    } else {
                        // The recursive call to `infer_map_impl` may succeed even if the actual
                        // type is not assignable to the formal element.
                        if !positive
                            .when_assignable_to(self.db, formal, self.constraints, self.inferable)
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

            (Type::SubclassOf(subclass_of), ty) | (ty, Type::SubclassOf(subclass_of))
                if subclass_of.is_type_var() =>
            {
                let formal_instance = Type::TypeVar(subclass_of.into_type_var().unwrap());
                if let Some(actual_instance) = ty.to_instance(self.db) {
                    return self.infer_map_impl(
                        formal_instance,
                        actual_instance,
                        polarity,
                        f,
                        seen,
                    );
                }
            }

            (
                formal @ (Type::NominalInstance(_) | Type::ProtocolInstance(_)),
                Type::LiteralValue(literal),
            ) => {
                // Retry specialization with the literal's fallback instance so literals can
                // contribute to generic inference for nominal and protocol formals.
                let actual_instance = literal.fallback_instance(self.db);
                return self.infer_map_impl(formal, actual_instance, polarity, f, seen);
            }

            (formal, Type::ProtocolInstance(actual_protocol)) => {
                // TODO: This will only handle protocol classes that explicit inherit
                // from other generic protocol classes by listing it as a base class.
                // To handle classes that implicitly implement a generic protocol, we
                // will need to check the types of the protocol members to be able to
                // infer the specialization of the protocol that the class implements.
                if let Some(actual_nominal) = actual_protocol.to_nominal_instance() {
                    return self.infer_map_impl(
                        formal,
                        Type::NominalInstance(actual_nominal),
                        polarity,
                        f,
                        seen,
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
                        self.infer_map_impl(
                            *formal_element,
                            *actual_element,
                            variance,
                            &mut f,
                            seen,
                        )?;
                    }
                    return Ok(());
                }

                // Extract formal_alias if this is a generic class
                let formal_alias = match formal {
                    Type::NominalInstance(formal_nominal) => {
                        formal_nominal.class(self.db).into_generic_alias()
                    }

                    Type::ProtocolInstance(_) => {
                        // TODO: For protocols, we use the new constraint set implementation, which
                        // will handle implicitly implemented protocols and generic protocols. We
                        // eventually want this logic to be used for _all_ nominal instances
                        // (replacing the logic below).
                        let when = actual.when_constraint_set_assignable_to_owned(self.db, formal);
                        // For protocol inference via constraint sets, we currently treat
                        // unsatisfiable results as "no inference" instead of an immediate
                        // specialization error. This matches the previous behavior (where
                        // unsatisfied comparisons simply produced no type mappings), and avoids
                        // false positives for callable-wrapper patterns while this path is still
                        // a hybrid of old and new solver logic.
                        let _ =
                            self.add_type_mappings_from_owned_constraint_set(formal, when, &mut f);
                        return Ok(());
                    }

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
                            self.infer_map_impl(*formal_ty, *base_ty, variance, &mut f, seen)?;
                        }
                        return Ok(());
                    }
                }
            }

            // TODO: in principle this could be a generalized Union-actual arm that maps over the
            // union, but the old solver isn't well-equipped to handle that (due to side effects
            // from even failed matches), so for now we handle this particular case.
            (formal @ Type::ProtocolInstance(_), actual @ Type::Union(_)) => {
                let when = actual.when_constraint_set_assignable_to_owned(self.db, formal);
                // For protocol inference via constraint sets, keep unsatisfiable results non-fatal
                // for now, matching the protocol constraint-set path in the nominal-instance
                // arm above.
                let _ = self.add_type_mappings_from_owned_constraint_set(formal, when, &mut f);
                return Ok(());
            }

            // When the formal type is a protocol with a `__call__` method, infer the specialization
            // from matching the actual type's callable signature against the protocol's `__call__`
            // method signature.
            (Type::ProtocolInstance(formal_protocol), _) => {
                let Some(call_method) = formal_protocol.interface(self.db).call_method(self.db)
                else {
                    return Ok(());
                };
                let Some(actual_callables) = actual.try_upcast_to_callable(self.db) else {
                    return Ok(());
                };

                // The `__call__` method is bound to `self`, so we need to bind it to get the
                // callable signature that the actual type needs to match.
                let formal_signature = call_method.bind_self(self.db, None).signatures(self.db);

                // For callable-signature inference, keep unsatisfiable constraint-set
                // comparisons non-fatal for now. The hybrid inference/checking pipeline still
                // depends on post-specialization assignability checks for some callable wrapper
                // patterns (e.g. `functools.wraps`, callback adapters).
                let _ = self.infer_from_callable_signature(
                    formal,
                    formal_signature,
                    &actual_callables,
                    f,
                );
            }

            (Type::Callable(formal_callable), _) => {
                let Some(actual_callables) = actual.try_upcast_to_callable(self.db) else {
                    return Ok(());
                };
                let formal_signature = formal_callable.signatures(self.db);

                // For callable-signature inference, keep unsatisfiable constraint-set
                // comparisons non-fatal for now. The hybrid inference/checking pipeline still
                // depends on post-specialization assignability checks for some callable wrapper
                // patterns (e.g. `functools.wraps`, callback adapters).
                let _ = self.infer_from_callable_signature(
                    formal,
                    formal_signature,
                    &actual_callables,
                    f,
                );
            }

            // Expand type aliases in the actual type.
            //
            // This is placed at the end of the match block to avoid expanding the type alias
            // when it can be matched directly against a type variable in the formal type,
            // e.g., `reveal_type(alias)` should reveal the type alias, not its value type.
            (formal, Type::TypeAlias(alias)) => {
                return self.infer_map_impl(formal, alias.value_type(self.db), polarity, f, seen);
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
