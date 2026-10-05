use std::convert::Infallible;

use ruff_db::diagnostic::Span;
use ruff_db::files::{File, FileRange};
use ruff_db::parsed::{ParsedModuleRef, parsed_module};
use ruff_python_ast as ast;
use smallvec::SmallVec;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;

use super::LocalOverrideDefinition;
use crate::types::context::InferContext;
use crate::types::function::{FunctionDecorators, FunctionType, KnownFunction, OverloadLiteral};
use crate::types::list_members::Member;
use crate::types::{Type, definition_expression_type};

pub(in crate::types) struct OverrideDecoratorSource<'db> {
    pub(in crate::types) definition: Definition<'db>,
    pub(in crate::types) file: File,
    pub(in crate::types) module: ParsedModuleRef,
    pub(in crate::types) node: &'db AstNodeRef<ast::StmtFunctionDef>,
}

pub(in crate::types) trait LocalOverrideEffects<'db> {
    type Error;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error>;

    async fn step<T>(&self, action: impl FnOnce() -> T) -> Result<T, Self::Error> {
        self.local(Some(1), Some(size_of::<T>()), action).await
    }

    async fn local_functions(
        &self,
        member: &Member<'db>,
        scope: ScopeId<'db>,
    ) -> Result<SmallVec<[FunctionType<'db>; 1]>, Self::Error>;

    async fn in_stub(&self) -> Result<bool, Self::Error>;

    async fn overloads(
        &self,
        function: FunctionType<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error>;

    async fn first_overload(
        &self,
        function: FunctionType<'db>,
    ) -> Result<OverloadLiteral<'db>, Self::Error>;

    async fn focus_range(&self, overload: OverloadLiteral<'db>) -> Result<FileRange, Self::Error>;

    async fn overload_decorators(
        &self,
        overload: OverloadLiteral<'db>,
    ) -> Result<FunctionDecorators, Self::Error>;

    async fn decorator_source(
        &self,
        overload: OverloadLiteral<'db>,
    ) -> Result<OverrideDecoratorSource<'db>, Self::Error>;

    async fn next_decorator<'source>(
        &self,
        source: &'source OverrideDecoratorSource<'db>,
        cursor: &mut usize,
    ) -> Result<Option<&'source ast::Decorator>, Self::Error> {
        self.local(Some(3), Some(size_of::<Option<&ast::Decorator>>()), || {
            let decorator = source.node.node(&source.module).decorator_list.get(*cursor);
            *cursor += usize::from(decorator.is_some());
            decorator
        })
        .await
    }

    async fn decorator_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;

    async fn known_override(&self, ty: Type<'db>) -> Result<bool, Self::Error>;

    async fn append_metadata(
        &self,
        definitions: &mut SmallVec<[LocalOverrideDefinition; 1]>,
        definition: LocalOverrideDefinition,
    ) -> Result<(), Self::Error>;
}

pub(in crate::types) async fn function_has_decorator_with<'db, E: LocalOverrideEffects<'db>>(
    function: FunctionType<'db>,
    decorator: FunctionDecorators,
    effects: &E,
) -> Result<bool, E::Error> {
    let (overloads, implementation) = effects.overloads(function).await?;
    let mut definitions = effects
        .step(|| overloads.iter().copied().chain(implementation))
        .await?;
    while let Some(definition) = effects.step(|| definitions.next()).await? {
        let decorators = effects.overload_decorators(definition).await?;
        if effects.step(|| decorators.contains(decorator)).await? {
            return effects.step(|| true).await;
        }
    }
    effects.step(|| false).await
}

async fn overriding_definition_with<'db, E: LocalOverrideEffects<'db>>(
    function: FunctionType<'db>,
    in_stub: bool,
    effects: &E,
) -> Result<OverloadLiteral<'db>, E::Error> {
    let (_, implementation) = effects.overloads(function).await?;
    if !in_stub && let Some(implementation) = implementation {
        effects.step(|| implementation).await
    } else {
        effects.first_overload(function).await
    }
}

async fn override_decorator_span_with<'db, E: LocalOverrideEffects<'db>>(
    overload: OverloadLiteral<'db>,
    effects: &E,
) -> Result<Option<Span>, E::Error> {
    let source = effects.decorator_source(overload).await?;
    let mut cursor = effects.step(|| 0usize).await?;
    while let Some(decorator) = effects.next_decorator(&source, &mut cursor).await? {
        let ty = effects
            .decorator_type(source.definition, &decorator.expression)
            .await?;
        if effects.known_override(ty).await? {
            return effects
                .step(|| Some(Span::from(source.file).with_range(decorator.range)))
                .await;
        }
    }
    effects.step(|| None).await
}

async fn metadata_from_function_with<'db, E: LocalOverrideEffects<'db>>(
    function: FunctionType<'db>,
    in_stub: bool,
    effects: &E,
) -> Result<LocalOverrideDefinition, E::Error> {
    let focus_definition = overriding_definition_with(function, in_stub, effects).await?;
    let focus_range = effects.focus_range(focus_definition).await?;
    let any_definition_has_override_decorator =
        function_has_decorator_with(function, FunctionDecorators::OVERRIDE, effects).await?;
    let decorators = effects.overload_decorators(focus_definition).await?;
    let focus_definition_has_override_decorator = effects
        .step(|| decorators.contains(FunctionDecorators::OVERRIDE))
        .await?;
    let focus_override_decorator_span =
        override_decorator_span_with(focus_definition, effects).await?;
    effects
        .step(|| LocalOverrideDefinition {
            focus_range,
            any_definition_has_override_decorator,
            focus_definition_has_override_decorator,
            focus_override_decorator_span,
        })
        .await
}

