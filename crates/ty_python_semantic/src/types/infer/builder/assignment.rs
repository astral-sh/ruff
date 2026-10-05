//! Assignment inference shared by immediate and suspended definition transactions.

use std::convert::Infallible;

use ruff_python_ast as ast;
use ty_module_resolver::ImportingFile;
use ty_python_core::definition::{AssignmentDefinitionKind, BindingsOwner, Definition};
use ty_python_core::expression::Expression;
use ty_python_core::unpack::Unpack;

use super::TypeInferenceBuilder;
use super::local;
use super::source_binding::{LegacySourceBindingEffects, SourceBindingEffects};
use super::source_expression::{LegacySourceExpressionEffects, SourceExpressionEffects};
use crate::types::diagnostic::report_invalid_type_checking_constant;
use crate::types::infer::{infer_expression_types, infer_unpack_types};
use crate::types::signatures::effects::legacy_inline;
use crate::types::{SpecialFormType, Type, TypeContext};

pub(in crate::types::infer::builder) mod statement;

pub(in crate::types::infer) mod sealed {
    pub(in crate::types::infer) trait Sealed {}
}

pub(in crate::types::infer) trait AssignmentDefinitionEffects<'db>:
    sealed::Sealed
{
    type Error;
    type BindingEffects: SourceBindingEffects<'db, Error = Self::Error>;
    type ExpressionEffects: SourceExpressionEffects<'db, Error = Self::Error>;

    fn binding_effects(&self) -> &Self::BindingEffects;
    fn expression_effects(&self) -> &Self::ExpressionEffects;

    async fn checkpoint(&self, builder: &TypeInferenceBuilder<'db, '_>) -> Result<(), Self::Error>;

    async fn unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        unpack: Unpack<'db>,
        target: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error>;

    async fn standalone_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: Expression<'db>,
        value: &ast::Expr,
        owner: BindingsOwner,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn local_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        value: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        target: &ast::Expr,
        call: &ast::ExprCall,
        definition: Definition<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn invalid_type_checking(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        target: &ast::Expr,
    ) -> Result<(), Self::Error>;

    async fn special_form(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        name: &str,
    ) -> Result<Option<SpecialFormType>, Self::Error>;
}

struct LegacyAssignmentDefinitionEffects;

impl sealed::Sealed for LegacyAssignmentDefinitionEffects {}

impl<'db> AssignmentDefinitionEffects<'db> for LegacyAssignmentDefinitionEffects {
    type Error = Infallible;
    type BindingEffects = LegacySourceBindingEffects;
    type ExpressionEffects = LegacySourceExpressionEffects;

    fn binding_effects(&self) -> &Self::BindingEffects {
        &LegacySourceBindingEffects
    }

    fn expression_effects(&self) -> &Self::ExpressionEffects {
        &LegacySourceExpressionEffects
    }

    async fn checkpoint(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        unpack: Unpack<'db>,
        target: &ast::Expr,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(infer_unpack_types(builder.db(), unpack).expression_type(target))
    }

    async fn standalone_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        expression: Expression<'db>,
        value: &ast::Expr,
        owner: BindingsOwner,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        let inference = infer_expression_types(builder.db(), expression, tcx);
        match owner {
            BindingsOwner::Definition => builder.extend_expression(inference),
            BindingsOwner::Statement => builder.extend_expression_without_bindings(inference),
        }
        Ok(inference.expression_type(value))
    }

    async fn local_expression(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        value: &ast::Expr,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.infer_expression(value, tcx))
    }

    async fn call(
        &self,
        builder: &mut TypeInferenceBuilder<'db, '_>,
        target: &ast::Expr,
        call: &ast::ExprCall,
        definition: Definition<'db>,
        tcx: TypeContext<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(builder.infer_assignment_call(target, call, definition, tcx))
    }

    async fn invalid_type_checking(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        target: &ast::Expr,
    ) -> Result<(), Self::Error> {
        report_invalid_type_checking_constant(&builder.context, target.into());
        Ok(())
    }

    async fn special_form(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        name: &str,
    ) -> Result<Option<SpecialFormType>, Self::Error> {
        let db = builder.db();
        let file = ImportingFile::File(
            builder.file(),
            builder.program_environment().resolver_environment(db),
        );
        Ok(SpecialFormType::try_from_file_and_name(db, file, name))
    }
}

