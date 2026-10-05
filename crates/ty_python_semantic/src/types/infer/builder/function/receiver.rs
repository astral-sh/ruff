use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::ast_node_ref::AstNodeRef;
use ty_python_core::definition::{
    Definition, DefinitionKind, DefinitionNodeKey, FunctionDefinitionKind,
};

use super::MethodReceiverKind;
use crate::types::function::FunctionDecorators;
use crate::types::generics::typing_self;
use crate::types::infer::{
    TypeInferenceBuilder, function_known_decorator_flags, original_class_type,
};
use crate::types::{BoundTypeVarInstance, ClassLiteral, SubclassOfInner, SubclassOfType, Type};

pub(in crate::types::infer) trait MethodReceiverEffects<'db, 'ast> {
    type Error;

    async fn initialize_value<T>(&self, make: impl FnOnce() -> T) -> Result<T, Self::Error>;
    async fn definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        key: DefinitionNodeKey,
    ) -> Result<Definition<'db>, Self::Error>;
    async fn kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;
    async fn function_node(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: &FunctionDefinitionKind,
    ) -> Result<&'ast ast::StmtFunctionDef, Self::Error>;
    async fn parameter_index(
        &self,
        function: &ast::StmtFunctionDef,
        parameter: &ast::Parameter,
    ) -> Result<Option<usize>, Self::Error>;
    async fn in_class_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;
    async fn known_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<FunctionDecorators, Self::Error>;
    async fn classify(
        &self,
        function: &ast::StmtFunctionDef,
        decorators: FunctionDecorators,
    ) -> Result<Option<MethodReceiverKind>, Self::Error>;
    async fn original_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Option<ClassLiteral<'db>>, Self::Error>;
    async fn typing_self(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Self::Error>;
    async fn subclass(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

/// Classifies methods by their decorators and implicit class-receiver rules.
///
/// Free functions and ordinary static methods have no receiver; `__new__` receives the class.
///
/// ```python
/// class Example:
///     def instance(self): ...
///     @classmethod
///     def class_method(cls): ...
///     @staticmethod
///     def static_method(): ...
/// ```
async fn method_receiver_kind_with<'db, 'ast, E: MethodReceiverEffects<'db, 'ast>>(
    builder: &TypeInferenceBuilder<'db, 'ast>,
    definition: Definition<'db>,
    function: &ast::StmtFunctionDef,
    effects: &E,
) -> Result<Option<MethodReceiverKind>, E::Error> {
    if !effects.in_class_scope(builder, definition).await? {
        return effects.initialize_value(|| None).await;
    }
    let decorators = if effects
        .initialize_value(|| function.decorator_list.is_empty())
        .await?
    {
        effects.initialize_value(FunctionDecorators::empty).await?
    } else {
        effects.known_decorators(builder, definition).await?
    };
    effects.classify(function, decorators).await
}

/// Infers bound `Self` or `type[Self]` for the first parameter of an instance or class method.
///
/// Returns `None` for other parameters and methods without a receiver. The parameter name is
/// used to find its position; it need not be spelled `self` or `cls`.
pub(in crate::types::infer) async fn infer_method_receiver_with<
    'db,
    'ast,
    E: MethodReceiverEffects<'db, 'ast>,
>(
    builder: &TypeInferenceBuilder<'db, 'ast>,
    parameter: &ast::Parameter,
    function: &AstNodeRef<ast::StmtFunctionDef>,
    class: &AstNodeRef<ast::StmtClassDef>,
    effects: &E,
) -> Result<Option<Type<'db>>, E::Error> {
    let key = effects
        .initialize_value(|| {
            <DefinitionNodeKey as From<&AstNodeRef<ast::StmtFunctionDef>>>::from(function)
        })
        .await?;
    let method_definition = effects.definition(builder, key).await?;
    let kind = effects.kind(builder, method_definition).await?;
    let DefinitionKind::Function(function_definition) = kind else {
        return effects.initialize_value(|| None).await;
    };
    let function_node = effects.function_node(builder, function_definition).await?;
    if effects.parameter_index(function_node, parameter).await? != Some(0) {
        return effects.initialize_value(|| None).await;
    }
    let function_node = effects.function_node(builder, function_definition).await?;
    let Some(receiver_kind) =
        method_receiver_kind_with(builder, method_definition, function_node, effects).await?
    else {
        return effects.initialize_value(|| None).await;
    };
    let key = effects
        .initialize_value(|| {
            <DefinitionNodeKey as From<&AstNodeRef<ast::StmtClassDef>>>::from(class)
        })
        .await?;
    let class_definition = effects.definition(builder, key).await?;
    let Some(class_literal) = effects.original_class(builder, class_definition).await? else {
        return effects.initialize_value(|| None).await;
    };
    let Some(variable) = effects
        .typing_self(builder, method_definition, class_literal)
        .await?
    else {
        return effects.initialize_value(|| None).await;
    };
    let receiver = match receiver_kind {
        MethodReceiverKind::Class => effects.subclass(builder, variable).await?,
        MethodReceiverKind::Instance => {
            effects.initialize_value(|| Type::TypeVar(variable)).await?
        }
    };
    effects.initialize_value(|| Some(receiver)).await
}

pub(super) struct OrdinaryMethodReceiverEffects;

impl<'db, 'ast> MethodReceiverEffects<'db, 'ast> for OrdinaryMethodReceiverEffects {
    type Error = Infallible;

    async fn initialize_value<T>(&self, make: impl FnOnce() -> T) -> Result<T, Infallible> {
        Ok(make())
    }

    async fn definition(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        key: DefinitionNodeKey,
    ) -> Result<Definition<'db>, Infallible> {
        Ok(builder.index.expect_single_definition(key))
    }

    async fn kind(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Infallible> {
        Ok(definition.kind(builder.db()))
    }

    async fn function_node(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        function: &FunctionDefinitionKind,
    ) -> Result<&'ast ast::StmtFunctionDef, Infallible> {
        Ok(function.node(builder.module()))
    }

    async fn parameter_index(
        &self,
        function: &ast::StmtFunctionDef,
        parameter: &ast::Parameter,
    ) -> Result<Option<usize>, Infallible> {
        Ok(function.parameters.index(parameter.name()))
    }

    async fn in_class_scope(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(definition
            .scope(builder.db())
            .scope(builder.db())
            .kind()
            .is_class())
    }

    async fn known_decorators(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<FunctionDecorators, Infallible> {
        Ok(function_known_decorator_flags(builder.db(), definition))
    }

    async fn classify(
        &self,
        function: &ast::StmtFunctionDef,
        decorators: FunctionDecorators,
    ) -> Result<Option<MethodReceiverKind>, Infallible> {
        Ok(MethodReceiverKind::from_decorators(function, decorators))
    }

    async fn original_class(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
    ) -> Result<Option<ClassLiteral<'db>>, Infallible> {
        Ok(original_class_type(builder.db(), definition))
    }

    async fn typing_self(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        class: ClassLiteral<'db>,
    ) -> Result<Option<BoundTypeVarInstance<'db>>, Infallible> {
        Ok(typing_self(
            builder.db(),
            builder.scope(),
            Some(definition),
            class,
        ))
    }

    async fn subclass(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        variable: BoundTypeVarInstance<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(SubclassOfType::from(
            builder.db(),
            builder.program_environment(),
            SubclassOfInner::TypeVar(variable),
        ))
    }
}
