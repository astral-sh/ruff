use crate::{Program, ProgramEnvironment};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::hash_map::Entry;

use itertools::Itertools;
use ruff_python_ast as ast;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::types::callable::walk_callable_type;
use crate::types::class::ClassType;
use crate::types::class_base::ClassBase;
use crate::types::constraints::{
    ConstraintBounds, ConstraintSet, ConstraintSetBuilder, IteratorConstraintsExtension, PathBound,
    PathBounds, Solutions,
};
use crate::types::infer::original_class_type;
use crate::types::relation::{
    DisjointnessChecker, HasRelationToVisitor, IsDisjointVisitor, TypeRelation,
    TypeRelationChecker, TypeVarEvaluation,
};
use crate::types::signatures::{Parameters, ReturnCallableTypeVarScope, SignatureRelationVisitor};
use crate::types::tuple::{
    TupleSpec, TupleSpecBuilder, TupleType, VariableSegment, walk_tuple_type,
};
use crate::types::type_alias::{walk_manual_pep_695_type_alias, walk_pep_695_type_alias};
use crate::types::typevar::{
    BoundTypeVarIdentity, TypeVarIdentity, TypeVarInstance, TypeVarSet, walk_type_var_bounds,
};
use crate::types::visitor::{
    TypeCollector, TypeVisitor, any_over_type, walk_type_with_recursion_guard,
};
use crate::types::{
    ApplyTypeMappingVisitor, BindingContext, BoundTypeVarInstance, CallableType, CallableTypes,
    ClassLiteral, FindLegacyTypeVarsVisitor, IntersectionType, KnownClass, KnownInstanceType,
    MaterializationKind, SubclassOfInner, Type, TypeAliasType, TypeContext, TypeMapping,
    TypeVarBoundOrConstraints, TypeVarKind, TypeVarVariance, UnionAccumulator, UnionType,
    binding_type, infer_definition_types, inferred_declaration,
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

/// Returns the binding contexts introduced by the given scope or any enclosing scope.
pub(crate) fn enclosing_binding_contexts<'a, 'db>(
    index: &'a SemanticIndex<'db>,
    scope: FileScopeId,
) -> impl Iterator<Item = BindingContext<'db>> + 'a {
    index
        .ancestor_scopes(scope)
        .filter_map(|(_, ancestor_scope)| match ancestor_scope.node() {
            NodeWithScopeKind::Class(node) => Some(index.expect_single_definition(node).into()),
            NodeWithScopeKind::Function(node) => Some(index.expect_single_definition(node).into()),
            NodeWithScopeKind::TypeAlias(node) => Some(index.expect_single_definition(node).into()),
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
    index: &SemanticIndex<'db>,
    containing_scope: FileScopeId,
    typevar_binding_context: Option<Definition<'db>>,
    typevar: TypeVarInstance<'db>,
) -> Option<BoundTypeVarInstance<'db>> {
    find_typevar_binding(
        db,
        index,
        containing_scope,
        typevar,
        ReturnCallableTypeVarScope::Public,
    )
    .or_else(|| {
        typevar_binding_context.map(|typevar_binding_context| {
            typevar.with_binding_context(db, typevar_binding_context)
        })
    })
}

/// Resolves a reference to a type variable that must already be bound.
///
/// Unlike [`bind_typevar`], this function never introduces a binding in the current context. It
/// also uses the lexical form of enclosing function signatures, in which type variables moved to
/// a returned callable's public generic context are still visible within the function body. This
/// lets `P.args` and `P.kwargs` validation establish that an enclosing `ParamSpec` is in scope
/// without changing the binding selected for the current function's signature.
pub(crate) fn resolve_typevar_reference<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    containing_scope: FileScopeId,
    typevar: TypeVarInstance<'db>,
) -> Option<BoundTypeVarInstance<'db>> {
    find_typevar_binding(
        db,
        index,
        containing_scope,
        typevar,
        ReturnCallableTypeVarScope::Lexical,
    )
}

/// Finds the nearest visible binding under the requested treatment of return-only callable type
/// variables.
///
/// Captured `ParamSpec` bindings are recovered from component annotations because those bindings
/// are deliberately excluded from a nested function's own generic context. A binding owned by a
/// class is hidden after the search crosses a nested class boundary.
fn find_typevar_binding<'db>(
    db: &'db dyn Db,
    index: &SemanticIndex<'db>,
    containing_scope: FileScopeId,
    typevar: TypeVarInstance<'db>,
    return_callable_typevar_scope: ReturnCallableTypeVarScope,
) -> Option<BoundTypeVarInstance<'db>> {
    /// Returns whether a binding remains visible after crossing an inner class boundary.
    ///
    /// Class-owned bindings are hidden by the inner class; function-owned and synthetic bindings
    /// remain visible.
    fn is_visible_across_class_boundary<'db>(
        db: &'db dyn Db,
        bound: BoundTypeVarInstance<'db>,
        crossed_class_scope: bool,
    ) -> bool {
        !crossed_class_scope
            || !bound
                .binding_context(db)
                .definition()
                .is_some_and(|definition| matches!(definition.kind(db), DefinitionKind::Class(_)))
    }

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
    for (ancestor_scope_id, ancestor_scope) in index.ancestor_scopes(containing_scope) {
        let is_class_scope = ancestor_scope.kind().is_class();
        if let NodeWithScopeKind::FunctionTypeParameters(function) = ancestor_scope.node() {
            // PEP 695 type parameters are defined in the function's type-parameter scope.
            // Check that directly instead of reconstructing the function's signature.
            if typevar
                .definition(db)
                .is_some_and(|definition| definition.file_scope(db) == ancestor_scope_id)
            {
                let definition = index.expect_single_definition(function);
                return Some(typevar.with_binding_context(db, definition));
            }
            continue;
        }
        if typevar.is_paramspec(db)
            && let NodeWithScopeKind::Function(function) = ancestor_scope.node()
        {
            let definition = index.expect_single_definition(function);
            if let Some(function_ty) =
                infer_definition_types(db, definition).function_type(definition)
            {
                let signature = function_ty
                    .last_definition_raw_signature(db, ReturnCallableTypeVarScope::Lexical);
                if let Some(bound) = signature.paramspec_component_binding(db, typevar)
                    && bound.binding_context(db).definition() != Some(definition)
                    && is_visible_across_class_boundary(db, bound, crossed_class_scope)
                {
                    return Some(bound);
                }
            }
        }
        let generic_context = match return_callable_typevar_scope {
            ReturnCallableTypeVarScope::Lexical => {
                GenericContext::lexical_of_node(db, ancestor_scope.node(), index)
            }
            ReturnCallableTypeVarScope::Public => {
                GenericContext::of_node(db, ancestor_scope.node(), index)
            }
        };
        // If we've already crossed a class boundary, skip class-scoped generic contexts.
        // This prevents inner classes from accessing type parameters of outer classes.
        // An enclosing function's context can also retain a type variable originally bound by its
        // enclosing class, so check the binding context as well as the ancestor node.
        if (!is_class_scope || !crossed_class_scope)
            && let Some(generic_context) = generic_context
            && let Some(bound) = generic_context.binds_typevar(db, typevar)
            && is_visible_across_class_boundary(db, bound, crossed_class_scope)
        {
            return Some(bound);
        }
        if is_class_scope {
            crossed_class_scope = true;
        }
    }
    None
}

/// Create a `typing.Self` type variable for a given class.
pub(crate) fn typing_self<'db>(
    db: &'db dyn Db,
    scope_id: ScopeId<'db>,
    typevar_binding_context: Option<Definition<'db>>,
    class: ClassLiteral<'db>,
) -> Option<BoundTypeVarInstance<'db>> {
    let env = ProgramEnvironment::from_scope(scope_id);
    let index = semantic_index(db, scope_id.program_file(db));

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
        &env,
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

