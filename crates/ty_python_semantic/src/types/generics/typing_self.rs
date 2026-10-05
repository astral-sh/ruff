//! Construct and bind `typing.Self` using the containing class as its upper bound.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::definition::{Definition, DefinitionKind};
use ty_python_core::node_key::NodeKey;
use ty_python_core::scope::{FileScopeId, NodeWithScopeKey, ScopeId};
use ty_python_core::{SemanticIndex, semantic_index};

use super::bind_typevar;
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarDefaultEvaluation, TypeVarIdentity, TypeVarInstance,
};
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, Type, TypeVarBoundOrConstraints, TypeVarKind,
    TypeVarVariance,
};
use crate::{Db, ProgramEnvironment};

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousTypingSelfEffects)]
    pub(in crate::types) trait TypingSelfEffects<'db> {
        type Error;

        #[operation(local)]
        async fn environment(&self, scope: ScopeId<'db>) -> Result<ProgramEnvironment<'db>, Self::Error>;
        #[operation(source)]
        async fn semantic_index(&self, scope: ScopeId<'db>) -> Result<&'db SemanticIndex<'db>, Self::Error>;
        #[operation(local)]
        async fn static_name(&self, name: &'static str) -> Result<Name, Self::Error>;
        #[operation(local)]
        async fn intern_identity(&self, name: &Name, definition: Option<Definition<'db>>, kind: TypeVarKind) -> Result<TypeVarIdentity<'db>, Self::Error>;
        #[operation(child)]
        async fn identity_specialization(&self, class: ClassLiteral<'db>) -> Result<ClassType<'db>, Self::Error>;
        #[operation(child)]
        async fn instance(&self, env: &ProgramEnvironment<'db>, class: ClassType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(local)]
        async fn upper_bound(&self, ty: Type<'db>) -> Result<TypeVarBoundOrConstraintsEvaluation<'db>, Self::Error>;
        #[operation(local)]
        async fn variable_arguments(&self, bounds: TypeVarBoundOrConstraintsEvaluation<'db>, variance: TypeVarVariance) -> Result<(Option<TypeVarBoundOrConstraintsEvaluation<'db>>, Option<TypeVarVariance>), Self::Error>;
        #[operation(local)]
        async fn intern_variable(&self, identity: TypeVarIdentity<'db>, bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>, variance: Option<TypeVarVariance>, default: Option<TypeVarDefaultEvaluation<'db>>) -> Result<TypeVarInstance<'db>, Self::Error>;
        #[operation(source)]
        async fn function_node(&self, definition: Definition<'db>) -> Result<Option<NodeKey>, Self::Error>;
        #[operation(source)]
        async fn function_scope(&self, index: &SemanticIndex<'db>, function: NodeKey) -> Result<FileScopeId, Self::Error>;
        #[operation(source)]
        async fn scope_file_scope_id(&self, scope: ScopeId<'db>) -> Result<FileScopeId, Self::Error>;
        #[operation(child)]
        async fn bind(&self, index: &SemanticIndex<'db>, scope: FileScopeId, binding: Option<Definition<'db>>, variable: TypeVarInstance<'db>) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    }

    /// Constructs and binds `Self` with an instance of `class` as its upper bound.
    ///
    /// A function binding definition selects that function's body scope for lexical lookup;
    /// otherwise lookup starts from `scope_id`. Returns `None` if lexical lookup finds no binding
    /// and no explicit binding definition is supplied.
    #[synchronous(typing_self_sync)]
    #[capabilities(effects = TypingSelfEffects)]
    #[passive_values(TypeVarKind::TypingSelf, TypeVarVariance::Invariant)]
    pub(in crate::types) async fn typing_self_with<'db, E: TypingSelfEffects<'db>>(
        scope_id: ScopeId<'db>,
        typevar_binding_context: Option<Definition<'db>>,
        class: ClassLiteral<'db>,
        effects: &E,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, E::Error> {
        let env = effects.environment(scope_id).await?;
        let index = effects.semantic_index(scope_id).await?;

        let name = effects.static_name("Self").await?;
        let identity = effects.intern_identity(
            &name,
            // `Self` has a different upper bound dependent on the containing class,
            // so pointing to the definition of the symbol `typing.Self` itself is
            // not useful here. We could point to the class definition, but the full
            // range of the class definition is much larger than the full range of a
            // TypeVar would usually be, which leads to bugs like
            // https://github.com/astral-sh/ty/issues/2514. So we just pass `None`
            // for the definition field here.
            None,
            TypeVarKind::TypingSelf,
        ).await?;
        let class = effects.identity_specialization(class).await?;
        let instance = effects.instance(&env, class).await?;
        let bounds = effects.upper_bound(instance).await?;
        let (bounds, variance) = effects.variable_arguments(
            bounds,
            // According to the [spec], we can consider `Self`
            // equivalent to an invariant type variable
            // [spec]: https://typing.python.org/en/latest/spec/generics.html#self
            TypeVarVariance::Invariant,
        ).await?;
        let typevar = effects.intern_variable(
            identity,
            bounds,
            variance,
            None,
        ).await?;

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
        let containing_scope = if let Some(definition) = typevar_binding_context
            && let Some(function) = effects.function_node(definition).await?
        {
            effects.function_scope(index, function).await?
        } else {
            effects.scope_file_scope_id(scope_id).await?
        };

        effects.bind(index, containing_scope, typevar_binding_context, typevar).await
    }
}

pub(super) struct OrdinaryTypingSelfEffects<'db> {
    pub(super) db: &'db dyn Db,
}

impl<'db> SynchronousTypingSelfEffects<'db> for OrdinaryTypingSelfEffects<'db> {
    type Error = Infallible;

    fn environment(&self, scope: ScopeId<'db>) -> Result<ProgramEnvironment<'db>, Infallible> {
        Ok(ProgramEnvironment::from_scope(scope))
    }

    fn semantic_index(&self, scope: ScopeId<'db>) -> Result<&'db SemanticIndex<'db>, Infallible> {
        Ok(semantic_index(self.db, scope.program_file(self.db)))
    }

    fn static_name(&self, name: &'static str) -> Result<Name, Infallible> {
        Ok(Name::new_static(name))
    }

    fn intern_identity(
        &self,
        name: &Name,
        definition: Option<Definition<'db>>,
        kind: TypeVarKind,
    ) -> Result<TypeVarIdentity<'db>, Infallible> {
        Ok(TypeVarIdentity::new(
            self.db,
            name.clone(),
            definition,
            kind,
        ))
    }

    fn identity_specialization(
        &self,
        class: ClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Infallible> {
        Ok(class.identity_specialization(self.db))
    }

    fn instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(Type::instance(self.db, env, class))
    }

    fn upper_bound(
        &self,
        ty: Type<'db>,
    ) -> Result<TypeVarBoundOrConstraintsEvaluation<'db>, Infallible> {
        Ok(TypeVarBoundOrConstraints::UpperBound(ty).into())
    }

    fn variable_arguments(
        &self,
        bounds: TypeVarBoundOrConstraintsEvaluation<'db>,
        variance: TypeVarVariance,
    ) -> Result<
        (
            Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
            Option<TypeVarVariance>,
        ),
        Infallible,
    > {
        Ok((Some(bounds), Some(variance)))
    }

    fn intern_variable(
        &self,
        identity: TypeVarIdentity<'db>,
        bounds: Option<TypeVarBoundOrConstraintsEvaluation<'db>>,
        variance: Option<TypeVarVariance>,
        default: Option<TypeVarDefaultEvaluation<'db>>,
    ) -> Result<TypeVarInstance<'db>, Infallible> {
        Ok(TypeVarInstance::new(
            self.db, identity, bounds, variance, default,
        ))
    }

    fn function_node(&self, definition: Definition<'db>) -> Result<Option<NodeKey>, Infallible> {
        Ok(match definition.kind(self.db) {
            DefinitionKind::Function(function) => Some(function.node_key()),
            _ => None,
        })
    }

    fn function_scope(
        &self,
        index: &SemanticIndex<'db>,
        function: NodeKey,
    ) -> Result<FileScopeId, Infallible> {
        Ok(index.node_scope_by_key(NodeWithScopeKey::Function(function)))
    }

    fn scope_file_scope_id(&self, scope: ScopeId<'db>) -> Result<FileScopeId, Infallible> {
        Ok(scope.file_scope_id(self.db))
    }

    fn bind(
        &self,
        index: &SemanticIndex<'db>,
        scope: FileScopeId,
        binding: Option<Definition<'db>>,
        variable: TypeVarInstance<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(bind_typevar(self.db, index, scope, binding, variable))
    }
}