impl<'db> TypeInferenceBuilder<'db, '_> {
    pub(super) fn infer_assignment_definition(
        &mut self,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
    ) {
        legacy_inline(self.infer_assignment_definition_with(
            &LegacyAssignmentDefinitionEffects,
            assignment,
            definition,
        ));
    }

    pub(in crate::types::infer) async fn infer_assignment_definition_with<
        E: AssignmentDefinitionEffects<'db>,
    >(
        &mut self,
        effects: &E,
        assignment: &AssignmentDefinitionKind<'db>,
        definition: Definition<'db>,
    ) -> Result<(), E::Error> {
        effects.checkpoint(self).await?;
        let target = assignment.target(self.module());
        let add = self
            .add_binding_with(effects.binding_effects(), target.into(), definition)
            .await?;
        let tcx = add.type_context();
        let value = assignment.value(self.module());

        let mut target_ty = match assignment.unpack() {
            Some(unpack) => {
                // The assignment statement owns unpacking diagnostics so that targets without a
                // name definition are still checked, and each diagnostic is reported only once.
                effects.unpack(self, unpack, target).await?
            }
            None => {
                // This could be an implicit type alias (OptionalList = list[T] | None). Use the definition
                // of `OptionalList` as the binding context while inferring the RHS (`list[T] | None`), in
                // order to bind `T` to `OptionalList`.
                let previous_typevar_binding_context =
                    self.typevar_binding_context.replace(definition);
                let value_result = async {
                    if let Some(expression) = self.index.try_expression(value) {
                        effects
                            .standalone_expression(self, expression, value, assignment.owner(), tcx)
                            .await
                    } else if let ast::Expr::Call(call) = value {
                        // If the RHS is not a standalone expression, this is a simple assignment
                        // (single target, no unpackings). That means it's a valid syntactic form
                        // for a legacy TypeVar creation; check for that.
                        let ty = effects.call(self, target, call, definition, tcx).await?;
                        effects
                            .expression_effects()
                            .store_expression(self, value, ty)
                            .await?;
                        Ok(ty)
                    } else {
                        effects.local_expression(self, value, tcx).await
                    }
                }
                .await;
                self.typevar_binding_context = previous_typevar_binding_context;
                let value_ty = value_result?;

                // `TYPE_CHECKING` is a special variable that should only be assigned `False`
                // at runtime, but is always considered `True` in type checking.
                // See mdtest/known_constants.md#user-defined-type_checking for details.
                if target.as_name_expr().map(|name| name.id.as_str()) == Some("TYPE_CHECKING") {
                    if !matches!(
                        value.as_boolean_literal_expr(),
                        Some(ast::ExprBooleanLiteral { value: false, .. })
                    ) {
                        effects.invalid_type_checking(self, target).await?;
                    }
                    Type::bool_literal(true)
                } else {
                    Self::stub_placeholder_binding_type(
                        effects.binding_effects().in_stub(&self.context).await?,
                        value,
                    )
                    .unwrap_or(value_ty)
                }
            }
        };

        if let Some(name) = target.as_name_expr()
            && let Some(special_form) = effects.special_form(self, &name.id).await?
        {
            target_ty = Type::SpecialForm(special_form);
        }

        effects
            .expression_effects()
            .store_expression(self, target, target_ty)
            .await?;
        add.insert_with(self, effects.binding_effects(), target_ty)
            .await?;
        Ok(())
    }

    fn infer_assignment_call(
        &mut self,
        target: &ast::Expr,
        call_expr: &ast::ExprCall,
        definition: Definition<'db>,
        tcx: TypeContext<'db>,
    ) -> Type<'db> {
        local::assignment_call(self, target, call_expr, definition, tcx)
    }
}