/// A list of formal type variables for a generic function, class, type alias, or fresh callable
/// occurrence.
///
/// Variables are keyed by bound occurrence identity, so freshened copies of the same source-level
/// generic context can coexist without collapsing into each other.
#[salsa::interned(debug, constructor=new_internal, heap_size=ruff_memory_usage::heap_size)]
pub struct GenericContext<'db> {
    #[returns(copy)]
    pub(crate) program: Program<'db>,

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
        index: &SemanticIndex<'db>,
        binding_context: Definition<'db>,
        type_params_node: &ast::TypeParams,
    ) -> Self {
        let variables = type_params_node.iter().filter_map(|type_param| {
            Self::variable_from_type_param(db, index, binding_context, type_param)
        });

        Self::from_typevar_instances_in_program(db, binding_context.program(db), variables)
    }

    pub(crate) fn of_node(
        db: &'db dyn Db,
        node: &NodeWithScopeKind,
        index: &SemanticIndex<'db>,
    ) -> Option<Self> {
        match node {
            NodeWithScopeKind::Class(class) => {
                let definition = index.expect_single_definition(class);
                original_class_type(db, definition)?.generic_context(db)
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

    /// Returns the generic context visible while checking the scope introduced by `node`.
    ///
    /// For functions, this retains type variables that are moved to a returned callable in the
    /// externally visible signature. Other scope kinds have identical lexical and public contexts.
    fn lexical_of_node(
        db: &'db dyn Db,
        node: &NodeWithScopeKind,
        index: &SemanticIndex<'db>,
    ) -> Option<Self> {
        if let NodeWithScopeKind::Function(function) = node {
            let definition = index.expect_single_definition(function);
            infer_definition_types(db, definition)
                .function_type(definition)?
                .last_definition_raw_signature(db, ReturnCallableTypeVarScope::Lexical)
                .generic_context
        } else {
            Self::of_node(db, node, index)
        }
    }

    /// Creates a generic context from a list of `BoundTypeVarInstance`s.
    pub(crate) fn from_typevar_instances(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        type_params: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
    ) -> Self {
        Self::from_typevar_instances_in_program(db, env.program(db), type_params)
    }

    fn from_typevar_instances_in_program(
        db: &'db dyn Db,
        program: Program<'db>,
        type_params: impl IntoIterator<Item = BoundTypeVarInstance<'db>>,
    ) -> Self {
        Self::new_internal(
            db,
            program,
            type_params
                .into_iter()
                .map(|variable| (variable.identity(db), variable))
                .collect::<FxOrderMap<_, _>>(),
        )
    }

    /// Merge this generic context with another, returning a new generic context that
    /// contains type variables from both contexts.
    pub(crate) fn merge(self, db: &'db dyn Db, other: Self) -> Self {
        let program = self.program(db);
        debug_assert_eq!(program, other.program(db));
        Self::from_typevar_instances_in_program(
            db,
            program,
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
        #[salsa::tracked(returns(copy), heap_size=ruff_memory_usage::heap_size)]
        fn remove_self_inner<'db>(
            db: &'db dyn Db,
            generic_context: GenericContext<'db>,
            binding_context: Option<BindingContext<'db>>,
        ) -> GenericContext<'db> {
            GenericContext::from_typevar_instances_in_program(
                db,
                generic_context.program(db),
                generic_context.variables(db).filter(|bound_typevar| {
                    !(bound_typevar.typevar(db).is_self(db)
                        && binding_context.is_none_or(|binding_context| {
                            bound_typevar.binding_context(db) == binding_context
                        }))
                }),
            )
        }

        remove_self_inner(db, self, binding_context)
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
    pub(crate) fn inferable_typevars(self, db: &'db dyn Db) -> TypeVarSet<'db> {
        struct CollectTypeVars<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            typevars: RefCell<FxOrderMap<BoundTypeVarIdentity<'db>, BoundTypeVarInstance<'db>>>,
            recursion_guard: TypeCollector<'db>,
        }

        impl<'db> TypeVisitor<'db> for CollectTypeVars<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }

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
                    .entry(bound_typevar.identity(db))
                    .or_insert(bound_typevar);
                let typevar = bound_typevar.typevar(db);
                if let Some(bound_or_constraints) =
                    typevar.bound_or_constraints(db, self.program_environment())
                {
                    walk_type_var_bounds(db, bound_or_constraints, self);
                }
            }

            fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
                walk_type_with_recursion_guard(db, ty, self, &self.recursion_guard);
            }
        }

        #[salsa::tracked(
            returns(copy),
            cycle_initial=|_, _, _| TypeVarSet::None,
            heap_size=ruff_memory_usage::heap_size,
        )]
        fn inferable_typevars_inner<'db>(
            db: &'db dyn Db,
            generic_context: GenericContext<'db>,
        ) -> TypeVarSet<'db> {
            let env = ProgramEnvironment::from_program(generic_context.program(db));
            let visitor = CollectTypeVars {
                env: &env,
                typevars: RefCell::default(),
                recursion_guard: TypeCollector::default(),
            };
            for bound_typevar in generic_context.variables(db) {
                visitor.visit_bound_type_var_type(db, bound_typevar);
            }
            TypeVarSet::from_typevars(db, visitor.typevars.into_inner().into_values())
        }

        inferable_typevars_inner(db, self)
    }

    pub(crate) fn variables(
        self,
        db: &'db dyn Db,
    ) -> impl ExactSizeIterator<Item = BoundTypeVarInstance<'db>> + Clone {
        self.variables_inner(db).values().copied()
    }

    pub(crate) fn contains(
        self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarIdentity<'db>,
    ) -> bool {
        let bound_typevar = if bound_typevar.is_paramspec(db) {
            bound_typevar.without_paramspec_attr(db)
        } else {
            bound_typevar
        };
        self.variables_inner(db).contains_key(&bound_typevar)
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
        index: &SemanticIndex<'db>,
        binding_context: Definition<'db>,
        type_param_node: &ast::TypeParam,
    ) -> Option<BoundTypeVarInstance<'db>> {
        match type_param_node {
            ast::TypeParam::TypeVar(node) => {
                let definition = index.expect_single_definition(node);
                let declared = inferred_declaration(db, definition).declared()?;
                let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
                    declared.inner_type()
                else {
                    return None;
                };
                Some(typevar.with_binding_context(db, binding_context))
            }
            ast::TypeParam::ParamSpec(node) => {
                let definition = index.expect_single_definition(node);
                let declared = inferred_declaration(db, definition).declared()?;
                let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
                    declared.inner_type()
                else {
                    return None;
                };
                Some(typevar.with_binding_context(db, binding_context))
            }
            ast::TypeParam::TypeVarTuple(node) => {
                let definition = index.expect_single_definition(node);
                let declared = inferred_declaration(db, definition).declared()?;
                let Type::KnownInstance(KnownInstanceType::TypeVar(typevar)) =
                    declared.inner_type()
                else {
                    return None;
                };
                Some(typevar.with_binding_context(db, binding_context))
            }
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
        let env = ProgramEnvironment::from_definition(definition);
        // Find all of the legacy typevars mentioned in the function signature.
        let mut variables = FxOrderSet::default();
        for param in parameters {
            param
                .annotated_type()
                .find_legacy_typevars(db, &env, Some(definition), &mut variables);
            if let Some(ty) = param.default_type() {
                ty.find_legacy_typevars(db, &env, Some(definition), &mut variables);
            }
        }
        return_type.find_legacy_typevars(db, &env, Some(definition), &mut variables);

        if variables.is_empty() {
            return None;
        }
        Some(Self::from_typevar_instances(db, &env, variables))
    }

    pub(crate) fn merge_pep695_and_legacy(
        db: &'db dyn Db,
        pep695_generic_context: Option<Self>,
        legacy_generic_context: Option<Self>,
    ) -> Option<Self> {
        match (legacy_generic_context, pep695_generic_context) {
            (Some(legacy_ctx), Some(env)) => {
                if legacy_ctx
                    .variables(db)
                    .exactly_one()
                    .is_ok_and(|bound_typevar| bound_typevar.typevar(db).is_self(db))
                {
                    Some(legacy_ctx.merge(db, env))
                } else {
                    // Invalid mixes retained in the inferred signature are reported during
                    // post-inference validation.
                    Some(env)
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
        let env = ProgramEnvironment::from_definition(definition);
        let mut variables = FxOrderSet::default();
        for base in bases {
            base.find_legacy_typevars(db, &env, Some(definition), &mut variables);
        }
        if variables.is_empty() {
            return None;
        }
        Some(Self::from_typevar_instances(db, &env, variables))
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
                let env = ProgramEnvironment::from_definition(function_definition);
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
                            &ApplyTypeMappingVisitor::new(&env),
                        );
                        let generic_context = GenericContext::from_typevar_instances(
                            db,
                            &env,
                            typevar_replacements.values().copied(),
                        );
                        let signatures =
                            signatures.with_inherited_generic_context(db, generic_context);
                        let replacement = CallableType::new(
                            db,
                            signatures,
                            callable.kind(db),
                            callable.provenance(db),
                        );

                        Some((callable, replacement))
                    })
                    .collect();

                (found_only_inside_callable_return, replacements)
            }
        }

        /// A visitor that walks through the parameter and return type annotations, recording
        /// whether each typevar appears inside and/or outside of a return type `Callable`.
        struct FindTypeVarLocations<'a, 'db> {
            env: &'a ProgramEnvironment<'db>,
            locations: RefCell<TypeVarLocations<'db>>,
            recursion_guard: TypeCollector<'db>,
            in_return_type: bool,
            in_callable_type: Cell<Option<CallableType<'db>>>,
        }

        impl<'db> TypeVisitor<'db> for FindTypeVarLocations<'_, 'db> {
            fn program_environment(&self) -> &ProgramEnvironment<'db> {
                self.env
            }

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
        let env = ProgramEnvironment::from_definition(function_definition);

        // Find whether each typevar appears inside and/or outside a return type Callable.
        let mut find_typevar_locations = FindTypeVarLocations {
            env: &env,
            locations: RefCell::default(),
            recursion_guard: TypeCollector::default(),
            in_return_type: false,
            in_callable_type: Cell::default(),
        };
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
        let return_type =
            return_type.apply_type_mapping(db, &env, &type_mapping, TypeContext::default());

        // And lastly remove those typevars from the function's generic context.
        let mut kept_typevars = generic_context
            .variables(db)
            .filter(|bound_typevar| !found_only_inside_callable_return.contains(bound_typevar))
            .peekable();
        let generic_context = if kept_typevars.peek().is_none() {
            None
        } else {
            Some(GenericContext::from_typevar_instances(
                db,
                &env,
                kept_typevars,
            ))
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
            let env = ProgramEnvironment::from_program(self.program(db));
            Specialization::new(
                db,
                self,
                partial.types(db),
                None,
                Some(TupleType::homogeneous(db, &env, Type::unknown())),
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

    /// Returns a specialization of this generic context where each typevar is mapped to the same type.
    pub(crate) fn repeat_specialization(
        self,
        db: &'db dyn Db,
        ty: Type<'db>,
    ) -> Specialization<'db> {
        let types: Vec<Type> = self.variables(db).map(|_| ty).collect();
        self.specialize(db, types)
    }

    /// Specializes every type parameter to its unknown form.
    ///
    /// The built-in `tuple` also needs an explicit variable-length tuple shape so that
    /// materialization can preserve its element type.
    pub(crate) fn unknown_specialization(
        self,
        db: &'db dyn Db,
        known_class: Option<KnownClass>,
    ) -> Specialization<'db> {
        let env = ProgramEnvironment::from_program(self.program(db));
        Specialization::new(
            db,
            self,
            self.variables(db)
                .map(|typevar| match typevar.kind(db) {
                    TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => {
                        Type::homogeneous_tuple(db, &env, Type::unknown())
                    }
                    TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => {
                        Type::paramspec_value_callable(db, Parameters::unknown())
                    }
                    _ => Type::unknown(),
                })
                .collect::<Box<[_]>>(),
            None,
            (known_class == Some(KnownClass::Tuple))
                .then(|| TupleType::homogeneous(db, &env, Type::unknown())),
        )
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
        let env = ProgramEnvironment::from_program(self.program(db));
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
                    &env,
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
        Specialization::new(db, self, [element_type].as_slice(), None, Some(tuple))
    }

    fn fill_in_defaults<I>(self, db: &'db dyn Db, types: I) -> Box<[Type<'db>]>
    where
        I: IntoIterator<Item = Option<Type<'db>>>,
        I::IntoIter: ExactSizeIterator,
    {
        let env = ProgramEnvironment::from_program(self.program(db));
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
            expanded.push(match typevar.kind(db) {
                TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => {
                    Type::homogeneous_tuple(db, &env, Type::unknown())
                }
                TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => {
                    Type::paramspec_value_callable(db, Parameters::unknown())
                }
                _ => Type::unknown(),
            });
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
                &env,
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
    #[returns(copy)]
    pub(crate) generic_context: GenericContext<'db>,
    #[returns(deref)]
    pub(crate) types: Box<[Type<'db>]>,
    /// The materialization kind of the specialization. For example, given an invariant
    /// generic type `A`, `Top[A[Any]]` is a supertype of all materializations of `A[Any]`,
    /// and is represented here with `Some(MaterializationKind::Top)`. Similarly,
    /// `Bottom[A[Any]]` is a subtype of all materializations of `A[Any]`, and is represented
    /// with `Some(MaterializationKind::Bottom)`.
    /// The `materialization_kind` field may be non-`None` only if the specialization contains
    /// dynamic types in invariant positions or positions with constrained type variables.
    #[returns(copy)]
    pub(crate) materialization_kind: Option<MaterializationKind>,

    /// For specializations of `tuple`, we also store more detailed information about the tuple's
    /// elements, above what the class's (single) typevar can represent.
    #[returns(copy)]
    tuple_inner: Option<TupleType<'db>>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for Specialization<'_> {}

/// Visit specialization arguments and the generic declaration.
pub(super) fn walk_specialization<'db, V: TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    specialization: Specialization<'db>,
    visitor: &V,
) {
    walk_generic_context(db, specialization.generic_context(db), visitor);
    walk_specialization_types(db, specialization, visitor);
}

/// Visit specialization arguments without walking the generic declaration.
pub(super) fn walk_specialization_types<'db, V: TypeVisitor<'db> + ?Sized>(
    db: &'db dyn Db,
    specialization: Specialization<'db>,
    visitor: &V,
) {
    for ty in specialization.types(db) {
        visitor.visit_type(db, *ty);
    }
    if let Some(tuple) = specialization.tuple_inner(db) {
        walk_tuple_type(db, tuple, visitor);
    }
}

impl<'db> Specialization<'db> {
    /// Merge cycle iterations that differ only by gradual `Unknown` type arguments.
    ///
    /// Known argument mismatches are not merged because doing so would be unsound for invariant
    /// type variables; the caller must retain the outer semantic union in that case.
    pub(super) fn merge_cycle_recovery(self, db: &'db dyn Db, previous: Self) -> Option<Self> {
        if self.generic_context(db) != previous.generic_context(db)
            || self.materialization_kind(db) != previous.materialization_kind(db)
            || self.tuple_inner(db) != previous.tuple_inner(db)
        {
            return None;
        }

        let types: Box<[_]> = previous
            .types(db)
            .iter()
            .zip(self.types(db))
            .map(|(previous, current)| match (*previous, *current) {
                (previous, current) if previous == current => Some(current),
                (previous, current)
                    if previous == Type::unknown() || current == Type::unknown() =>
                {
                    Some(Type::unknown())
                }
                _ => None,
            })
            .collect::<Option<Box<[_]>>>()?;

        Some(Self::new(
            db,
            self.generic_context(db),
            types,
            self.materialization_kind(db),
            self.tuple_inner(db),
        ))
    }

    /// Maps the specialization's types, returning [`Cow::Borrowed`] without allocating if every
    /// type is unchanged and [`Cow::Owned`] otherwise.
    fn map_types(
        self,
        db: &'db dyn Db,
        mut map: impl FnMut(usize, BoundTypeVarInstance<'db>, Type<'db>) -> Type<'db>,
    ) -> Cow<'db, [Type<'db>]> {
        let types = self.types(db);
        let mut mapped_types: Option<Vec<Type<'db>>> = None;

        for (index, (typevar, ty)) in self
            .generic_context(db)
            .variables(db)
            .zip(types.iter().copied())
            .enumerate()
        {
            let mapped_ty = map(index, typevar, ty);
            if let Some(mapped_types) = &mut mapped_types {
                mapped_types.push(mapped_ty);
            } else if mapped_ty != ty {
                let mut changed_types = Vec::with_capacity(types.len());
                changed_types.extend_from_slice(&types[..index]);
                changed_types.push(mapped_ty);
                mapped_types = Some(changed_types);
            }
        }

        mapped_types.map_or(Cow::Borrowed(types), Cow::Owned)
    }

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

    /// Returns the specialization to use when the builtin tuple type parameter represents an
    /// individual runtime element.
    ///
    /// The builtin tuple class's type parameter describes the type produced by runtime element
    /// access. An unpacked `TypeVarTuple` describes those elements collectively, so project it to
    /// `object` for tuple members and base classes while retaining the symbolic pack in the tuple's
    /// own specialization.
    pub(crate) fn tuple_runtime_element_specialization(self, db: &'db dyn Db) -> Self {
        let Some(tuple) = self.tuple_inner(db) else {
            return self;
        };
        // Ordinary tuple specializations already use their runtime element type as the tuple
        // class's generic argument. Rebuilding them would add allocation and interning work to
        // every tuple member and MRO lookup, both of which are hot paths in tuple-heavy programs.
        if !matches!(
            tuple.tuple(db),
            TupleSpec::Variable(tuple)
                if matches!(tuple.variable(), VariableSegment::TypeVarTuple(_))
        ) {
            return self;
        }

        let env = ProgramEnvironment::from_program(self.generic_context(db).program(db));
        Self::new(
            db,
            self.generic_context(db),
            [tuple.tuple(db).homogeneous_element_type(db, &env)].as_slice(),
            self.materialization_kind(db),
            None,
        )
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
        let env = &ProgramEnvironment::from_program(other.generic_context(db).program(db));
        let new_specialization = self.apply_type_mapping(
            db,
            env,
            &TypeMapping::ApplySpecialization(ApplySpecialization::Specialization(other)),
        );
        match other.materialization_kind(db) {
            None => new_specialization,
            Some(materialization_kind) => new_specialization.materialize_impl(
                db,
                materialization_kind,
                &ApplyTypeMappingVisitor::new(env),
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

    fn apply_type_mapping<'a>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        type_mapping: &TypeMapping<'a, 'db>,
    ) -> Self {
        self.apply_type_mapping_impl(db, type_mapping, &[], &ApplyTypeMappingVisitor::new(env))
    }

    pub(crate) fn apply_type_mapping_impl<'a>(
        self,
        db: &'db dyn Db,
        type_mapping: &TypeMapping<'a, 'db>,
        tcx: &[Type<'db>],
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        if let TypeMapping::Materialize(materialization_kind) = type_mapping {
            return self.materialize_impl(db, *materialization_kind, visitor);
        }

        let mut new_materialization_kind = self.materialization_kind(db);
        let types = self.map_types(db, |i, typevar, ty| {
            let tcx = TypeContext::new(tcx.get(i).copied());
            match (typevar.variance(db), type_mapping) {
                (
                    TypeVarVariance::Invariant,
                    TypeMapping::ApplySpecializationWithMaterialization {
                        specialization,
                        materialization_kind,
                    },
                ) => {
                    let env = visitor.env;
                    // An invariant type argument cannot be materialized in isolation. Keep the
                    // specialized argument and record the materialization on this specialization.
                    // Comparing both mappings distinguishes substituted gradual types from
                    // unrelated gradual types already present in the argument. Use separate
                    // visitors because their transformation caches are keyed only by type.
                    let specialized = ty.apply_type_mapping_impl(
                        db,
                        &TypeMapping::ApplySpecialization(*specialization),
                        tcx,
                        &ApplyTypeMappingVisitor::new(env),
                    );

                    if new_materialization_kind.is_none() {
                        let materialized = ty.apply_type_mapping_impl(
                            db,
                            type_mapping,
                            tcx,
                            &ApplyTypeMappingVisitor::new(env),
                        );
                        if specialized != materialized {
                            new_materialization_kind = Some(*materialization_kind);
                        }
                    }

                    specialized
                }
                (variance, _) if variance.is_covariant() => {
                    ty.apply_type_mapping_impl(db, type_mapping, tcx, visitor)
                }
                _ => ty.apply_type_mapping_impl(db, &type_mapping.flip(), tcx, visitor),
            }
        });

        let original_tuple_inner = self.tuple_inner(db);
        let tuple_inner = original_tuple_inner.map(|tuple| {
            tuple.apply_type_mapping_impl(db, type_mapping, TypeContext::default(), visitor)
        });

        // Keep this check in sync with every field that can be transformed above.
        let specialization_unchanged = matches!(&types, Cow::Borrowed(_))
            && tuple_inner == original_tuple_inner
            && new_materialization_kind == self.materialization_kind(db);
        if specialization_unchanged {
            self
        } else {
            Specialization::new(
                db,
                self.generic_context(db),
                types,
                new_materialization_kind,
                tuple_inner,
            )
        }
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
        let env = ProgramEnvironment::from_program(generic_context.program(db));
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
                _ => UnionType::from_two_elements(db, &env, *self_type, *other_type),
            })
            .collect();
        // TODO: Combine the tuple specs too
        // TODO(jelle): specialization type?
        Specialization::new(db, self.generic_context(db), types, None, None)
    }

    pub(super) fn recursive_type_normalized_impl(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        div: Type<'db>,
        nested: bool,
    ) -> Option<Self> {
        let types = if nested {
            self.types(db)
                .iter()
                .map(|ty| ty.recursive_type_normalized_impl(db, env, div, true))
                .collect::<Option<Box<[_]>>>()?
        } else {
            self.types(db)
                .iter()
                .map(|ty| {
                    ty.recursive_type_normalized_impl(db, env, div, true)
                        .unwrap_or(div)
                })
                .collect::<Box<[_]>>()
        };
        let tuple_inner = match self.tuple_inner(db) {
            Some(tuple) => Some(tuple.recursive_type_normalized_impl(db, env, div, nested)?),
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
        visitor: &ApplyTypeMappingVisitor<'_, 'db>,
    ) -> Self {
        // The top and bottom materializations are fully static types already, so materializing them
        // further does nothing.
        if self.materialization_kind(db).is_some() {
            return self;
        }
        let mut has_unsimplified_dynamic_typevar = false;
        let types = self.map_types(db, |_, bound_typevar, vartype| {
            let variance = specialization_variance(db, bound_typevar);
            let top_materialization = vartype.materialize(db, MaterializationKind::Top, visitor);
            let has_dynamic_type =
                !visitor.is_equivalent_to_materialization(db, vartype, top_materialization);

            match variance {
                TypeVarVariance::Bivariant => {
                    // With bivariance, all specializations are subtypes of each other,
                    // so any materialization is acceptable.
                    top_materialization
                }
                TypeVarVariance::Covariant | TypeVarVariance::Contravariant
                    if has_dynamic_type && bound_typevar.typevar(db).is_constrained(db) =>
                {
                    has_unsimplified_dynamic_typevar = true;
                    vartype
                }
                TypeVarVariance::Covariant | TypeVarVariance::Contravariant => {
                    let effective_materialization_kind = if variance.is_covariant() {
                        materialization_kind
                    } else {
                        materialization_kind.flip()
                    };
                    let materialized =
                        vartype.materialize(db, effective_materialization_kind, visitor);

                    if has_dynamic_type
                        && effective_materialization_kind == MaterializationKind::Top
                        && let Some(upper_bound) = bound_typevar.top_materialized_upper_bound(db)
                    {
                        IntersectionType::from_two_elements(
                            db,
                            visitor.env,
                            materialized,
                            upper_bound,
                        )
                    } else {
                        materialized
                    }
                }
                TypeVarVariance::Invariant => {
                    has_unsimplified_dynamic_typevar |= has_dynamic_type;
                    vartype
                }
            }
        });
        let original_tuple_inner = self.tuple_inner(db);
        let tuple_inner = original_tuple_inner.map(|tuple| {
            // Tuples are immutable, so tuple element types are always in covariant position.
            tuple.apply_type_mapping_impl(
                db,
                &TypeMapping::Materialize(materialization_kind),
                TypeContext::default(),
                visitor,
            )
        });
        let new_materialization_kind =
            has_unsimplified_dynamic_typevar.then_some(materialization_kind);
        // Keep this check in sync with every field that can be transformed above.
        let specialization_unchanged = matches!(&types, Cow::Borrowed(_))
            && tuple_inner == original_tuple_inner
            && new_materialization_kind == self.materialization_kind(db);
        if specialization_unchanged {
            self
        } else {
            Specialization::new(
                db,
                self.generic_context(db),
                types,
                new_materialization_kind,
                tuple_inner,
            )
        }
    }

    pub(crate) fn is_disjoint_from<'c>(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        other: Self,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> ConstraintSet<'db, 'c> {
        let relation_visitor = HasRelationToVisitor::default(constraints);
        let disjointness_visitor = IsDisjointVisitor::default(constraints);
        let signature_relation_visitor = SignatureRelationVisitor::default();
        let materialization_visitor = ApplyTypeMappingVisitor::new(env);
        let checker = DisjointnessChecker::new(
            env,
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
        env: &ProgramEnvironment<'db>,
        binding_context: Option<Definition<'db>>,
        typevars: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        visitor: &FindLegacyTypeVarsVisitor<'db>,
    ) {
        if let Some(tuple) = self.tuple_inner(db) {
            tuple.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
        } else {
            for ty in self.types(db) {
                ty.find_legacy_typevars_impl(db, env, binding_context, typevars, visitor);
            }
        }
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

        let env = self.env;

        // A gradual specialization is a subtype of a fully static specialization when all its
        // valid materializations are subtypes. Materializing the source applies declared bounds
        // and constraints before comparing arguments. This establishes `C[Any] <: Top[C[Any]]`
        // and lets negative `isinstance` narrowing exclude every specialization of `C`.
        //
        // This transformation is sound for directional subtyping and non-pure redundancy.
        // Assignability and pure redundancy must retain the source's gradual semantics.
        if matches!(
            self.relation,
            TypeRelation::Subtyping
                | TypeRelation::SubtypingAssuming
                | TypeRelation::Redundancy { pure: false }
        ) && (
            // Explicitly materialized sources are already static and cannot advance further.
            source.materialization_kind(db).is_none()
        ) && (
            // Performance only: `source_top != source` below already handles unchanged
            // arguments. Without expanding aliases, treat them as potentially gradual.
            source.types(db).iter().any(|ty| {
                any_over_type(db, env, *ty, false, |ty| {
                    ty.is_dynamic() || matches!(ty, Type::TypeAlias(_))
                })
            })
        ) && (
            // Avoid the `self.always()` type-variable shortcut in
            // `check_subtyping_in_invariant_position`: it would incorrectly conclude
            // that `Top[Inv[Any]] <: Inv[T]` for an unresolved `T`.
            // TODO: remove this once that shortcut is removed.
            target
                .types(db)
                .iter()
                .all(|ty| !ty.has_typevar_or_typevar_instance(db, env))
        ) && (
            // Only non-pure redundancy needs a target already equal to its top.
            // Materializing the source otherwise loses the bottom needed to
            // simplify `Covariant[Any] | Covariant[Any | str]`. Comparing both
            // top and bottom is a possible alternative, but it gets more complex
            // due to the need to preserve Divergent markers.
            // TODO: Resolve that issue so we can compare top/bottom subtyping here.
            !matches!(self.relation, TypeRelation::Redundancy { pure: false })
                || target
                    == target.materialize_impl(
                        db,
                        MaterializationKind::Top,
                        self.materialization_visitor,
                    )
        ) {
            let source_top =
                source.materialize_impl(db, MaterializationKind::Top, self.materialization_visitor);
            // Dynamic arguments can still be unchanged by top materialization; retrying
            // the same pair would recurse indefinitely.
            if source_top != source {
                return self.check_specialization_pair(db, source_top, target);
            }
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
                let variance = specialization_variance(db, bound_typevar);

                // Subtyping/assignability of each type in the specialization depends on the variance
                // of the corresponding typevar:
                //   - covariant: verify that source_type <: target_type
                //   - contravariant: verify that target_type <: source_type
                //   - invariant: verify that source_type <: target_type AND target_type <: source_type
                //   - bivariant: skip, can't make subtyping/assignability false
                match variance {
                    TypeVarVariance::Invariant => self.check_relation_in_invariant_position(
                        db,
                        *source_type,
                        source_materialization_kind,
                        *target_type,
                        target_materialization_kind,
                    ),
                    TypeVarVariance::Covariant | TypeVarVariance::Contravariant => {
                        let (
                            source_type,
                            source_materialization,
                            target_type,
                            target_materialization,
                        ) = if variance.is_covariant() {
                            (
                                *source_type,
                                source_materialization_kind,
                                *target_type,
                                target_materialization_kind,
                            )
                        } else {
                            (
                                *target_type,
                                target_materialization_kind.map(MaterializationKind::flip),
                                *source_type,
                                source_materialization_kind.map(MaterializationKind::flip),
                            )
                        };

                        self.check_type_pair(
                            db,
                            self.materialize_constrained_type_argument(
                                db,
                                bound_typevar,
                                source_type,
                                source_materialization,
                            ),
                            self.materialize_constrained_type_argument(
                                db,
                                bound_typevar,
                                target_type,
                                target_materialization,
                            ),
                        )
                    }
                    TypeVarVariance::Bivariant => self.always(),
                }
            },
        )
    }

    /// Materializes a constrained covariant or contravariant argument for a relation check.
    ///
    /// A constrained type variable can only take one of its declared alternatives. For example,
    /// replacing `Any` with `int | str` for `class C[T: (int, str)]` would create the invalid
    /// specialization `C[int | str]`. The caller preserves the enclosing `Top[C[Any]]`; this
    /// helper combines the reachable constraints into `int | str` only for the relation check,
    /// without constructing `C[int | str]`.
    fn materialize_constrained_type_argument(
        &self,
        db: &'db dyn Db,
        bound_typevar: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
        materialization: Option<MaterializationKind>,
    ) -> Type<'db> {
        let Some(materialization) = materialization else {
            return ty;
        };

        // A lazy upper bound may refer back to the enclosing specialization. Check whether this
        // type variable is constrained before evaluating its bounds or constraints.

        let typevar = bound_typevar.typevar(db);
        if !typevar.is_constrained(db) {
            return ty;
        }

        let argument_top =
            ty.materialize(db, MaterializationKind::Top, self.materialization_visitor);
        if self
            .materialization_visitor
            .is_equivalent_to_materialization(db, ty, argument_top)
        {
            return ty;
        }
        let env = self.env;
        let Some(constraints) = typevar.constraints(db, env) else {
            return ty;
        };
        let argument_bottom = ty.materialize(
            db,
            MaterializationKind::Bottom,
            self.materialization_visitor,
        );

        let viable_constraints = constraints.iter().filter_map(|constraint| {
            let constraint_top =
                constraint.materialize(db, MaterializationKind::Top, self.materialization_visitor);

            // A viable constraint must overlap the argument's upper materialization and contain
            // its lower materialization. The upper check matters for `Intersection[int, Any]`,
            // and the lower check matters for `Any | int`.
            if argument_top.is_disjoint_from(db, env, constraint_top)
                || !argument_bottom.is_subtype_of(db, env, constraint_top)
            {
                return None;
            }

            Some(match materialization {
                MaterializationKind::Top => constraint_top,
                MaterializationKind::Bottom => constraint.materialize(
                    db,
                    MaterializationKind::Bottom,
                    self.materialization_visitor,
                ),
            })
        });

        match materialization {
            MaterializationKind::Top => IntersectionType::from_two_elements(
                db,
                env,
                argument_top,
                UnionType::from_elements(db, env, viable_constraints),
            ),
            MaterializationKind::Bottom => UnionType::from_two_elements(
                db,
                env,
                argument_bottom,
                IntersectionType::from_elements(db, env, viable_constraints),
            ),
        }
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
            //
            // For lazy type-variable evaluation, those two directions describe a single
            // constraint. Constructing it directly avoids combinatorial path expansion when many
            // invariant specializations are combined in a union. Subtyping uses the gradual
            // type's materialization range, while assignability uses the type itself for both
            // bounds.
            (None, None, _) => {
                if self.typevar_evaluation == TypeVarEvaluation::Lazy
                    && let (Type::TypeVar(typevar), ty) | (ty, Type::TypeVar(typevar)) =
                        (source_type, target_type)
                    && !ty.is_type_var()
                    && (
                        // Preserve union distribution before constructing constraints. Storing the
                        // entire union as an exact bound makes solving common generic calls involving
                        // large unions significantly more expensive.
                        !ty.is_union()
                    )
                {
                    let ty = ty.materialized_divergent_fallback().unwrap_or(ty);
                    let env = self.env;
                    let (lower, upper) = if self.relation.is_subtyping() {
                        (
                            ty.top_materialization(db, env),
                            ty.bottom_materialization(db, env),
                        )
                    } else {
                        (ty, ty)
                    };
                    ConstraintSet::constrain_typevar(
                        db,
                        env,
                        self.constraints,
                        typevar,
                        lower,
                        upper,
                    )
                } else {
                    self.check_type_pair(db, target_type, source_type).and(
                        db,
                        self.constraints,
                        || self.check_type_pair(db, source_type, target_type),
                    )
                }
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
            (None, Some(target_mat), TypeRelation::Assignability) => self
                .check_subtyping_in_invariant_position(
                    db,
                    source_type,
                    MaterializationKind::Bottom,
                    target_type,
                    target_mat,
                ),
            (Some(source_mat), None, TypeRelation::Assignability) => self
                .check_subtyping_in_invariant_position(
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

        types.when_any(
            db,
            self.constraints,
            |(bound_typevar, left_type, right_type)| match bound_typevar.variance(db) {
                TypeVarVariance::Invariant => {
                    let left_type = left_type.resolve_type_alias(db);
                    let right_type = right_type.resolve_type_alias(db);

                    // `Bottom[L] <: Top[R]` asks whether the materialization ranges for `L`
                    // and `R` have any common materialization, so this is symmetric despite
                    // using a directional subtyping checker.
                    self.as_relation_checker(TypeRelation::Subtyping)
                        .check_subtyping_in_invariant_position(
                            db,
                            left_type,
                            MaterializationKind::Bottom,
                            right_type,
                            MaterializationKind::Top,
                        )
                        .negate(db, self.constraints)
                }

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
    TypeAlias(Specialization<'db>),
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
            ApplySpecialization::Specialization(specialization)
            | ApplySpecialization::TypeAlias(specialization) => {
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
            ApplySpecialization::Specialization(specialization)
            | ApplySpecialization::TypeAlias(specialization) => Some(specialization),
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
        env: &ProgramEnvironment<'db>,
        bound_typevar: BoundTypeVarInstance<'db>,
        replacement: Type<'db>,
    ) -> Type<'db> {
        self.apply_type_mapping(
            db,
            env,
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
    env: &'c ProgramEnvironment<'db>,
    constraints: &'c ConstraintSetBuilder<'db>,
    inferable: TypeVarSet<'db>,
    pending: ConstraintSet<'db, 'c>,
    types: FxHashMap<BoundTypeVarIdentity<'db>, UnionAccumulator<'db>>,
    paramspec_seen: FxHashSet<BoundTypeVarIdentity<'db>>,
}

/// The result of type variable inference before choosing how to handle unsolved type variables.
///
/// A `Some` entry means inference solved the corresponding type variable to that type. A `None`
/// entry means the type variable was not solved and should be projected according to the use site.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub(crate) struct TypeVarInference<'db> {
    #[returns(copy)]
    pub(crate) generic_context: GenericContext<'db>,
    #[returns(deref)]
    types: Box<[Option<Type<'db>>]>,
}

// The Salsa heap is tracked separately.
impl get_size2::GetSize for TypeVarInference<'_> {}

impl<'db> TypeVarInference<'db> {
    /// Project this inference result into a closed specialization.
    pub(crate) fn specialization(self, db: &'db dyn Db) -> Specialization<'db> {
        #[salsa::tracked(
            returns(copy),
            cycle_initial=|db, _, inference: TypeVarInference<'db>| {
                inference.generic_context(db).unknown_specialization(db, None)
            },
            cycle_fn=|db, cycle: &salsa::Cycle, previous: &Specialization<'db>, current: Specialization<'db>, inference: TypeVarInference<'db>| {
                if cycle.iteration() <= crate::TAINTED_CYCLES {
                    current
                } else {
                    current
                        .merge_cycle_recovery(db, *previous)
                        .unwrap_or_else(|| inference.generic_context(db).unknown_specialization(db, None))
                }
            }
        )]
        fn specialization_inner<'db>(
            db: &'db dyn Db,
            inference: TypeVarInference<'db>,
        ) -> Specialization<'db> {
            inference.specialization_with(db, |_, _| None)
        }

        specialization_inner(db, self)
    }

    /// Project this inference result into a specialization with explicit handling for each
    /// type variable.
    ///
    /// The hook receives the type variable and its inferred type, if any. Returning `Some` overrides
    /// the projection for that variable. Returning `None` uses the inferred type if present,
    /// otherwise the type variable's default.
    pub(crate) fn specialization_with(
        self,
        db: &'db dyn Db,
        mut choose: impl FnMut(BoundTypeVarInstance<'db>, Option<Type<'db>>) -> Option<Type<'db>>,
    ) -> Specialization<'db> {
        let types = self
            .generic_context(db)
            .variables(db)
            .zip(self.types(db).iter().copied())
            .map(|(typevar, inferred)| choose(typevar, inferred).or(inferred));

        self.generic_context(db).specialize_recursive(db, types)
    }
}

