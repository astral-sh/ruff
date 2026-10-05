//! Deferred definition dispatch and class-header expression dependencies.

pub(in crate::types::infer) mod assignment;
pub(in crate::types::infer) mod type_parameter;

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_python_core::definition::{Definition, DefinitionKind};

use super::TypeInferenceBuilder;
use super::typevar::pep695::TypeParameterDefinitionNode;
use crate::Db;
use crate::types::context::InferContext;
use crate::types::infer::original_class_type;
use crate::types::{Type, TypeContext};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub(super) enum DeferredClassWork {
    Begin { keywords: usize },
    Base,
    ExtraItems,
}

pub(super) trait DeferredEffects<'db> {
    type Error;

    async fn checkpoint(&self) -> Result<(), Self::Error>;

    async fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Self::Error>;

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error>;

    async fn class_checkpoint(&self, work: DeferredClassWork) -> Result<(), Self::Error>;

    async fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;

    async fn is_typed_dict(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<bool, Self::Error>;

    async fn extra_items(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<(), Self::Error>;

    async fn assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        value: &'ast ast::Expr,
    ) -> Result<(), Self::Error>;

    async fn function_annotations<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> Result<(), Self::Error>;

    async fn type_parameter<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: TypeParameterDefinitionNode<'_>,
    ) -> Result<(), Self::Error>;
}

pub(super) struct LegacyDeferredEffects;

impl<'db> DeferredEffects<'db> for LegacyDeferredEffects {
    type Error = Infallible;

    async fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    async fn function_annotations<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        definition: Definition<'db>,
        function: &ast::StmtFunctionDef,
    ) -> Result<(), Infallible> {
        builder.infer_function_annotations(definition, function);
        Ok(())
    }

    async fn definition_kind(
        &self,
        db: &'db dyn Db,
        definition: Definition<'db>,
    ) -> Result<&'db DefinitionKind<'db>, Infallible> {
        Ok(definition.kind(db))
    }

    async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Infallible> {
        Ok(context.in_stub())
    }

    async fn class_checkpoint(&self, _work: DeferredClassWork) -> Result<(), Infallible> {
        Ok(())
    }

    async fn expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<Type<'db>, Infallible> {
        Ok(builder.infer_expression(expression, TypeContext::default()))
    }

    async fn is_typed_dict(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        definition: Definition<'db>,
    ) -> Result<bool, Infallible> {
        Ok(original_class_type(builder.db(), definition)
            .is_some_and(|class| class.is_typed_dict(builder.db())))
    }

    async fn extra_items(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> Result<(), Infallible> {
        builder.infer_extra_items_kwarg(expression);
        Ok(())
    }

    async fn type_parameter<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: TypeParameterDefinitionNode<'_>,
    ) -> Result<(), Infallible> {
        match node.node(builder.module()) {
            ast::TypeParamRef::TypeVar(node) => builder.infer_typevar_deferred(node),
            ast::TypeParamRef::ParamSpec(node) => builder.infer_paramspec_deferred(node),
            ast::TypeParamRef::TypeVarTuple(node) => builder.infer_typevartuple_deferred(node),
        }
        Ok(())
    }

    async fn assignment<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: &ast::Expr,
        value: &'ast ast::Expr,
    ) -> Result<(), Self::Error> {
        builder.infer_assignment_deferred(target, value);
        Ok(())
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) async fn infer_region_deferred_with<E: DeferredEffects<'db>>(
        &mut self,
        effects: &E,
        definition: Definition<'db>,
    ) -> Result<(), E::Error> {
        effects.checkpoint().await?;
        // N.B. We don't defer the types for an annotated assignment here because it is done in
        // the same definition query. It utilizes the deferred expression state instead.
        //
        // This is because for partially stringified annotations like `a: tuple[int, "ForwardRef"]`,
        // we need to defer the types of non-stringified expressions like `tuple` and `int` in the
        // definition query while the stringified expression `"ForwardRef"` would need to be deferred
        // to use end-of-scope semantics. This would require a custom and possibly complex
        // implementation to allow this "split" to happen.
        match effects.definition_kind(self.db(), definition).await? {
            DefinitionKind::Function(function) => {
                let function = function.node(self.module());
                effects
                    .function_annotations(self, definition, function)
                    .await?;
            }
            DefinitionKind::Class(class) => {
                self.infer_class_deferred_with(effects, definition, class.node(self.module()))
                    .await?;
            }
            DefinitionKind::TypeVar(typevar) => {
                effects
                    .type_parameter(self, TypeParameterDefinitionNode::TypeVar(typevar))
                    .await?;
            }
            DefinitionKind::ParamSpec(paramspec) => {
                effects
                    .type_parameter(self, TypeParameterDefinitionNode::ParamSpec(paramspec))
                    .await?;
            }
            DefinitionKind::TypeVarTuple(typevartuple) => {
                effects
                    .type_parameter(self, TypeParameterDefinitionNode::TypeVarTuple(typevartuple))
                    .await?;
            }
            DefinitionKind::Assignment(assignment) => {
                let target = assignment.target(self.module());
                let value = assignment.value(self.module());
                effects
                    .assignment(self, target, value)
                    .await?;
            }
            _ => {}
        }
        Ok(())
    }
}
