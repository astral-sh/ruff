//! Attribute inference borrows the expression owner and names unavailable semantic descendants.

use ruff_python_ast::{self as ast, ExprContext};
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::FileScopeId;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::{LookupError, LookupResult, PlaceAndQualifiers};
use crate::types::generic_attribute::GenericAttributeEffects;
use crate::types::infer::builder::applicable_constraints::{
    ApplicableConstraintsFacts, narrow_expr_with_applicable_constraints_with,
};
use crate::types::infer::builder::attribute::{
    AttributeEffects, AttributeFacts, AttributeLoadResult, AttributeOperation,
    infer_attribute_load_impl_with, infer_attribute_load_with,
    validate_generic_class_attribute_access_with,
};
use crate::types::infer::builder::source_expression::SourceExpressionEffects;
use crate::types::infer::builder::{TypeInferenceBuilder, local};
use crate::types::member_lookup::general::{
    GeneralMemberFacts, GeneralMemberName, member_lookup_entry_with,
};
use crate::types::typevar::TypeVarInstance;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, GenericAlias, MemberLookupError, MemberLookupPolicy,
    MemberLookupResult, PropertyDeprecations, ResolvedMember, Type, TypeAndQualifiers, TypeContext,
    UnionType,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GenericAttributeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn union(&self, _union: UnionType<'db>, _name: &str) -> RunResult<bool> {
        self.unavailable(SourceOperation::Attribute(
            AttributeOperation::GenericAccess,
        ))
        .await
    }

    async fn class(&self, _class: ClassLiteral<'db>, _name: &str) -> RunResult<bool> {
        self.unavailable(SourceOperation::Attribute(
            AttributeOperation::GenericAccess,
        ))
        .await
    }

    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<ClassLiteral<'db>> {
        self.local(2, 0, || alias.origin(self.db()).into()).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> AttributeEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        standalone: bool,
    ) -> RunResult<Type<'db>> {
        if standalone {
            local::source::maybe_standalone_expression(
                builder,
                expression,
                TypeContext::default(),
                self,
            )
            .await
        } else {
            local::source::expression(builder, expression, TypeContext::default(), self).await
        }
    }

    async fn load(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
    ) -> RunResult<AttributeLoadResult<'db>> {
        infer_attribute_load_with(builder, attribute, AttributeFacts, self).await
    }

    async fn load_on_receiver(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> RunResult<AttributeLoadResult<'db>> {
        infer_attribute_load_impl_with(builder, attribute, receiver, AttributeFacts, self).await
    }

    async fn is_paramspec(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.local(3, 0, || typevar.is_paramspec(builder.db()))
            .await
    }

    async fn bind_paramspec(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.unavailable(SourceOperation::Attribute(
            AttributeOperation::ParamSpecBinding,
        ))
        .await
    }

    async fn empty_constraints(&self) -> RunResult<Vec<(FileScopeId, ConstraintKey)>> {
        self.local(1, 0, Vec::new).await
    }

    async fn place_expression(
        &self,
        attribute: &ast::ExprAttribute,
    ) -> RunResult<Option<PlaceExpr>> {
        self.construct_place(ast::ExprRef::Attribute(attribute))
            .await
    }

    async fn assigned_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        place: PlaceExpr,
    ) -> RunResult<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>)> {
        builder
            .infer_place_load_with(self, place, ast::ExprRef::Attribute(attribute))
            .await
    }

    async fn member_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.environment_program(builder.program_environment())
            .await?;
        member_lookup_entry_with(
            receiver,
            GeneralMemberName::Shared(&attribute.attr.id),
            MemberLookupPolicy::default(),
            None,
            GeneralMemberFacts,
            self,
        )
        .await
    }

    async fn recover_member(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _attribute: &ast::ExprAttribute,
        _receiver: Type<'db>,
        _assigned: Option<Type<'db>>,
        _error: MemberLookupError<'db>,
    ) -> RunResult<ResolvedMember<'db>> {
        self.unavailable(SourceOperation::Attribute(
            AttributeOperation::MemberLookupDiagnostic,
        ))
        .await
    }

    async fn member_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        member: ResolvedMember<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.local(2, 0, || member.member(builder.db())).await
    }

    async fn narrow(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> RunResult<Type<'db>> {
        narrow_expr_with_applicable_constraints_with(
            builder,
            ast::ExprRef::Attribute(attribute),
            ty,
            constraints,
            ApplicableConstraintsFacts,
            self,
        )
        .await
    }

    async fn validate_generic_access(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> RunResult<()> {
        validate_generic_class_attribute_access_with(builder, attribute, receiver, true, self)
            .await?;
        Ok(())
    }

    async fn has_generic_instance_attribute(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        receiver: Type<'db>,
    ) -> RunResult<bool> {
        crate::types::generic_attribute::has_generic_instance_attribute_with(
            receiver,
            &attribute.attr.id,
            self,
        )
        .await
    }

    async fn report_generic_access(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _attribute: &ast::ExprAttribute,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Attribute(
            AttributeOperation::GenericAccess,
        ))
        .await
    }

    async fn place_lookup(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        place: PlaceAndQualifiers<'db>,
    ) -> RunResult<LookupResult<'db>> {
        SourceExpressionEffects::place_lookup(self, builder, place).await
    }

    async fn recover_lookup(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _attribute: &ast::ExprAttribute,
        _receiver: Type<'db>,
        error: LookupError<'db>,
    ) -> RunResult<TypeAndQualifiers<'db>> {
        let operation = match error {
            LookupError::Undefined(_) => AttributeOperation::UndefinedDiagnostic,
            LookupError::PossiblyUndefined(_) => AttributeOperation::PossiblyUndefinedDiagnostic,
        };
        self.unavailable(SourceOperation::Attribute(operation))
            .await
    }

    async fn check_deprecated(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
        ty: Type<'db>,
    ) -> RunResult<()> {
        builder
            .check_deprecated_with(self, &attribute.attr, ty)
            .await
    }

    async fn deprecated_properties(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        member: ResolvedMember<'db>,
    ) -> RunResult<Option<PropertyDeprecations<'db>>> {
        self.local(2, 0, || member.deprecated_properties(builder.db()))
            .await
    }

    async fn check_deprecated_property(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _attribute: &ast::ExprAttribute,
        _properties: PropertyDeprecations<'db>,
        _access: ExprContext,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Attribute(
            AttributeOperation::PropertyDeprecation,
        ))
        .await
    }

    async fn stored_receiver(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        attribute: &ast::ExprAttribute,
    ) -> RunResult<Type<'db>> {
        let work = Self::checked(
            builder
                .expressions
                .capacity()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, 0, || builder.expression_type(&attribute.value))
            .await
    }

    async fn validate_deletion(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _attribute: &ast::ExprAttribute,
        _receiver: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Attribute(AttributeOperation::Deletion))
            .await
    }
}