/// A failure to project a constraint set into the legacy type-mapping representation.
///
/// A type-variable declaration failure can be reported immediately. Other unsatisfiable
/// relations must remain in the pending constraint set so that they invalidate the call-wide
/// solution without producing a misleading bound diagnostic.
enum ConstraintSetInferenceError<'db> {
    InvalidTypeVar(SpecializationError<'db>),
    Unsatisfiable,
}

/// Returns the directional comparisons required by this comparison's polarity.
///
/// A covariant comparison requires `actual <= formal`, a contravariant comparison requires
/// `formal <= actual`, and an invariant comparison requires both. Bivariant comparisons add
/// no constraints.
fn relation_directions<T: Copy>(
    formal: T,
    actual: T,
    polarity: TypeVarVariance,
) -> impl Iterator<Item = (T, T)> {
    [
        (!polarity.is_contravariant()).then_some((actual, formal)),
        (!polarity.is_covariant()).then_some((formal, actual)),
    ]
    .into_iter()
    .flatten()
}

impl<'db, 'c> SpecializationBuilder<'db, 'c> {
    pub(crate) fn new(
        db: &'db dyn Db,
        env: &'c ProgramEnvironment<'db>,
        constraints: &'c ConstraintSetBuilder<'db>,
        inferable: TypeVarSet<'db>,
    ) -> Self {
        Self {
            db,
            env,
            constraints,
            inferable,
            pending: ConstraintSet::from_bool(constraints, true),
            types: FxHashMap::default(),
            paramspec_seen: FxHashSet::default(),
        }
    }

