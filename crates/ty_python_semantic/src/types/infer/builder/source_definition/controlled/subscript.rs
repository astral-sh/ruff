use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::SemanticIndex;
use ty_python_core::definition::Definition;
use ty_python_core::narrowing_constraints::ConstraintKey;
use ty_python_core::place::PlaceExpr;
use ty_python_core::scope::FileScopeId;

use super::storage::{ordered_merge, slots};
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::PlaceAndQualifiers;
use crate::types::generics::GenericContext;
use crate::types::generics::binding::bind_typevar_with;
use crate::types::infer::TypeExpressionFlags;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::applicable_constraints::{
    ApplicableConstraintsFacts, narrow_expr_with_applicable_constraints_with,
};
use crate::types::infer::builder::subscript::legacy_generic::{
    AstArguments, LegacyArguments, LegacyGenericContextError, LegacyGenericEffects,
    infer_legacy_generic_subscript_with, legacy_generic_class_context_with,
    unpacked_typevartuple_with,
};
use crate::types::infer::builder::subscript::{SubscriptEffects, SubscriptFacts};
use crate::types::instance::{NominalClassFacts, NominalVisitorKind, nominal_known_class_with};
use crate::types::subscript::SubscriptError;
use crate::types::tuple::TupleSpec;
use crate::types::type_expression_conversion::TypeExpressionConversionEffects;
use crate::types::typevar::TypeVarInstance;
use crate::types::{BoundTypeVarInstance, ClassLiteral, KnownClass, NominalInstanceType, Type};
use crate::{Db, FxOrderSet, ProgramEnvironment};

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> SubscriptEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn expected_keys(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::SubscriptExpectedKeys)
            .await
    }

    async fn store_expected(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _slice: &ast::Expr,
        _expected: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::SubscriptExpectedKeys)
            .await
    }

    async fn empty_constraints(&self) -> RunResult<Vec<(FileScopeId, ConstraintKey)>> {
        self.local(1, 0, Vec::new).await
    }

    async fn place_expression(
        &self,
        subscript: &ast::ExprSubscript,
    ) -> RunResult<Option<PlaceExpr>> {
        self.construct_place(ast::ExprRef::Subscript(subscript))
            .await
    }

    async fn assigned_place(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        place: PlaceExpr,
    ) -> RunResult<(PlaceAndQualifiers<'db>, Vec<(FileScopeId, ConstraintKey)>)> {
        builder
            .infer_place_load_with(self, place, ast::ExprRef::Subscript(subscript))
            .await
    }

    async fn implicit_alias(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
        _value_ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::SubscriptImplicitAlias)
            .await
    }

    async fn class_is_tuple(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        let known = TypeExpressionConversionEffects::class_known(self, class).await?;
        self.local(1, 0, || known == Some(KnownClass::Tuple)).await
    }

    async fn class_is_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> RunResult<bool> {
        let known = TypeExpressionConversionEffects::class_known(self, class).await?;
        self.local(1, 0, || known == Some(KnownClass::Type)).await
    }

    async fn class_generic_context(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        let class = self.local(1, 0, || class.as_static()).await?;
        match class {
            Some(class) => self.access.class_generic_context(class).await,
            None => Ok(None),
        }
    }

    async fn tuple_class_specialization(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationTupleClass)
            .await
    }

    async fn type_class_specialization(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationTypeClass)
            .await
    }

    async fn receiver_tail(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _subscript: &ast::ExprSubscript,
        _value_ty: Type<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::SubscriptReceiver).await
    }

    async fn expression_types(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        value_ty: Type<'db>,
        slice_ty: Type<'db>,
    ) -> RunResult<Result<Type<'db>, Type<'db>>> {
        match SubscriptFacts.legacy_origin(value_ty) {
            Some(origin) => {
                infer_legacy_generic_subscript_with(builder, subscript, slice_ty, origin, self)
                    .await
            }
            None => {
                self.unavailable(SourceOperation::SubscriptExpressionTypes)
                    .await
            }
        }
    }

    async fn narrow(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        subscript: &ast::ExprSubscript,
        ty: Type<'db>,
        constraints: &[(FileScopeId, ConstraintKey)],
    ) -> RunResult<Type<'db>> {
        narrow_expr_with_applicable_constraints_with(
            builder,
            ast::ExprRef::Subscript(subscript),
            ty,
            constraints,
            ApplicableConstraintsFacts,
            self,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LegacyGenericEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn exact_tuple(&self, ty: Type<'db>) -> RunResult<Option<&'db TupleSpec<'db>>> {
        let tuple = self
            .local(2, 0, || {
                match ty
                    .as_nominal_instance()
                    .map(|instance| instance.visitor_kind())
                {
                    Some(NominalVisitorKind::Tuple(tuple)) => Some(tuple),
                    _ => None,
                }
            })
            .await?;
        match tuple {
            Some(tuple) => Ok(Some(
                self.field(
                    tuple
                        .field_requests(self.access.endpoint().field_request_context())
                        .tuple(),
                )
                .await?,
            )),
            None => Ok(None),
        }
    }

    async fn next_type(
        &self,
        arguments: &mut LegacyArguments<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(6, 0, || arguments.next()).await
    }

    async fn new_validated(&self) -> RunResult<FxOrderSet<BoundTypeVarInstance<'db>>> {
        self.initialize_value(FxOrderSet::default).await
    }

    async fn insert(
        &self,
        variables: &mut FxOrderSet<BoundTypeVarInstance<'db>>,
        bound: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let quote =
            ordered_merge::<BoundTypeVarInstance<'db>>(variables.len(), variables.capacity(), 1)
                .ok_or(RunError::Contract(
                    "legacy generic variable storage quotation overflow",
                ))?;
        let work = Self::checked(
            quote
                .work
                .checked_add(Self::checked(slots(variables.capacity()))?)
                .and_then(|work| work.checked_add(quote.bytes.checked_mul(2)?))
                .and_then(|work| work.checked_add(4)),
        )?;
        // The insertion also pays for disposing of the retained set if a later child refuses.
        self.local(work, quote.bytes, || variables.insert(bound))
            .await
    }

    async fn bind(
        &self,
        db: &'db dyn Db,
        index: &'db SemanticIndex<'db>,
        scope: FileScopeId,
        context: Option<Definition<'db>>,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        bind_typevar_with(db, index, scope, context, typevar, self).await
    }

    async fn bound_is_typevartuple(&self, bound: BoundTypeVarInstance<'db>) -> RunResult<bool> {
        let fields = self.access.endpoint().field_request_context();
        let identity = self.field(bound.identity_request(fields)).await?;
        let kind = self
            .field(identity.identity.field_requests(fields).kind())
            .await?;
        self.local(1, 0, || kind.is_typevartuple()).await
    }

    async fn typevar_name(&self, typevar: TypeVarInstance<'db>) -> RunResult<&'db str> {
        let fields = self.access.endpoint().field_request_context();
        let identity = self
            .field(typevar.field_requests(fields).identity())
            .await?;
        let name = self.field(identity.field_requests(fields).name()).await?;
        self.local(1, 0, || name.as_str()).await
    }

    async fn bound_name(&self, bound: BoundTypeVarInstance<'db>) -> RunResult<&'db str> {
        let typevar = self
            .field(
                bound
                    .field_requests(self.access.endpoint().field_request_context())
                    .typevar(),
            )
            .await?;
        LegacyGenericEffects::typevar_name(self, typevar).await
    }

    async fn nominal_is_typevartuple(&self, nominal: NominalInstanceType<'db>) -> RunResult<bool> {
        let known = nominal_known_class_with(nominal, NominalClassFacts, self).await?;
        self.local(1, 0, || {
            matches!(
                known,
                Some(KnownClass::TypeVarTuple | KnownClass::ExtensionsTypeVarTuple)
            )
        })
        .await
    }

    async fn contains_typevartuple(
        &self,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::SubscriptLegacyArgumentTraversal)
            .await
    }

    async fn context(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variables: FxOrderSet<BoundTypeVarInstance<'db>>,
    ) -> RunResult<GenericContext<'db>> {
        self.context_from_legacy_variables(env, variables).await
    }

    async fn next_ast<'expr>(
        &self,
        arguments: &mut AstArguments<'expr>,
    ) -> RunResult<Option<&'expr ast::Expr>> {
        self.local(2, 0, || arguments.next()).await
    }

    async fn invalid_unpack(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        argument: &ast::Expr,
    ) -> RunResult<bool> {
        let work = Self::checked(
            builder
                .type_expression_flags
                .capacity()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, 0, || {
            builder
                .type_expression_flags(argument)
                .contains(TypeExpressionFlags::INVALID_UNPACK)
        })
        .await
    }

    async fn expression_type(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        expression: &ast::Expr,
    ) -> RunResult<Type<'db>> {
        let work = Self::checked(
            builder
                .expressions
                .capacity()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, 0, || builder.expression_type(expression))
            .await
    }

    async fn unpacked(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        argument: &ast::Expr,
    ) -> RunResult<bool> {
        unpacked_typevartuple_with(builder, argument, self).await
    }

    async fn report(
        &self,
        _builder: &TypeInferenceBuilder<'db, '_>,
        _subscript: &ast::ExprSubscript,
        _error: SubscriptError<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::SubscriptDiagnostic).await
    }

    async fn validate(
        &self,
        builder: &TypeInferenceBuilder<'db, '_>,
        slice_ty: Type<'db>,
    ) -> RunResult<Result<GenericContext<'db>, LegacyGenericContextError<'db>>> {
        self.environment_program(builder.program_environment())
            .await?;
        let scope = self
            .field(
                builder
                    .scope()
                    .read_fields(self.access.endpoint().field_request_context())
                    .file_scope_id(),
            )
            .await?;
        legacy_generic_class_context_with(
            builder.db(),
            builder.program_environment(),
            builder.index,
            scope,
            builder.typevar_binding_context,
            slice_ty,
            self,
        )
        .await
    }
}