async fn extract_local_override_definitions_with<'db, E: LocalOverrideEffects<'db>>(
    member: &Member<'db>,
    scope: ScopeId<'db>,
    effects: &E,
) -> Result<SmallVec<[LocalOverrideDefinition; 1]>, E::Error> {
    let functions = effects.local_functions(member, scope).await?;
    let mut functions = effects.step(|| functions.into_iter()).await?;
    let mut definitions = effects
        .local(
            Some(5),
            Some(size_of::<SmallVec<[LocalOverrideDefinition; 1]>>()),
            SmallVec::new,
        )
        .await?;
    while let Some(function) = effects.step(|| functions.next()).await? {
        let in_stub = effects.in_stub().await?;
        let definition = metadata_from_function_with(function, in_stub, effects).await?;
        effects
            .append_metadata(&mut definitions, definition)
            .await?;
    }
    effects.step(|| definitions).await
}

pub(in crate::types) async fn invalid_explicit_override_definition_with<
    'db,
    E: LocalOverrideEffects<'db>,
>(
    member: &Member<'db>,
    scope: ScopeId<'db>,
    effects: &E,
) -> Result<Option<LocalOverrideDefinition>, E::Error> {
    let definitions = extract_local_override_definitions_with(member, scope, effects).await?;
    let mut definitions = effects.step(|| definitions.into_iter()).await?;
    while let Some(definition) = effects.step(|| definitions.next()).await? {
        if effects
            .step(|| definition.any_definition_has_override_decorator)
            .await?
        {
            return effects.step(|| Some(definition)).await;
        }
    }
    effects.step(|| None).await
}

pub(in crate::types) async fn missing_override_definition_with<
    'db,
    E: LocalOverrideEffects<'db>,
>(
    member: &Member<'db>,
    scope: ScopeId<'db>,
    effects: &E,
) -> Result<Option<LocalOverrideDefinition>, E::Error> {
    let definitions = extract_local_override_definitions_with(member, scope, effects).await?;
    let mut definitions = effects.step(|| definitions.into_iter()).await?;
    while let Some(definition) = effects.step(|| definitions.next()).await? {
        if effects
            .step(|| !definition.focus_definition_has_override_decorator)
            .await?
        {
            return effects.step(|| Some(definition)).await;
        }
    }
    effects.step(|| None).await
}

pub(super) struct OrdinaryLocalOverrideEffects<'a, 'db, 'ast> {
    pub(super) context: &'a InferContext<'db, 'ast>,
}

impl<'db> LocalOverrideEffects<'db> for OrdinaryLocalOverrideEffects<'_, 'db, '_> {
    type Error = Infallible;

    async fn local<T>(
        &self,
        _work: Option<usize>,
        _bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> Result<T, Self::Error> {
        Ok(action())
    }

    async fn local_functions(
        &self,
        member: &Member<'db>,
        scope: ScopeId<'db>,
    ) -> Result<SmallVec<[FunctionType<'db>; 1]>, Self::Error> {
        Ok(member.local_functions(self.context.db(), scope))
    }

    async fn in_stub(&self) -> Result<bool, Self::Error> {
        Ok(self.context.in_stub())
    }

    async fn overloads(
        &self,
        function: FunctionType<'db>,
    ) -> Result<(&'db [OverloadLiteral<'db>], Option<OverloadLiteral<'db>>), Self::Error> {
        Ok(function.overloads_and_implementation(self.context.db()))
    }

    async fn first_overload(
        &self,
        function: FunctionType<'db>,
    ) -> Result<OverloadLiteral<'db>, Self::Error> {
        Ok(function.first_overload_or_implementation(self.context.db()))
    }

    async fn focus_range(&self, overload: OverloadLiteral<'db>) -> Result<FileRange, Self::Error> {
        Ok(overload.focus_range(self.context.db(), self.context.module()))
    }

    async fn overload_decorators(
        &self,
        overload: OverloadLiteral<'db>,
    ) -> Result<FunctionDecorators, Self::Error> {
        Ok(overload.decorators(self.context.db()))
    }

    async fn decorator_source(
        &self,
        overload: OverloadLiteral<'db>,
    ) -> Result<OverrideDecoratorSource<'db>, Self::Error> {
        let db = self.context.db();
        let definition = overload.definition(db);
        let file = definition.file(db);
        let module = parsed_module(db, definition.python_file(db)).load(db);
        debug_assert_eq!(
            file,
            overload.body_scope(db).file(db),
            "OverloadLiteral::node() must be called with the same file as the one where \
            the function is defined."
        );
        let node = overload.body_scope(db).node(db).expect_function();
        Ok(OverrideDecoratorSource {
            definition,
            file,
            module,
            node,
        })
    }

    async fn decorator_type(
        &self,
        definition: Definition<'db>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(definition_expression_type(
            self.context.db(),
            definition,
            expression,
        ))
    }

    async fn known_override(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        Ok(ty
            .as_function_literal()
            .is_some_and(|function| function.is_known(self.context.db(), KnownFunction::Override)))
    }

    async fn append_metadata(
        &self,
        definitions: &mut SmallVec<[LocalOverrideDefinition; 1]>,
        definition: LocalOverrideDefinition,
    ) -> Result<(), Self::Error> {
        definitions.push(definition);
        Ok(())
    }
}