    /// Adds a constraint set to the pending specialization and projects its valid solutions into
    /// the legacy type mappings.
    pub(crate) fn add_constraint_set(
        &mut self,
        set: ConstraintSet<'db, 'c>,
    ) -> Result<(), SpecializationError<'db>> {
        self.infer_from_constraint_set(set)
    }

    /// Build a specialization, using a caller-provided hook to select the solution for each
    /// typevar.
    ///
    /// The `choose` hook is called for each typevar on the generic context with the typevar's
    /// explicit lower and upper bounds.
    /// Unmapped typevars receive `None` for their bounds and fall back to their default
    /// specialization if an alternative default type is not chosen.
    ///
    /// The hook should return `Some(ty)` to use `ty` as the specialization for this typevar, or
    /// `None` to use the inferred type unchanged.
    pub(crate) fn build_with(
        &mut self,
        generic_context: GenericContext<'db>,
        mut choose: impl FnMut(BoundTypeVarInstance<'db>, Option<&PathBound<'db>>) -> Option<Type<'db>>,
    ) -> Specialization<'db> {
        let db = self.db;
        let types = self
            .solve_pending_with(generic_context, &mut choose)
            .unwrap_or_else(|()| self.solve_hash_map_with(generic_context, &mut choose));
        let specialization =
            generic_context
                .variables_inner(db)
                .iter()
                .map(|(identity, variable)| {
                    types
                        .get(identity)
                        .copied()
                        .or_else(|| choose(*variable, None))
                });

        generic_context.specialize_recursive(db, specialization)
    }

    /// Build raw type-variable inference, preserving which type variables were left unsolved.
    ///
    /// Returns an error if the call-wide pending constraints are unsatisfiable.
    pub(crate) fn build_inference_with(
        &mut self,
        generic_context: GenericContext<'db>,
        mut choose: impl FnMut(BoundTypeVarInstance<'db>, Option<&PathBound<'db>>) -> Option<Type<'db>>,
    ) -> Result<TypeVarInference<'db>, ()> {
        let types = self.solve_pending_with(generic_context, &mut choose)?;
        Ok(self.typevar_inference(generic_context, &types))
    }

    /// Build a diagnostic specialization after the call-wide constraints were unsatisfiable.
    ///
    /// Each argument relation is solved independently, then its solutions are merged into the
    /// legacy type map. This preserves enough information to report the conflicting arguments
    /// even when a migrated inference path only populated `pending`.
    pub(crate) fn build_diagnostic_inference_with(
        &mut self,
        generic_context: GenericContext<'db>,
        argument_relations: impl IntoIterator<Item = (Type<'db>, Type<'db>)>,
        mut choose: impl FnMut(BoundTypeVarInstance<'db>, Option<&PathBound<'db>>) -> Option<Type<'db>>,
    ) -> TypeVarInference<'db> {
        let db = self.db;
        for (formal, actual) in argument_relations {
            let when =
                actual.when_constraint_set_assignable_to(db, self.env, formal, self.constraints);
            let _ = self.add_type_mappings_from_constraint_set(when);
        }

        let types = self.solve_hash_map_with(generic_context, &mut choose);
        self.typevar_inference(generic_context, &types)
    }

    fn typevar_inference(
        &self,
        generic_context: GenericContext<'db>,
        types: &FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,
    ) -> TypeVarInference<'db> {
        let db = self.db;
        let inferred: Box<[_]> = generic_context
            .variables_inner(db)
            .keys()
            .map(|identity| types.get(identity).copied())
            .collect();

        TypeVarInference::new(db, generic_context, inferred)
    }

    fn solve_pending_with(
        &mut self,
        generic_context: GenericContext<'db>,
        choose: &mut impl FnMut(BoundTypeVarInstance<'db>, Option<&PathBound<'db>>) -> Option<Type<'db>>,
    ) -> Result<FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>, ()> {
        let db = self.db;
        // TODO: Move `ParamSpec` and `TypeVarTuple` handling to the new constraint solver.
        if generic_context
            .variables_inner(db)
            .values()
            .any(|typevar| typevar.is_paramspec(db) || typevar.is_typevartuple(db))
        {
            return Ok(self.solve_hash_map_with(generic_context, choose));
        }

        // TODO: This projection / solve can be expensive for large-union collection-literal type
        // contexts. During the pending-constraint-set migration, pydantic and hydra-zen regressed
        // on empty dict literals with a context equivalent to
        // `dict[Hashable, pydantic_core.core_schema.CoreSchema]`. `CoreSchema` is a large union,
        // so the invariant `_VT@dict = CoreSchema` constraint expands to many lower-bound
        // constraints; combined with `_KT@dict = Hashable`, PathAssignments/SequentMap traversal
        // derives cross-typevar facts such as `TypedDictSchema <= _VT@dict <= Hashable`.
        //
        // A naive fast path that skipped `remove_noninferable` when no constraint would be removed
        // was not enough: `solutions_with` still performed the expensive path traversal, and the
        // skipped projection changed precision in LiteralString tests. See the
        // `ty_micro[pydantic_core_schema_dict]` benchmark for a minimized reproducer.
        let solutions = match self.pending.solutions_with(
            db,
            self.env,
            self.constraints,
            self.inferable,
            |_variance, path_bound| {
                let typevar = path_bound.bound_typevar;
                if let Some(ty) = choose(typevar, Some(path_bound)) {
                    return Ok(Some(ty));
                }

                PathBounds::default_solve(db, self.env, self.constraints, path_bound)
            },
        ) {
            Solutions::Unsatisfiable => return Err(()),
            Solutions::Unconstrained => {
                return Ok(self.solve_hash_map_with(generic_context, choose));
            }
            Solutions::Constrained(solutions) => solutions,
        };

        let mut types = FxHashMap::default();
        for solution in solutions {
            for binding in solution {
                let identity = binding.bound_typevar.identity(db);
                types
                    .entry(identity)
                    .and_modify(|existing| {
                        *existing =
                            UnionType::from_two_elements(db, self.env, *existing, binding.solution);
                    })
                    .or_insert(binding.solution);
            }
        }

        // Sequent-map transitivity can add relationships between inferable typevars to path
        // bounds. Those relationships are important while solving, but should not become recursive
        // specialization outputs when concrete bounds are available. (This is tested in "Generic
        // callable chains" in the call/function.md mdtest.)
        //
        // TODO: This is a solution-level projection. A more principled version would live in the
        // constraint-set solution extraction layer, taking an explicit domain of typevars to solve
        // for and existentially quantifying away the other typevars in that domain.
        for (identity, variable) in generic_context.variables_inner(db) {
            if let Some(ty) = types.get_mut(identity) {
                *ty = self.remove_inferable_typevar_artifacts_from_solution(*variable, *ty);
            }
        }

        // TODO: Replace this fallback with expanding-cycle detection in the constraint-set
        // solution layer.
        if types
            .iter()
            .any(|(identity, ty)| self.has_expanding_cycle(generic_context, &types, *identity, *ty))
        {
            // Recursive specialization cannot reach a fixed point when a cycle grows through an
            // embedded generic type, such as `SupportsAdd[T, S]`.
            Ok(self.solve_hash_map_with(generic_context, choose))
        } else {
            Ok(types)
        }
    }

    fn has_expanding_cycle(
        &self,
        generic_context: GenericContext<'db>,
        types: &FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,
        identity: BoundTypeVarIdentity<'db>,
        ty: Type<'db>,
    ) -> bool {
        let db = self.db;
        match ty {
            // A bare `T = U` edge only replaces one typevar with another; it does not wrap the
            // replacement in additional structure and therefore cannot grow during repeated
            // specialization.
            Type::TypeVar(_) => false,
            // Unions and intersections are flattened and deduplicated as they are constructed.
            // A cyclic reference directly inside one can add elements but cannot create
            // unbounded nesting. Keep looking inside its elements for a genuinely embedded edge.
            Type::Union(union) => union.elements(db).iter().any(|element| {
                self.has_expanding_cycle(generic_context, types, identity, *element)
            }),
            Type::Intersection(intersection) => intersection
                .iter_positive(db)
                .chain(intersection.iter_negative(db))
                .any(|element| self.has_expanding_cycle(generic_context, types, identity, element)),
            _ => any_over_type(db, self.env, ty, false, |nested| {
                nested.as_typevar().is_some_and(|dependency| {
                    let dependency = dependency.identity(db);
                    dependency != identity
                        && generic_context.contains(db, dependency)
                        && self.reaches_pending_typevar(
                            generic_context,
                            types,
                            dependency,
                            identity,
                            &RefCell::default(),
                        )
                })
            }),
        }
    }

    fn reaches_pending_typevar(
        &self,
        generic_context: GenericContext<'db>,
        types: &FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>>,
        identity: BoundTypeVarIdentity<'db>,
        target: BoundTypeVarIdentity<'db>,
        visited: &RefCell<FxHashSet<BoundTypeVarIdentity<'db>>>,
    ) -> bool {
        let db = self.db;
        if identity == target {
            return true;
        }
        if !visited.borrow_mut().insert(identity) {
            return false;
        }

        types.get(&identity).is_some_and(|ty| {
            any_over_type(db, self.env, *ty, false, |nested| {
                nested.as_typevar().is_some_and(|dependency| {
                    let dependency = dependency.identity(db);
                    // Recursive specialization skips a typevar's own slot. Only references
                    // through other mappings can recursively expand.
                    dependency != identity
                        && generic_context.contains(db, dependency)
                        && self.reaches_pending_typevar(
                            generic_context,
                            types,
                            dependency,
                            target,
                            visited,
                        )
                })
            })
        })
    }

    fn is_inferable_typevar_artifact(
        &self,
        target: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) -> bool {
        let db = self.db;
        let target_context = target.binding_context(db);
        let target_freshness = target.freshness(db);
        ty.as_typevar().is_some_and(|typevar| {
            // Relationships across binding contexts can intentionally remap one generic context
            // onto another, as with constructor `self` annotations. Relationships across fresh
            // occurrences preserve an outer generic value through a recursive call. Synthetic
            // contexts do not identify a single source-level binding, so they are not safe to
            // project either.
            !matches!(target_context, BindingContext::Synthetic(_))
                && typevar.is_inferable(db, self.inferable)
                && typevar.binding_context(db) == target_context
                && typevar.freshness(db) == target_freshness
        })
    }

    /// Remove inferable type variables introduced by transitivity within the target's binding
    /// context while preserving intentional relationships to other generic contexts.
    pub(crate) fn remove_inferable_typevar_artifacts_from_solution(
        &self,
        target: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) -> Type<'db> {
        let db = self.db;
        match ty {
            Type::Intersection(intersection)
                if intersection
                    .iter_positive(db)
                    .any(|element| !self.is_inferable_typevar_artifact(target, element)) =>
            {
                intersection.map_positive(db, self.env, |element| {
                    if self.is_inferable_typevar_artifact(target, *element) {
                        Type::object()
                    } else {
                        *element
                    }
                })
            }
            Type::Union(union)
                if union
                    .elements(db)
                    .iter()
                    .any(|element| !self.is_inferable_typevar_artifact(target, *element)) =>
            {
                union.map(db, self.env, |element| {
                    if self.is_inferable_typevar_artifact(target, *element) {
                        Type::Never
                    } else {
                        self.remove_inferable_typevar_artifacts_from_solution(target, *element)
                    }
                })
            }
            _ => ty,
        }
    }

    fn solve_hash_map_with(
        &mut self,
        generic_context: GenericContext<'db>,
        choose: &mut impl FnMut(BoundTypeVarInstance<'db>, Option<&PathBound<'db>>) -> Option<Type<'db>>,
    ) -> FxHashMap<BoundTypeVarIdentity<'db>, Type<'db>> {
        let db = self.db;
        generic_context
            .variables_inner(db)
            .iter()
            .filter_map(|(identity, variable)| {
                let mapped_ty = self
                    .types
                    .get_mut(identity)
                    .map(|accumulator| accumulator.get_or_build(db, self.env));
                let chosen = match mapped_ty {
                    Some(mapped_ty) => {
                        let path_bound = PathBound::exact(*variable, mapped_ty);
                        choose(*variable, Some(&path_bound)).unwrap_or(mapped_ty)
                    }
                    None => choose(*variable, None)?,
                };
                Some((*identity, chosen))
            })
            .collect()
    }

    fn insert_hash_map_type_mapping(
        &mut self,
        bound_typevar: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
    ) {
        let db = self.db;
        let identity = bound_typevar.identity(db);
        match self.types.entry(identity) {
            Entry::Occupied(mut entry) => {
                match bound_typevar.kind(db) {
                    TypeVarKind::LegacyParamSpec | TypeVarKind::Pep695ParamSpec => {
                        // TODO: The spec says that when a ParamSpec is used multiple times in a signature,
                        // the type checker can solve it to a common behavioral supertype. We don't
                        // implement that yet so in case there are multiple ParamSpecs, use the
                        // specialization from the first occurrence.
                        // https://github.com/astral-sh/ty/issues/1778
                        // https://github.com/astral-sh/ruff/pull/21445#discussion_r2591510145
                    }
                    TypeVarKind::LegacyTypeVarTuple | TypeVarKind::Pep695TypeVarTuple => {
                        // Repeated uses of a `TypeVarTuple` must have the same length, but the typing
                        // spec leaves the exact inference behavior unspecified. Merge equal-length
                        // candidates element-wise using unions.
                        // https://typing.python.org/en/latest/spec/generics.html#type-variable-tuple-equality
                        let accumulator = entry.get_mut();
                        let existing = accumulator.get_or_build(db, self.env);
                        if existing == ty {
                            return;
                        }
                        let Some(existing_tuple) = existing.exact_tuple_instance_spec(db) else {
                            return;
                        };
                        let Some(new_tuple) = ty.exact_tuple_instance_spec(db) else {
                            return;
                        };
                        if existing_tuple.len() != new_tuple.len() {
                            return;
                        }
                        let unioned = TupleSpecBuilder::from(existing_tuple.as_ref())
                            .union(db, self.env, &new_tuple)
                            .build();
                        *accumulator = UnionAccumulator::new(Type::tuple(TupleType::new(
                            db, self.env, &unioned,
                        )));
                    }
                    _ => {
                        entry.get_mut().add(db, self.env, ty);
                    }
                }
            }
            Entry::Vacant(entry) => {
                entry.insert(UnionAccumulator::new(ty));
            }
        }
    }

    fn intersect_pending_typevar_constraint(
        &mut self,
        bound_typevar: BoundTypeVarInstance<'db>,
        bounds: ConstraintBounds<'db>,
    ) {
        let db = self.db;
        let identity = bound_typevar.identity(db);
        if bound_typevar.is_paramspec(db) && !self.paramspec_seen.insert(identity) {
            return;
        }

        let constraint = ConstraintSet::constrain_typevar_with_bounds(
            db,
            self.env,
            self.constraints,
            bound_typevar,
            bounds.lower,
            bounds.upper,
        );
        self.pending.intersect(db, self.constraints, constraint);
    }

    pub(crate) fn inferred_type_is_assignable_to(
        &mut self,
        bound_typevar: BoundTypeVarIdentity<'db>,
        ty: Type<'db>,
    ) -> bool {
        let db = self.db;
        self.types
            .get_mut(&bound_typevar)
            .is_some_and(|inferred_ty| {
                inferred_ty
                    .get_or_build(db, self.env)
                    .is_assignable_to(db, self.env, ty)
            })
    }

    /// Add a type mapping for a bound typevar using the given variance to determine how the
    /// inferred type constrains the typevar.
    pub(crate) fn add_type_mapping(
        &mut self,
        bound_typevar: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
        variance: TypeVarVariance,
    ) {
        self.insert_hash_map_type_mapping(bound_typevar, ty);
        self.insert_pending_type_mapping(bound_typevar, ty, variance);
    }

    fn insert_pending_type_mapping(
        &mut self,
        bound_typevar: BoundTypeVarInstance<'db>,
        ty: Type<'db>,
        variance: TypeVarVariance,
    ) {
        let bounds = match variance {
            TypeVarVariance::Covariant => ConstraintBounds::new(Some(ty), None),
            TypeVarVariance::Contravariant => ConstraintBounds::new(None, Some(ty)),
            TypeVarVariance::Invariant => ConstraintBounds::exact(ty),
            TypeVarVariance::Bivariant => return,
        };
        self.intersect_pending_typevar_constraint(bound_typevar, bounds);
    }

    /// Finds all of the valid specializations of a constraint set, and adds their type mappings to
    /// the specialization that this builder is building up.
    ///
    /// TODO: This is a stopgap! Eventually, the builder will maintain a single constraint set for
    /// the main specialization that we are building, and [`build_with`][Self::build_with] will
    /// build the specialization directly from that constraint set. This method lets us migrate to
    /// that brave new world incrementally, by using the new constraint set mechanism piecemeal for
    /// certain type comparisons.
    fn add_type_mappings_from_constraint_set(
        &mut self,
        set: ConstraintSet<'db, 'c>,
    ) -> Result<(), ConstraintSetInferenceError<'db>> {
        let db = self.db;
        let mut first_error = None;
        let solutions = match set.solutions_with(
            db,
            self.env,
            self.constraints,
            self.inferable,
            |_variance, path_bound| {
                let solution =
                    PathBounds::preliminary_solve(db, self.env, self.constraints, path_bound);
                if solution.is_err() && first_error.is_none() {
                    first_error = self.specialization_error_from_failed_bounds(path_bound);
                }
                solution
            },
        ) {
            Solutions::Unsatisfiable => {
                return Err(first_error.map_or(
                    ConstraintSetInferenceError::Unsatisfiable,
                    ConstraintSetInferenceError::InvalidTypeVar,
                ));
            }
            Solutions::Unconstrained => return Ok(()),
            Solutions::Constrained(solutions) => solutions,
        };
        for solution in solutions {
            for binding in solution {
                let solution = self.remove_inferable_typevar_artifacts_from_solution(
                    binding.bound_typevar,
                    binding.solution,
                );
                self.insert_hash_map_type_mapping(binding.bound_typevar, solution);
            }
        }
        Ok(())
    }

    /// Returns an actionable type-variable error for a failed projected path.
    ///
    /// Conflicting inferred lower and upper bounds are not necessarily violations of the type
    /// variable's declaration, so they remain generic unsatisfiable constraints.
    fn specialization_error_from_failed_bounds(
        &self,
        path_bound: &PathBound<'db>,
    ) -> Option<SpecializationError<'db>> {
        let db = self.db;
        let bound_typevar = path_bound.bound_typevar;
        let argument = path_bound.lower?;
        match bound_typevar
            .typevar(db)
            .bound_or_constraints(db, self.env)?
        {
            TypeVarBoundOrConstraints::UpperBound(bound) => (!argument
                .when_assignable_to(db, self.env, bound, self.constraints, self.inferable)
                .is_always_satisfied(db, self.env))
            .then_some(SpecializationError::MismatchedBound {
                bound_typevar,
                argument,
            }),
            TypeVarBoundOrConstraints::Constraints(_) => {
                (!path_bound.has_upper()).then_some(SpecializationError::MismatchedConstraint {
                    bound_typevar,
                    argument,
                })
            }
        }
    }

    /// Adds legacy type mappings from `when` and records it in the call-wide constraint set.
    ///
    /// Generic unsatisfiability is retained in `pending`; only failures against a type variable's
    /// declared bound or constraints are returned for immediate diagnosis.
    fn infer_from_constraint_set(
        &mut self,
        when: ConstraintSet<'db, 'c>,
    ) -> Result<(), SpecializationError<'db>> {
        let db = self.db;
        let result = self.add_type_mappings_from_constraint_set(when);
        self.pending.intersect(db, self.constraints, when);
        match result {
            Ok(()) | Err(ConstraintSetInferenceError::Unsatisfiable) => Ok(()),
            Err(ConstraintSetInferenceError::InvalidTypeVar(error)) => Err(error),
        }
    }

    /// Returns the assignability constraints required by this comparison's polarity.
    fn constraint_for_relation(
        &self,
        formal: Type<'db>,
        actual: Type<'db>,
        polarity: TypeVarVariance,
    ) -> ConstraintSet<'db, 'c> {
        let db = self.db;
        relation_directions(formal, actual, polarity).when_all(
            db,
            self.constraints,
            |(source, target)| {
                let when = source.when_constraint_set_assignable_to_owned(db, self.env, target);
                self.constraints.load(db, self.env, &when)
            },
        )
    }

    /// Returns common protocol constraints for the `TypedDict` members of a union when every such
    /// member has the same constraints as their shared `Mapping[str, object]` fallback.
    fn common_typed_dict_protocol_constraints(
        &self,
        formal: Type<'db>,
        actual: UnionType<'db>,
    ) -> Option<ConstraintSet<'db, 'c>> {
        fn is_string_keyed_mapping<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
        ) -> bool {
            let Type::NominalInstance(instance) = ty.resolve_type_alias(db) else {
                return false;
            };

            matches!(
                instance.class(db, env).known(db),
                Some(
                    KnownClass::Dict
                        | KnownClass::Mapping
                        | KnownClass::MutableMapping
                        | KnownClass::DefaultDict
                        | KnownClass::ChainMap
                        | KnownClass::OrderedDict
                )
            ) && instance
                .class(db, env)
                .into_generic_alias()
                .is_some_and(|alias| {
                    matches!(
                        alias.specialization(db).types(db),
                        [key, _] if key.resolve_type_alias(db) == KnownClass::Str.to_instance(db, env)
                    )
                })
        }

        fn collect_typed_dicts<'db>(
            db: &'db dyn Db,
            env: &ProgramEnvironment<'db>,
            ty: Type<'db>,
            resolving: &mut FxHashSet<Type<'db>>,
            completed: &mut FxHashMap<Type<'db>, bool>,
            typed_dicts: &mut FxHashSet<Type<'db>>,
            other_types: &mut FxOrderSet<Type<'db>>,
        ) -> bool {
            let ty = ty.resolve_type_alias(db);
            if let Some(result) = completed.get(&ty) {
                return *result;
            }

            let result = match ty {
                Type::TypedDict(_) => {
                    typed_dicts.insert(ty);
                    true
                }
                Type::Union(union) => {
                    if !resolving.insert(ty) {
                        return false;
                    }
                    let result = union.elements(db).iter().all(|element| {
                        collect_typed_dicts(
                            db,
                            env,
                            *element,
                            resolving,
                            completed,
                            typed_dicts,
                            other_types,
                        )
                    });
                    resolving.remove(&ty);
                    result
                }
                Type::Intersection(intersection)
                    if intersection.negative(db).is_empty()
                        && intersection
                            .iter_positive(db)
                            .any(|element| element.resolve_type_alias(db).is_typed_dict())
                        && intersection.iter_positive(db).all(|element| {
                            let element = element.resolve_type_alias(db);
                            element.is_typed_dict()
                                || element
                                    == KnownClass::Dict
                                        .to_instance_unknown(db, env)
                                        .top_materialization(db, env)
                        }) =>
                {
                    // `isinstance(value, dict)` narrows a `TypedDict` to an intersection with
                    // `Top[dict[Unknown, Unknown]]`. Other conjuncts may contribute gradual
                    // constraints that the shared mapping would erase.
                    typed_dicts.insert(ty);
                    true
                }
                Type::Intersection(intersection)
                    if intersection.negative(db).is_empty()
                        && intersection
                            .iter_positive(db)
                            .any(|element| is_string_keyed_mapping(db, env, element))
                        && intersection.iter_positive(db).all(|element| {
                            let element = element.resolve_type_alias(db);
                            is_string_keyed_mapping(db, env, element)
                                || element
                                    == KnownClass::Dict
                                        .to_instance_unknown(db, env)
                                        .top_materialization(db, env)
                        }) =>
                {
                    // `isinstance(value, dict)` can also narrow a mapping to an intersection with
                    // `Top[dict[Unknown, Unknown]]`. Retain the full intersection so its original
                    // key and value constraints are preserved.
                    other_types.insert(ty);
                    true
                }
                Type::NominalInstance(_) if is_string_keyed_mapping(db, env, ty) => {
                    other_types.insert(ty);
                    true
                }
                _ => false,
            };
            completed.insert(ty, result);
            result
        }
        let db = self.db;

        let mut resolving = FxHashSet::default();
        let mut completed = FxHashMap::default();
        let mut typed_dicts = FxHashSet::default();
        let mut other_types = FxOrderSet::default();
        let env = self.env;

        if !actual.elements(db).iter().all(|element| {
            collect_typed_dicts(
                db,
                env,
                *element,
                &mut resolving,
                &mut completed,
                &mut typed_dicts,
                &mut other_types,
            )
        }) {
            return None;
        }
        if typed_dicts.is_empty() {
            return None;
        }
        // Other protocols can observe key-specific or gradual evidence that the shared mapping
        // fallback erases; restrict mixed unions to the protocol used by dictionary constructors.
        if !other_types.is_empty()
            && !matches!(formal, Type::ProtocolInstance(protocol)
            if protocol.class_origin(db).is_some_and(|class| {
                class.is_known(db, KnownClass::SupportsKeysAndGetItem)
            }))
        {
            return None;
        }

        // Use the read-only `Mapping[str, object]` as the fallback rather than `dict[str, object]`.
        // The current constraint solver can consider mutable protocol constraints equivalent even
        // when a `TypedDict` preserves more precise correlations between its keys and values.
        let spec = &[KnownClass::Str.to_instance(db, env), Type::object()];
        let mapping = KnownClass::Mapping.to_specialized_instance(db, env, spec);
        let mapping_when = mapping.when_constraint_set_assignable_to_owned(db, env, formal);
        let mapping_when = self.constraints.load(db, env, &mapping_when);
        // Logically equivalent constraints can still infer different solutions, such as `Any`
        // instead of `object`; preserve the original constraints when gradual evidence differs.
        let mapping_solutions = mapping_when.solutions(db, env, self.constraints, self.inferable);
        if !typed_dicts.into_iter().all(|element| {
            let element_when = self.constraints.load(
                db,
                env,
                &element.when_constraint_set_assignable_to_owned(db, env, formal),
            );
            element_when
                .iff(db, self.constraints, mapping_when)
                .is_always_satisfied(db, env)
                && element_when.solutions(db, env, self.constraints, self.inferable)
                    == mapping_solutions
        }) {
            return None;
        }

        // Reuse one constraint for all equivalent TypedDicts, but retain each mapping arm's
        // original constraints.
        Some(mapping_when.and(db, self.constraints, || {
            other_types
                .into_iter()
                .when_all(db, self.constraints, |element| {
                    self.constraints.load(
                        db,
                        env,
                        &element.when_constraint_set_assignable_to_owned(db, env, formal),
                    )
                })
        }))
    }

    /// Infer type mappings by comparing formal callable signatures against actual callables.
    ///
    /// Unsatisfiable relations are recorded in `pending`; only type-variable declaration failures
    /// are returned for immediate diagnosis.
    fn infer_from_callable_signature(
        &mut self,
        formal: CallableType<'db>,
        actual_callables: CallableTypes<'db>,
        polarity: TypeVarVariance,
    ) -> Result<(), SpecializationError<'db>> {
        let db = self.db;
        if !matches!(polarity, TypeVarVariance::Covariant) {
            let actual = actual_callables
                .map(|callable| callable.into_regular(db))
                .into_type(db, self.env);
            let formal = Type::Callable(formal.into_regular(db));
            let when = self.constraint_for_relation(formal, actual, polarity);
            return self.infer_from_constraint_set(when);
        }

        let formal_signature = formal.signatures(db);
        let formal_is_single_paramspec = formal_signature.is_single_paramspec().is_some();
        for actual_callable in actual_callables.as_slice() {
            if formal_is_single_paramspec {
                let when = actual_callable
                    .signatures(db)
                    .when_constraint_set_assignable_to(
                        db,
                        self.env,
                        formal_signature,
                        self.constraints,
                    );
                self.infer_from_constraint_set(when)?;
            } else {
                // An overloaded actual callable is compatible with the formal signature if at
                // least one of its overloads is. We collect type mappings from all satisfiable
                // overloads, and only report an error if none of them are satisfiable.

                let env = self.env.clone();
                let constraints = self.constraints;
                let mut first_error = None;
                let combined = actual_callable
                    .signatures(db)
                    .overloads
                    .iter()
                    .filter_map(|actual_signature| {
                        let when = actual_signature.when_constraint_set_assignable_to_signatures(
                            db,
                            &env,
                            formal_signature,
                            constraints,
                        );
                        match self.add_type_mappings_from_constraint_set(when) {
                            Ok(()) => Some(when),
                            Err(error) => {
                                first_error.get_or_insert(error);
                                None
                            }
                        }
                    })
                    .reduce(|lhs, rhs| lhs.or(db, constraints, || rhs));
                let Some(combined) = combined else {
                    self.pending = ConstraintSet::from_bool(self.constraints, false);
                    if let Some(ConstraintSetInferenceError::InvalidTypeVar(error)) = first_error {
                        return Err(error);
                    }
                    return Ok(());
                };
                self.pending.intersect(db, self.constraints, combined);
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
        self.infer_map_impl(
            formal,
            actual,
            TypeVarVariance::Covariant,
            &mut FxHashSet::default(),
        )
    }

    fn infer_map_impl(
        &mut self,
        formal: Type<'db>,
        actual: Type<'db>,
        polarity: TypeVarVariance,
        seen: &mut FxHashSet<(Type<'db>, Type<'db>, TypeVarVariance)>,
    ) -> Result<(), SpecializationError<'db>> {
        let db = self.db;
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

        // Avoid infinite recursion while retaining comparisons under different polarities.
        if !seen.insert((formal, actual, polarity)) {
            return Ok(());
        }

        // Remove the union elements from `actual` that are not related to `formal`, and vice
        // versa.
        //
        // For example, if `formal` is `list[T]` and `actual` is `list[int] | None`, we want to
        // specialize `T` to `int`, and so ignore the `None`.
        let actual = actual.filter_disjoint_elements(db, self.env, formal, self.inferable);
        let formal = formal.filter_disjoint_elements(db, self.env, actual, self.inferable);

        // ParamSpecs and TypeVarTuples still use the forward-only legacy mapping table. Keep
        // their entire inference context on the existing signature path, and use forward
        // structural relations so nested variadics and ordinary type variables retain their
        // mappings. Preserve the original polarity for recursive and ordinary inference.
        // TODO: Apply full polarity once variadics are supported by the new constraint solver.
        let relation_polarity = if !polarity.is_covariant()
            && self
                .inferable
                .iter(db)
                .any(|typevar| typevar.is_paramspec(db) || typevar.is_typevartuple(db))
        {
            TypeVarVariance::Covariant
        } else {
            polarity
        };

        match (formal, actual) {
            // Expand PEP 695 type aliases in the formal type.
            // This is necessary for solving generics like `def head[T](my_list: MyList[T]) -> T`.
            (Type::TypeAlias(alias), _) => {
                return self.infer_map_impl(alias.value_type(db), actual, polarity, seen);
            }

            (Type::TypeForm(formal_typeform), Type::TypeForm(actual_typeform)) => {
                let variance = TypeVarVariance::Covariant.compose(polarity);
                return self.infer_map_impl(
                    formal_typeform.type_argument(db),
                    actual_typeform.type_argument(db),
                    variance,
                    seen,
                );
            }

            (
                Type::TypeForm(formal_typeform),
                actual @ (Type::ClassLiteral(_) | Type::GenericAlias(_) | Type::SubclassOf(_)),
            ) => {
                let variance = TypeVarVariance::Covariant.compose(polarity);
                if let Some(actual_instance) = actual.to_instance_approximation(db, self.env) {
                    return self.infer_map_impl(
                        formal_typeform.type_argument(db),
                        actual_instance,
                        variance,
                        seen,
                    );
                }
            }

            (Type::TypeForm(formal_typeform), Type::KnownInstance(actual_instance))
                if let Some(actual_argument) = actual_instance.type_form_argument(db, self.env) =>
            {
                let variance = TypeVarVariance::Covariant.compose(polarity);
                return self.infer_map_impl(
                    formal_typeform.type_argument(db),
                    actual_argument,
                    variance,
                    seen,
                );
            }

            (Type::TypeForm(formal_typeform), Type::SpecialForm(actual_form)) => {
                let variance = TypeVarVariance::Covariant.compose(polarity);
                if let Some(actual_argument) = actual_form.type_form_argument(db, self.env) {
                    return self.infer_map_impl(
                        formal_typeform.type_argument(db),
                        actual_argument,
                        variance,
                        seen,
                    );
                }
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
                    .elements(db)
                    .iter()
                    .filter(|ty| ty.has_typevar(db, self.env));
                let Ok(Type::TypeVar(formal_bound_typevar)) = types_have_typevars.exactly_one()
                else {
                    return Ok(());
                };
                if actual_union.elements(db).iter().any(|ty| ty.is_type_var()) {
                    return Ok(());
                }
                let remaining_actual =
                    actual_union.filter(db, |ty| !ty.is_subtype_of(db, self.env, formal));
                if remaining_actual.is_never() {
                    return Ok(());
                }
                // Infer through the TypeVar arm so its bound or constraints are still enforced.
                return self.infer_map_impl(
                    Type::TypeVar(*formal_bound_typevar),
                    remaining_actual,
                    polarity,
                    seen,
                );
            }
            (Type::Union(union_formal), _) => {
                // If the formal is a union and the actual is a bare inferable TypeVar in an
                // invariant position, record the whole union as the mapping. Invariant matching is
                // equality-like; probing individual union elements below can leave spurious
                // partial mappings from non-matching elements. For example, while comparing
                // `ClassSelector[T]` with `ClassSelector[CT | None]`, descending into `None`
                // would map `T` to `None` before `CT` is solved from another argument.
                if let Type::TypeVar(actual_typevar) = actual
                    && actual_typevar.is_inferable(db, self.inferable)
                    && matches!(polarity, TypeVarVariance::Invariant)
                {
                    self.add_type_mapping(actual_typevar, formal, polarity);
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
                    let assignable_elements = union_formal.elements(db).iter().filter(|ty| {
                        actual
                            .when_subtype_of(db, self.env, **ty, self.constraints, self.inferable)
                            .is_always_satisfied(db, self.env)
                    });
                    if assignable_elements.exactly_one().is_ok() {
                        return Ok(());
                    }
                }

                let mut bound_typevars = union_formal
                    .elements(db)
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
                for formal_element in union_formal.elements(db) {
                    let result = self.infer_map_impl(*formal_element, actual, polarity, seen);
                    if let Err(err) = result {
                        first_error.get_or_insert(err);
                    } else {
                        // The recursive call to `infer_map_impl` may succeed even if the actual type is
                        // not assignable to the formal element.
                        if !actual
                            .when_assignable_to(
                                db,
                                self.env,
                                *formal_element,
                                self.constraints,
                                self.inferable,
                            )
                            .is_never_satisfied(db, self.env)
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
                if bound_typevar.is_inferable(db, self.inferable) =>
            {
                match bound_typevar.typevar(db).bound_or_constraints(db, self.env) {
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
                                IntersectionType::from_two_elements(db, self.env, bound, ty),
                                polarity,
                            );
                            return Ok(());
                        }
                        if !ty
                            .when_assignable_to(
                                db,
                                self.env,
                                bound,
                                self.constraints,
                                self.inferable,
                            )
                            .is_always_satisfied(db, self.env)
                        {
                            return Err(SpecializationError::MismatchedBound {
                                bound_typevar,
                                argument: ty,
                            });
                        }
                        self.add_type_mapping(bound_typevar, ty, polarity);
                    }
                    Some(TypeVarBoundOrConstraints::Constraints(typevar_constraints)) => {
                        // Prefer an exact match first.
                        for constraint in typevar_constraints.elements(db) {
                            if ty == *constraint {
                                self.add_type_mapping(bound_typevar, ty, polarity);
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
                                actual_typevar.typevar(db).constraints(db, self.env)
                        {
                            let all_satisfied =
                                actual_constraints.iter().all(|actual_constraint| {
                                    typevar_constraints.elements(db).iter().any(
                                        |formal_constraint| {
                                            actual_constraint.is_equivalent_to(
                                                db,
                                                self.env,
                                                *formal_constraint,
                                            )
                                        },
                                    )
                                });
                            if all_satisfied {
                                self.add_type_mapping(bound_typevar, ty, polarity);
                                return Ok(());
                            }
                        }

                        for constraint in typevar_constraints.elements(db) {
                            let is_satisfied = if polarity.is_contravariant() {
                                constraint
                                    .when_assignable_to(
                                        db,
                                        self.env,
                                        ty,
                                        self.constraints,
                                        self.inferable,
                                    )
                                    .is_always_satisfied(db, self.env)
                            } else {
                                ty.when_assignable_to(
                                    db,
                                    self.env,
                                    *constraint,
                                    self.constraints,
                                    self.inferable,
                                )
                                .is_always_satisfied(db, self.env)
                            };

                            if is_satisfied {
                                // For the old solver, we use the constraint itself as the mapped
                                // type, since the old solver's hash map stores solutions. For the
                                // new solver's pending constraint set, we store the type that
                                // matched against the constraint. This is especially important
                                // when the matched type is gradual, since it might match multiple
                                // constraints, and we need the constraint set to be able to reason
                                // about all of them.
                                self.insert_hash_map_type_mapping(bound_typevar, *constraint);
                                self.insert_pending_type_mapping(bound_typevar, ty, polarity);
                                return Ok(());
                            }
                        }
                        return Err(SpecializationError::MismatchedConstraint {
                            bound_typevar,
                            argument: ty,
                        });
                    }
                    _ => self.add_type_mapping(bound_typevar, ty, polarity),
                }
            }

            (Type::Intersection(formal_intersection), _) => {
                // The actual type must be assignable to every (positive) element of the
                // formal intersection, so we must infer type mappings for each of them. (The
                // actual type must also be disjoint from every negative element of the
                // intersection, but that doesn't help us infer any type mappings.)
                for positive in formal_intersection.iter_positive(db) {
                    self.infer_map_impl(positive, actual, polarity, seen)?;
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
                for positive in actual_intersection.iter_positive(db) {
                    let result = self.infer_map_impl(formal, positive, polarity, seen);
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
                            .when_assignable_to(
                                db,
                                self.env,
                                formal,
                                self.constraints,
                                self.inferable,
                            )
                            .is_never_satisfied(db, self.env)
                        {
                            found_matching_element = true;
                        }
                    }
                }
                if !found_matching_element && let Some(error) = first_error {
                    return Err(error);
                }
            }

            (
                Type::SubclassOf(formal_subclass),
                actual @ (Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::Union(_)),
            ) if let SubclassOfInner::Protocol(protocol) = formal_subclass.subclass_of() => {
                let formal_protocol = Type::ProtocolInstance(protocol);
                if let Type::Union(union) = actual {
                    for element in union.elements(db) {
                        self.infer_map_impl(
                            formal_protocol,
                            element.bindings(db, self.env).return_type(db, self.env),
                            polarity,
                            seen,
                        )?;
                    }
                    return Ok(());
                }
                return self.infer_map_impl(
                    formal_protocol,
                    actual.bindings(db, self.env).return_type(db, self.env),
                    polarity,
                    seen,
                );
            }

            (
                formal @ (Type::SubclassOf(_) | Type::GenericAlias(_)),
                Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::Union(_),
            ) if formal.is_generic_alias()
                || matches!(formal, Type::SubclassOf(subclass)
                    if matches!(subclass.subclass_of(), SubclassOfInner::Class(_))) =>
            {
                let when = self.constraint_for_relation(formal, actual, relation_polarity);
                return self.infer_from_constraint_set(when);
            }

            (Type::SubclassOf(subclass_of), ty) | (ty, Type::SubclassOf(subclass_of))
                if let Some(type_var) = subclass_of.into_type_var()
                    && let Some(actual_instance) = ty.to_instance_approximation(db, self.env) =>
            {
                return self.infer_map_impl(
                    Type::TypeVar(type_var),
                    actual_instance,
                    polarity,
                    seen,
                );
            }

            (
                formal @ (Type::NominalInstance(_) | Type::ProtocolInstance(_)),
                Type::LiteralValue(literal),
            ) => {
                // Retry specialization with the literal's fallback instance so literals can
                // contribute to generic inference for nominal and protocol formals.
                let actual_instance = literal.fallback_instance(db, self.env);
                return self.infer_map_impl(formal, actual_instance, polarity, seen);
            }

            (
                formal @ (Type::NominalInstance(_) | Type::ProtocolInstance(_)),
                Type::KnownInstance(known_instance @ KnownInstanceType::Range { .. }),
            ) => {
                // `range(...)` is a known instance only to preserve its truthiness; use the
                // ordinary `range` instance when inferring through generic nominal/protocol types.
                return self.infer_map_impl(
                    formal,
                    known_instance.instance_fallback(db, self.env),
                    polarity,
                    seen,
                );
            }

            (formal, Type::ProtocolInstance(actual_protocol)) => {
                if let Type::ProtocolInstance(formal_protocol) = formal
                    && let Some(actual_origin) = actual_protocol.materialized_origin(db)
                    && let Some(formal_origin) = formal_protocol.class_origin(db)
                {
                    // Materialized protocols cannot be replaced by their nominal origin: doing
                    // so would recover the original `Any` requirements. Infer from the complete
                    // interface when doing so is cycle-safe; otherwise use its nonrecursive
                    // requirements and leave full recursive compatibility to argument checking.
                    let when = relation_directions(
                        (formal_protocol, formal_origin),
                        (actual_protocol, actual_origin),
                        relation_polarity,
                    )
                    .try_fold(
                        ConstraintSet::from_bool(self.constraints, true),
                        |mut combined, ((source, source_origin), (target, target_origin))| {
                            if combined.is_trivially_never_satisfied() {
                                return Some(combined);
                            }

                            let when = if source_origin
                                .is_subtype_of_class_literal(db, target_origin.class_literal(db))
                                || target.interface(db).has_only_finite_members(db)
                            {
                                Type::ProtocolInstance(source)
                                    .when_constraint_set_assignable_to_owned(
                                        db,
                                        self.env,
                                        Type::ProtocolInstance(target),
                                    )
                            } else {
                                Cow::Borrowed(
                                    source.when_non_recursive_members_assignable_to_owned(
                                        db, target,
                                    )?,
                                )
                            };
                            let next = self.constraints.load(db, self.env, &when);
                            Some(combined.intersect(db, self.constraints, next))
                        },
                    );

                    if let Some(when) = when {
                        return self.infer_from_constraint_set(when);
                    }
                }

                // Converting the actual protocol to its nominal origin makes the reversed
                // comparison impossible: a protocol cannot be assignable to a nominal class.
                if matches!(formal, Type::ProtocolInstance(_)) && !relation_polarity.is_covariant()
                {
                    let when = self.constraint_for_relation(formal, actual, relation_polarity);
                    return self.infer_from_constraint_set(when);
                }

                // TODO: This will only handle protocol classes that explicit inherit
                // from other generic protocol classes by listing it as a base class.
                // To handle classes that implicitly implement a generic protocol, we
                // will need to check the types of the protocol members to be able to
                // infer the specialization of the protocol that the class implements.
                if let Some(actual_nominal) = actual_protocol.nominal_origin_instance(db) {
                    return self.infer_map_impl(
                        formal,
                        Type::NominalInstance(actual_nominal),
                        polarity,
                        seen,
                    );
                }
            }

            // Special case: `formal` and `actual` are both tuples.
            (Type::NominalInstance(formal), Type::NominalInstance(actual))
                if let Some(formal_tuple) = formal.tuple_spec(db, self.env)
                    && let Some(actual_tuple) = actual.tuple_spec(db, self.env) =>
            {
                if let TupleSpec::Variable(formal_variable) = &*formal_tuple
                    && let VariableSegment::TypeVarTuple(typevartuple) = formal_variable.variable()
                {
                    let formal_prefix_len = formal_variable.prefix_elements().len();
                    let formal_suffix_len = formal_variable.suffix_elements().len();
                    let (actual_prefix, packed, actual_suffix) = match &*actual_tuple {
                        TupleSpec::Fixed(actual) => {
                            let Some(middle_end) = actual.len().checked_sub(formal_suffix_len)
                            else {
                                return Ok(());
                            };
                            if middle_end < formal_prefix_len {
                                return Ok(());
                            }

                            let elements = actual.elements_slice();
                            (
                                &elements[..formal_prefix_len],
                                Type::heterogeneous_tuple(
                                    db,
                                    self.env,
                                    elements[formal_prefix_len..middle_end].iter().copied(),
                                ),
                                &elements[middle_end..],
                            )
                        }
                        TupleSpec::Variable(actual) => {
                            let actual_prefix_elements = actual.prefix_elements();
                            let actual_suffix_elements = actual.suffix_elements();
                            if actual_prefix_elements.len() < formal_prefix_len
                                || actual_suffix_elements.len() < formal_suffix_len
                            {
                                return Ok(());
                            }

                            let suffix_start = actual_suffix_elements.len() - formal_suffix_len;
                            (
                                &actual_prefix_elements[..formal_prefix_len],
                                Type::tuple(TupleType::mixed_with_segment(
                                    db,
                                    self.env,
                                    actual_prefix_elements[formal_prefix_len..].iter().copied(),
                                    actual.variable(),
                                    actual_suffix_elements[..suffix_start].iter().copied(),
                                )),
                                &actual_suffix_elements[suffix_start..],
                            )
                        }
                    };
                    let variance = TypeVarVariance::Covariant.compose(polarity);
                    for (formal_element, actual_element) in
                        formal_variable.prefix_elements().iter().zip(actual_prefix)
                    {
                        self.infer_map_impl(*formal_element, *actual_element, variance, seen)?;
                    }
                    for (formal_element, actual_element) in
                        formal_variable.suffix_elements().iter().zip(actual_suffix)
                    {
                        self.infer_map_impl(*formal_element, *actual_element, variance, seen)?;
                    }
                    self.add_type_mapping(typevartuple, packed, variance);
                    return Ok(());
                }

                let Some(most_precise_length) = formal_tuple.len().most_precise(actual_tuple.len())
                else {
                    return Ok(());
                };
                let Ok(formal_tuple) = formal_tuple.resize(db, self.env, most_precise_length)
                else {
                    return Ok(());
                };
                let Ok(actual_tuple) = actual_tuple.resize(db, self.env, most_precise_length)
                else {
                    return Ok(());
                };
                for (formal_element, actual_element) in formal_tuple
                    .iter_element_types(db)
                    .zip(actual_tuple.iter_element_types(db))
                {
                    let variance = TypeVarVariance::Covariant.compose(polarity);
                    self.infer_map_impl(formal_element, actual_element, variance, seen)?;
                }
                return Ok(());
            }

            (
                formal @ (Type::NominalInstance(_) | Type::ProtocolInstance(_)),
                Type::NominalInstance(actual_nominal),
            ) => {
                // Extract formal_alias if this is a generic class
                let formal_alias = match formal {
                    Type::NominalInstance(formal_nominal) => {
                        formal_nominal.class(db, self.env).into_generic_alias()
                    }

                    Type::ProtocolInstance(_) => {
                        // TODO: For protocols, we use the new constraint set implementation, which
                        // will handle implicitly implemented protocols and generic protocols. We
                        // eventually want this logic to be used for _all_ nominal instances
                        // (replacing the logic below).
                        let when = self.constraint_for_relation(formal, actual, relation_polarity);
                        return self.infer_from_constraint_set(when);
                    }

                    _ => None,
                };

                if let Some(formal_alias) = formal_alias {
                    let formal_origin = formal_alias.origin(db);
                    for base in actual_nominal.class(db, self.env).iter_mro(db) {
                        let ClassBase::Class(ClassType::Generic(base_alias)) = base else {
                            continue;
                        };
                        if formal_origin != base_alias.origin(db) {
                            continue;
                        }
                        let generic_context = formal_alias
                            .specialization(db)
                            .generic_context(db)
                            .variables(db);
                        let formal_specialization = formal_alias.specialization(db).types(db);
                        let base_specialization = base_alias.specialization(db).types(db);
                        for (typevar, formal_ty, base_ty) in itertools::izip!(
                            generic_context,
                            formal_specialization,
                            base_specialization
                        ) {
                            let variance = typevar.variance_with_polarity(db, polarity);
                            self.infer_map_impl(*formal_ty, *base_ty, variance, seen)?;
                        }
                        return Ok(());
                    }
                }
            }

            // TODO: in principle this could be a generalized Union-actual arm that maps over the
            // union, but the old solver isn't well-equipped to handle that (due to side effects
            // from even failed matches), so for now we handle this particular case.
            (formal @ Type::ProtocolInstance(_), actual @ Type::Union(actual_union)) => {
                // Common TypedDict constraints prove only `actual <= formal`. Contravariance
                // reverses that relation, while invariance additionally requires the reverse.
                let when = if matches!(relation_polarity, TypeVarVariance::Covariant)
                    && let Some(common) =
                        self.common_typed_dict_protocol_constraints(formal, actual_union)
                {
                    common
                } else {
                    self.constraint_for_relation(formal, actual, relation_polarity)
                };
                return self.infer_from_constraint_set(when);
            }

            (formal @ Type::ProtocolInstance(_), actual @ Type::TypedDict(_)) => {
                let when = self.constraint_for_relation(formal, actual, relation_polarity);
                return self.infer_from_constraint_set(when);
            }

            // When the formal type is a protocol with a `__call__` method, infer the specialization
            // from matching the actual type's callable signature against the protocol's `__call__`
            // method signature.
            (Type::ProtocolInstance(formal_protocol), _) => {
                let Some(call_method) = formal_protocol.interface(db).call_method(db, self.env)
                else {
                    return Ok(());
                };
                let Some(actual_callables) = actual.try_upcast_to_callable(db, self.env) else {
                    return Ok(());
                };

                // The protocol interface exposes the callable signature already bound for
                // instance access.
                self.infer_from_callable_signature(
                    call_method,
                    actual_callables,
                    relation_polarity,
                )?;
            }

            (Type::Callable(formal_callable), _) => {
                let Some(actual_callables) = actual.try_upcast_to_callable(db, self.env) else {
                    return Ok(());
                };
                self.infer_from_callable_signature(
                    formal_callable,
                    actual_callables,
                    relation_polarity,
                )?;
            }

            // Expand type aliases in the actual type.
            //
            // This is placed at the end of the match block to avoid expanding the type alias
            // when it can be matched directly against a type variable in the formal type,
            // e.g., `reveal_type(alias)` should reveal the type alias, not its value type.
            (formal, Type::TypeAlias(alias)) => {
                return self.infer_map_impl(formal, alias.value_type(db), polarity, seen);
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

#[cfg(test)]
mod tests {
    use super::*;

    use ruff_python_ast::name::Name;

    use crate::db::tests::setup_db;

    #[test]
    fn generic_context_inferable_typevars_retain_instances_from_bounds() {
        let db = setup_db();
        let db = &db;
        let env = db.program_environment();
        let u = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("U"),
            TypeVarVariance::Invariant,
        );
        let t = BoundTypeVarInstance::synthetic(
            db,
            &env,
            Name::new_static("T"),
            TypeVarVariance::Invariant,
        )
        .map_bound_or_constraints(db, |_| {
            Some(TypeVarBoundOrConstraints::UpperBound(Type::TypeVar(u)))
        });
        let context = GenericContext::from_typevar_instances(db, &env, [t]);

        let inferable = context.inferable_typevars(db);
        assert_eq!(inferable.iter(db).collect::<Vec<_>>(), [t, u]);
        assert!(t.is_inferable(db, inferable));
        assert!(u.is_inferable(db, inferable));
    }
}
