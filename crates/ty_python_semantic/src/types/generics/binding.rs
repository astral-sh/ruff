use std::convert::Infallible;

use ty_python_core::definition::{Definition, DefinitionKind, DefinitionNodeKey};
use ty_python_core::scope::{FileScopeId, NodeWithScopeKind, Scope};
use ty_python_core::{AncestorsIter, SemanticIndex};

use super::GenericContext;
use super::context_construction::ContextVariables;
use crate::Db;
use crate::types::infer::original_class_type;
use crate::types::signatures::ReturnCallableTypeVarScope;
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarIdentity, TypeVarInstance};
use crate::types::{BoundTypeVarInstance, TypeVarKind, binding_type, infer_definition_types};

#[derive(Clone, Copy)]
pub(in crate::types) enum BindingNode {
    Class(DefinitionNodeKey),
    Function(DefinitionNodeKey),
    FunctionTypeParameters(DefinitionNodeKey),
    TypeAlias(DefinitionNodeKey),
    Other,
}

pub(in crate::types) struct BindingFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypeVarBindingEffects)]
    pub(in crate::types) trait TypeVarBindingEffects<'db> {
        type Error;

        #[operation(source)]
        async fn kind(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarKind, Self::Error>;
        #[operation(source)]
        async fn typevar_definition(&self, typevar: TypeVarInstance<'db>) -> Result<Option<Definition<'db>>, Self::Error>;
        #[operation(source)]
        async fn definition_scope(&self, definition: Definition<'db>) -> Result<FileScopeId, Self::Error>;
        #[operation(source)]
        async fn definition_is_class(&self, definition: Definition<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn bound_identity(&self, bound: BoundTypeVarInstance<'db>) -> Result<BoundTypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn typevar_identity(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(source)]
        async fn bound_typevar(&self, bound: BoundTypeVarInstance<'db>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(local)]
        async fn bind(&self, typevar: TypeVarInstance<'db>, definition: Definition<'db>) -> Result<BoundTypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn ancestors<'index>(&self, index: &'index SemanticIndex<'db>, scope: FileScopeId) -> Result<AncestorsIter<'index>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_ancestor<'index>(&self, ancestors: &mut AncestorsIter<'index>) -> Result<Option<(FileScopeId, &'index Scope)>, Self::Error>;
        #[operation(source)]
        async fn scope<'index>(&self, index: &'index SemanticIndex<'db>, scope: FileScopeId) -> Result<&'index Scope, Self::Error>;
        #[operation(source)]
        async fn definition(&self, index: &SemanticIndex<'db>, key: DefinitionNodeKey) -> Result<Definition<'db>, Self::Error>;
        #[operation(child)]
        async fn class_context(&self, definition: Definition<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn function_context(&self, definition: Definition<'db>, mode: ReturnCallableTypeVarScope) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn alias_context(&self, definition: Definition<'db>) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn captured_paramspec(&self, definition: Definition<'db>, typevar: TypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(source)]
        async fn variables(&self, context: GenericContext<'db>) -> Result<&'db ContextVariables<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_variable(&self, variables: &ContextVariables<'db>, cursor: &mut usize) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn scope_context(&self, index: &SemanticIndex<'db>, node: BindingNode, mode: ReturnCallableTypeVarScope) -> Result<Option<GenericContext<'db>>, Self::Error>;
        #[operation(child)]
        async fn find_in_context(&self, context: GenericContext<'db>, typevar: TypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
        #[operation(child)]
        async fn visible(&self, bound: BoundTypeVarInstance<'db>, crossed_class_scope: bool) -> Result<bool, Self::Error>;
        #[operation(child)]
        async fn find_binding(&self, index: &SemanticIndex<'db>, scope: FileScopeId, typevar: TypeVarInstance<'db>, mode: ReturnCallableTypeVarScope) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    }

    #[finite_capability]
    impl BindingFacts {
        pub(in crate::types) fn node(&self, scope: &Scope) -> BindingNode {
            self.node_kind(scope.node())
        }

        pub(in crate::types) fn node_kind(&self, node: &NodeWithScopeKind) -> BindingNode {
            match node {
                NodeWithScopeKind::Class(node) => BindingNode::Class(node.into()),
                NodeWithScopeKind::Function(node) => BindingNode::Function(node.into()),
                NodeWithScopeKind::FunctionTypeParameters(node) => BindingNode::FunctionTypeParameters(node.into()),
                NodeWithScopeKind::TypeAlias(node) => BindingNode::TypeAlias(node.into()),
                _ => BindingNode::Other,
            }
        }

        fn is_class(&self, scope: &Scope) -> bool {
            scope.kind().is_class()
        }

        fn is_pep695(&self, kind: TypeVarKind) -> bool {
            kind.is_pep695()
        }

        fn is_paramspec(&self, kind: TypeVarKind) -> bool {
            kind.is_paramspec()
        }

        fn binding_definition<'db>(&self, identity: BoundTypeVarIdentity<'db>) -> Option<Definition<'db>> {
            identity.binding_context.definition()
        }

        fn same_identity<'db>(&self, left: TypeVarIdentity<'db>, right: TypeVarIdentity<'db>) -> bool {
            left == right
        }

        fn same_scope(&self, left: FileScopeId, right: FileScopeId) -> bool {
            left == right
        }

        fn different_definition<'db>(&self, binding: Option<Definition<'db>>, definition: Definition<'db>) -> bool {
            binding != Some(definition)
        }
    }

    /// Returns the generic context visible while checking the scope introduced by `node`.
    ///
    /// For functions, lexical mode retains type variables that are moved to a returned callable in the
    /// externally visible signature. Other scope kinds have identical lexical and public contexts.
    #[synchronous(scope_context_sync)]
    #[capabilities(effects = TypeVarBindingEffects)]
    #[passive_values()]
    pub(in crate::types) async fn scope_context_with<'db, E: TypeVarBindingEffects<'db>>(
        index: &SemanticIndex<'db>,
        node: BindingNode,
        mode: ReturnCallableTypeVarScope,
        effects: &E,
    ) -> Result<Option<GenericContext<'db>>, E::Error> {
        match node {
            BindingNode::Class(key) => {
                let definition = effects.definition(index, key).await?;
                effects.class_context(definition).await
            }
            BindingNode::Function(key) => {
                let definition = effects.definition(index, key).await?;
                effects.function_context(definition, mode).await
            }
            BindingNode::TypeAlias(key) => {
                let definition = effects.definition(index, key).await?;
                effects.alias_context(definition).await
            }
            _ => Ok(None),
        }
    }

    #[synchronous(find_in_context_sync)]
    #[capabilities(effects = TypeVarBindingEffects, facts = BindingFacts)]
    #[passive_values()]
    pub(in crate::types) async fn find_in_context_with<'db, E: TypeVarBindingEffects<'db>>(
        context: GenericContext<'db>,
        typevar: TypeVarInstance<'db>,
        facts: BindingFacts,
        effects: &E,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
        let variables = effects.variables(context).await?;
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(bound) = effects.next_variable(variables, &mut cursor).await? {
            let candidate = effects.bound_typevar(bound).await?;
            let candidate = effects.typevar_identity(candidate).await?;
            let requested = effects.typevar_identity(typevar).await?;
            if facts.same_identity(candidate, requested) {
                return Ok(Some(bound));
            }
        }
        Ok(None)
    }

    /// Returns whether a binding remains visible after crossing an inner class boundary.
    ///
    /// Class-owned bindings are hidden by the inner class; function-owned and synthetic bindings
    /// remain visible.
    #[synchronous(binding_visible_sync)]
    #[capabilities(effects = TypeVarBindingEffects, facts = BindingFacts)]
    #[passive_values()]
    pub(in crate::types) async fn binding_visible_with<'db, E: TypeVarBindingEffects<'db>>(
        bound: BoundTypeVarInstance<'db>,
        crossed_class_scope: bool,
        facts: BindingFacts,
        effects: &E,
    ) -> Result<bool, E::Error> {
        if !crossed_class_scope {
            return Ok(true);
        }
        let identity = effects.bound_identity(bound).await?;
        let Some(definition) = facts.binding_definition(identity) else {
            return Ok(true);
        };
        Ok(!effects.definition_is_class(definition).await?)
    }

    #[synchronous(find_typevar_binding_sync)]
    #[capabilities(effects = TypeVarBindingEffects, facts = BindingFacts)]
    #[passive_values()]
    pub(in crate::types) async fn find_typevar_binding_with<'db, E: TypeVarBindingEffects<'db>>(
        db: &'db dyn Db,
        index: &SemanticIndex<'db>,
        containing_scope: FileScopeId,
        typevar: TypeVarInstance<'db>,
        mode: ReturnCallableTypeVarScope,
        facts: BindingFacts,
        effects: &E,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
        let _ = db;
        // typing.Self is treated like a legacy typevar, but doesn't follow the same scoping rules. It
        // is always bound to the outermost method in the nearest enclosing class. The walk looks for a
        // (function, class) pair in the scope hierarchy. The caller (`typing_self`) is responsible for
        // ensuring that `containing_scope` starts from the function body scope rather than the scope
        // where the function is defined, so that the function itself appears in the ancestor chain.
        //
        // We also match `FunctionTypeParameters` as a valid inner scope because for generic methods
        // (e.g., `def foo[T](self) -> Self`), the type-params scope sits between the function body
        // and the class body in the ancestor chain.
        if matches!(effects.kind(typevar).await?, TypeVarKind::TypingSelf) {
            let mut ancestors = effects.ancestors(index, containing_scope).await?;
            #[passive_state]
            let mut inner = None;
            #[cursor_loop]
            while let Some(ancestor) = effects.next_ancestor(&mut ancestors).await? {
                let (_, outer) = ancestor;
                if facts.is_class(outer)
                    && let Some(BindingNode::Function(key) | BindingNode::FunctionTypeParameters(key)) = inner
                {
                    let definition = effects.definition(index, key).await?;
                    return Ok(Some(effects.bind(typevar, definition).await?));
                }
                inner = Some(facts.node(outer));
            }
            // Handle `Self` directly in class body annotations (not inside a method).
            let scope = effects.scope(index, containing_scope).await?;
            if let BindingNode::Class(key) = facts.node(scope) {
                let definition = effects.definition(index, key).await?;
                return Ok(Some(effects.bind(typevar, definition).await?));
            }
        }
        // Walk ancestor scopes, tracking whether we've crossed a class scope boundary.
        // Legacy class-scoped type variables are not visible from inner class scopes. PEP 695 type
        // parameters have lexical scopes that include nested classes, so they do not use this barrier.
        let kind = effects.kind(typevar).await?;
        let is_pep695 = facts.is_pep695(kind);
        #[passive_state]
        let mut crossed_class_scope = false;
        let mut ancestors = effects.ancestors(index, containing_scope).await?;
        #[cursor_loop]
        while let Some(ancestor) = effects.next_ancestor(&mut ancestors).await? {
            let (scope_id, scope) = ancestor;
            let is_class_scope = facts.is_class(scope);
            let node = facts.node(scope);
            if let BindingNode::FunctionTypeParameters(key) = node {
                // PEP 695 type parameters are defined in the function's type-parameter scope.
                // Check that directly instead of reconstructing the function's signature.
                if let Some(definition) = effects.typevar_definition(typevar).await?
                    && facts.same_scope(effects.definition_scope(definition).await?, scope_id)
                {
                    let definition = effects.definition(index, key).await?;
                    return Ok(Some(effects.bind(typevar, definition).await?));
                }
                continue;
            }
            let kind = effects.kind(typevar).await?;
            if facts.is_paramspec(kind)
                && let BindingNode::Function(key) = node
            {
                let definition = effects.definition(index, key).await?;
                if let Some(bound) = effects.captured_paramspec(definition, typevar).await? {
                    let identity = effects.bound_identity(bound).await?;
                    if facts.different_definition(facts.binding_definition(identity), definition)
                        && effects.visible(bound, crossed_class_scope).await?
                    {
                        return Ok(Some(bound));
                    }
                }
            }
            let context = effects.scope_context(index, node, mode).await?;
            // If we've already crossed a class boundary, skip class-scoped generic contexts.
            // This prevents inner classes from accessing legacy type variables bound by outer classes.
            // An enclosing function's context can also retain a type variable originally bound by its
            // enclosing class, so check the binding context as well as the ancestor node.
            if (!is_class_scope || !crossed_class_scope)
                && let Some(context) = context
                && let Some(bound) = effects.find_in_context(context, typevar).await?
                && effects.visible(bound, crossed_class_scope).await?
            {
                return Ok(Some(bound));
            }
            if is_class_scope && !is_pep695 {
                crossed_class_scope = true;
            }
        }
        Ok(None)
    }

    #[synchronous(bind_typevar_sync)]
    #[capabilities(effects = TypeVarBindingEffects)]
    #[passive_values(ReturnCallableTypeVarScope::Public)]
    pub(in crate::types) async fn bind_typevar_with<'db, E: TypeVarBindingEffects<'db>>(
        db: &'db dyn Db,
        index: &SemanticIndex<'db>,
        containing_scope: FileScopeId,
        binding_context: Option<Definition<'db>>,
        typevar: TypeVarInstance<'db>,
        effects: &E,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
        let _ = db;
        if let Some(bound) = effects.find_binding(index, containing_scope, typevar, ReturnCallableTypeVarScope::Public).await? {
            return Ok(Some(bound));
        }
        if let Some(definition) = binding_context {
            return Ok(Some(effects.bind(typevar, definition).await?));
        }
        Ok(None)
    }
}

pub(super) struct InlineBinding<'db>(pub(super) &'db dyn Db);

impl<'db> SynchronousTypeVarBindingEffects<'db> for InlineBinding<'db> {
    type Error = Infallible;

    fn kind(&self, typevar: TypeVarInstance<'db>) -> Result<TypeVarKind, Infallible> {
        Ok(typevar.kind(self.0))
    }
    fn typevar_definition(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<Definition<'db>>, Infallible> {
        Ok(typevar.definition(self.0))
    }
    fn definition_scope(&self, definition: Definition<'db>) -> Result<FileScopeId, Infallible> {
        Ok(definition.file_scope(self.0))
    }
    fn definition_is_class(&self, definition: Definition<'db>) -> Result<bool, Infallible> {
        Ok(matches!(definition.kind(self.0), DefinitionKind::Class(_)))
    }
    fn bound_identity(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<BoundTypeVarIdentity<'db>, Infallible> {
        Ok(bound.identity(self.0))
    }
    fn typevar_identity(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(typevar.identity(self.0))
    }
    fn bound_typevar(
        &self,
        bound: BoundTypeVarInstance<'db>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(bound.typevar(self.0))
    }
    fn bind(
        &self,
        typevar: TypeVarInstance<'db>,
        definition: Definition<'db>,
    ) -> Result<BoundTypeVarInstance<'db>, Infallible> {
        Ok(typevar.with_binding_context(self.0, definition))
    }
    fn ancestors<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> Result<AncestorsIter<'index>, Infallible> {
        Ok(index.ancestor_scopes(scope))
    }
    fn next_ancestor<'index>(
        &self,
        ancestors: &mut AncestorsIter<'index>,
    ) -> Result<Option<(FileScopeId, &'index Scope)>, Infallible> {
        Ok(ancestors.next())
    }
    fn scope<'index>(
        &self,
        index: &'index SemanticIndex<'db>,
        scope: FileScopeId,
    ) -> Result<&'index Scope, Infallible> {
        Ok(index.scope(scope))
    }
    fn definition(
        &self,
        index: &SemanticIndex<'db>,
        key: DefinitionNodeKey,
    ) -> Result<Definition<'db>, Infallible> {
        Ok(index.expect_single_definition(key))
    }
    fn class_context(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(original_class_type(self.0, definition).and_then(|class| class.generic_context(self.0)))
    }
    fn function_context(
        &self,
        definition: Definition<'db>,
        mode: ReturnCallableTypeVarScope,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(infer_definition_types(self.0, definition)
            .function_type(definition)
            .and_then(|function| match mode {
                ReturnCallableTypeVarScope::Public => {
                    function.last_definition_signature(self.0).generic_context
                }
                ReturnCallableTypeVarScope::Lexical => {
                    function
                        .last_definition_raw_signature(self.0, mode)
                        .generic_context
                }
            }))
    }
    fn alias_context(
        &self,
        definition: Definition<'db>,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        Ok(binding_type(self.0, definition)
            .as_type_alias()
            .and_then(|alias| alias.as_pep_695_type_alias())
            .and_then(|alias| alias.generic_context(self.0)))
    }
    fn captured_paramspec(
        &self,
        definition: Definition<'db>,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(infer_definition_types(self.0, definition)
            .function_type(definition)
            .and_then(|function| {
                function
                    .last_definition_raw_signature(self.0, ReturnCallableTypeVarScope::Lexical)
                    .paramspec_component_binding(self.0, typevar)
            }))
    }
    fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> Result<&'db ContextVariables<'db>, Infallible> {
        Ok(context.variables_inner(self.0))
    }
    fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        cursor: &mut usize,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        let next = GenericContext::variable_at_in(variables, *cursor);
        if next.is_some() {
            *cursor += 1;
        }
        Ok(next)
    }
    fn scope_context(
        &self,
        index: &SemanticIndex<'db>,
        node: BindingNode,
        mode: ReturnCallableTypeVarScope,
    ) -> Result<Option<GenericContext<'db>>, Infallible> {
        scope_context_sync(index, node, mode, self)
    }
    fn find_in_context(
        &self,
        context: GenericContext<'db>,
        typevar: TypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        find_in_context_sync(context, typevar, BindingFacts, self)
    }
    fn visible(
        &self,
        bound: BoundTypeVarInstance<'db>,
        crossed_class_scope: bool,
    ) -> Result<bool, Infallible> {
        binding_visible_sync(bound, crossed_class_scope, BindingFacts, self)
    }
    fn find_binding(
        &self,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        typevar: TypeVarInstance<'db>,
        mode: ReturnCallableTypeVarScope,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        find_typevar_binding_sync(self.0, index, scope, typevar, mode, BindingFacts, self)
    }
}
