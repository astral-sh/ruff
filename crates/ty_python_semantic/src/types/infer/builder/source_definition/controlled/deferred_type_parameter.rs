//! Controlled deferred parameter inference keeps temporary flags inside an owned builder checkpoint.

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::NodeWithScopeKind;

use super::lint_diagnostic_cost::{
    BufferQuotePreparation, buffer_quote_preparation, empty_vec_quote, prepared_vec_push_quote,
    vec_into_boxed_slice_quote, vec_reserve_exact_quote,
};
use super::storage::StorageQuote;
use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::analysis::DeferredInferenceOperation;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::infer::builder::deferred::assignment::{DeferredAssignmentEffects, validate_typevar_default_with};
use crate::types::infer::builder::deferred::type_parameter::{
    DefaultContext, DefaultFlagState, DeferredTypeParameterEffects, DeferredTypeParameterFacts,
    ParameterWork, infer_paramspec_default_with, infer_type_parameter_deferred_with,
    infer_typevartuple_default_with,
};
use crate::types::infer::builder::local::{BuilderId, BuilderStore};
use crate::types::infer::builder::scope::ScopeEffects;
use crate::types::infer::builder::source_expression::SourceExpressionEffects;
use crate::types::infer::builder::type_expression::TypeExpressionMode;
use crate::types::infer::builder::typevar::pep695::TypeParameterDefinitionNode;
use crate::types::infer::builder::{BoundOrConstraintsNodes, DeferredExpressionState, TypeInferenceBuilder, local};
use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::local_transfer::generated_field_quote;
use crate::types::tuple::{FixedLengthTuple, TupleSpec};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarConstraints, TypeVarIdentity};
use crate::types::{BoundTypeVarInstance, KnownInstanceType, Type, TypeVarBoundOrConstraints, TypeVarKind};

// Public and internal bitflags methods both participate in these source-event bounds.
// These are the same contains/set chains used by annotation_expression::starred_cost.
const CALL_1: usize = 3 + 2;
const CALL_2: usize = 6 + 2;
const CALL_3: usize = 9 + 2;
const FLAG_CONTAINS: usize = 2 * CALL_2 + 6;
const FLAG_SET: usize = 2 * CALL_3 + 2 * CALL_2 + 13;
const ENTER_DEFAULT: usize = CALL_2 + CALL_3 + FLAG_CONTAINS + FLAG_SET + 22;
const RESTORE_DEFAULT: usize = 2 * CALL_2 + FLAG_SET + 12;
const TEST_UNPACK: usize = FLAG_CONTAINS + 4;
const TEST_KIND: usize = 2 * CALL_1 + 8;
const COMPLETE_OWNER: usize = CALL_1 + CALL_1 + 17 + 8;

/// Reservation and mutation costs for appending one type to an owned inference buffer.
#[derive(Debug)]
struct TypeAppend {
    quote: StorageQuote,
    reserve: usize,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Saves the builder's temporary inference state until all deferred children have drained.
    async fn deferred_parameter_store<'builder, 'ast>(
        &self,
        builder: &'builder mut TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<BuilderStore<'builder, 'db, 'ast>> {
        let quote = const {
            match empty_vec_quote::<TypeInferenceBuilder<'db, 'ast>>() {
                Ok((work, bytes)) => match (
                    work.checked_add(61 + 17 + CALL_1 + 3),
                    bytes.checked_add(size_of::<[BuilderStore<'_, 'db, 'ast>; 12]>()),
                ) {
                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                    _ => Err(RunError::Contract("deferred parameter owner quotation overflow")),
                },
                Err(error) => Err(error),
            }
        };
        self.local_quoted_with_fixed_transfers(quote, || BuilderStore::new(builder)).await
    }

    /// Runs shared declaration inference inside a checkpoint that restores temporary state on refusal.
    /// The enclosing deferred-definition provider owns and discards the unpublished inference maps.
    pub(super) async fn infer_deferred_type_parameter_source<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        node: TypeParameterDefinitionNode<'_>,
    ) -> RunResult<()> {
        let mut store = self.deferred_parameter_store(builder).await?;
        #[cfg(test)]
        store.observe_deferred_parameter(local::DeferredParameterTransactionKind::Declaration);
        let node = self.local_with_fixed_transfers(
            21,
            size_of::<[(TypeParameterDefinitionNode<'_>, ast::TypeParamRef<'_>); 3]>(),
            || node.node(store.get_mut(BuilderId::ROOT).module()),
        ).await?;
        self.boxed_future_with_fixed_transfers(
            Ok((23, size_of::<[(&mut TypeInferenceBuilder<'db, 'ast>, ast::TypeParamRef<'ast>, DeferredTypeParameterFacts, &Self); 2]>())),
            || infer_type_parameter_deferred_with(store.get_mut(BuilderId::ROOT), node, DeferredTypeParameterFacts, self),
        ).await?.await?;
        self.local_with_fixed_transfers(COMPLETE_OWNER, COMPLETE_OWNER * size_of::<(&Vec<TypeInferenceBuilder<'db, 'ast>>, usize, bool)>(), || {
            #[cfg(test)]
            store.observe_deferred_parameter_completion();
            store.complete();
        }).await
    }

    /// Infers a ParamSpec default and restores its incoming flags before completing the checkpoint.
    pub(super) async fn infer_paramspec_default_source<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        name: Option<&str>,
    ) -> RunResult<()> {
        let mut store = self.deferred_parameter_store(builder).await?;
        #[cfg(test)]
        store.observe_deferred_parameter(local::DeferredParameterTransactionKind::ParamSpec);
        self.boxed_future_with_fixed_transfers(
            Ok((23, size_of::<[(&mut TypeInferenceBuilder<'db, 'ast>, &ast::Expr, Option<&str>, &Self); 2]>())),
            || infer_paramspec_default_with(store.get_mut(BuilderId::ROOT), expression, name, self),
        ).await?.await?;
        self.local_with_fixed_transfers(COMPLETE_OWNER, COMPLETE_OWNER * size_of::<(&Vec<TypeInferenceBuilder<'db, 'ast>>, usize, bool)>(), || {
            #[cfg(test)]
            store.observe_deferred_parameter_completion();
            store.complete();
        }).await
    }

    /// Infers a TypeVarTuple default with abort restoration through the existing builder owner.
    pub(super) async fn infer_typevartuple_default_source<'ast>(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
        name: Option<&str>,
    ) -> RunResult<()> {
        let mut store = self.deferred_parameter_store(builder).await?;
        #[cfg(test)]
        store.observe_deferred_parameter(local::DeferredParameterTransactionKind::TypeVarTuple);
        self.boxed_future_with_fixed_transfers(
            Ok((23, size_of::<[(&mut TypeInferenceBuilder<'db, 'ast>, &ast::Expr, Option<&str>, &Self); 2]>())),
            || infer_typevartuple_default_with(store.get_mut(BuilderId::ROOT), expression, name, self),
        ).await?.await?;
        self.local_with_fixed_transfers(COMPLETE_OWNER, COMPLETE_OWNER * size_of::<(&Vec<TypeInferenceBuilder<'db, 'ast>>, usize, bool)>(), || {
            #[cfg(test)]
            store.observe_deferred_parameter_completion();
            store.complete();
        }).await
    }

    /// Converts an owned type buffer to a boxed slice after paying for any shrink and retirement
    /// of the returned boxed slice's allocation.
    async fn boxed_parameter_types(&self, types: Vec<Type<'db>>) -> RunResult<Box<[Type<'db>]>> {
        let preparation = const {
            match buffer_quote_preparation(BufferQuotePreparation::VecIntoBoxedSlice) {
                Ok((work, bytes)) => match (work.checked_add(17 + 42 + 2 * 5 + 8 + 8), bytes.checked_add(size_of::<[usize; 85]>())) {
                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                    _ => Err(RunError::Contract("boxed parameter preparation overflow")),
                },
                Err(error) => Err(error),
            }
        };
        let quote = self.local_quoted_with_fixed_transfers(preparation, || {
            vec_into_boxed_slice_quote::<Type<'db>>(types.len(), types.capacity())
        }).await?;
        self.local_quoted_with_fixed_transfers(quote, || types.into_boxed_slice()).await
    }

    /// Reads a bound variable's identity and kind through the same native fields as ordinary inference.
    async fn deferred_bound_kind(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        let identity = self.boxed_future_with_fixed_transfers(
            Ok((8, size_of::<[(BoundTypeVarInstance<'db>, &Self); 2]>())),
            || TypeVarBindingEffects::bound_identity(self, variable),
        ).await?.await?;
        let identity = self.local_with_fixed_transfers(2, size_of::<BoundTypeVarIdentity<'db>>(), || identity.identity).await?;
        let quote = generated_field_quote(
            |identity: TypeVarIdentity<'db>, context| identity.field_requests(context),
            |identity: TypeVarIdentity<'db>, context| identity.field_requests(context).kind(),
        );
        let endpoint = self.access.endpoint();
        let read = self.boxed_future_with_fixed_transfers(quote, || {
            let request = identity.field_requests(endpoint.field_request_context()).kind();
            endpoint.read_field(request, &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> DeferredTypeParameterEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, stage: ParameterWork) -> RunResult<()> {
        let (work, bytes) = self.local_with_fixed_transfers(5, size_of::<(ParameterWork, usize, usize)>(), || match stage {
            ParameterWork::Declaration => (3 * 50 + 2 * 16 + 64 + 48 + 20, size_of::<[(ast::TypeParamRef<'ast>, Option<&ast::Expr>, Option<TypeVarBoundOrConstraints<'db>>, Option<BoundOrConstraintsNodes<'ast>>, Type<'db>, DeferredExpressionState); 12]>()),
            ParameterWork::ParamSpecDefault => (3 * 20 + 2 * 7 + 21 + 33, size_of::<[(&ast::Expr, Option<&str>, DefaultFlagState, Type<'db>, bool); 12]>()),
            ParameterWork::TypeVarTupleDefault => (3 * 23 + 2 * 7 + 21 + 24, size_of::<[(&ast::Expr, Option<&str>, DefaultFlagState, Type<'db>, bool); 8]>()),
            ParameterWork::Element => (3 * 12 + 2 * 4 + 12 + 12, size_of::<[(&ast::Expr, Type<'db>, bool, usize); 12]>()),
        }).await?;
        self.local_with_fixed_transfers(work, bytes, || ()).await
    }

    async fn replace_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, state: DeferredExpressionState) -> RunResult<DeferredExpressionState> {
        self.local_with_fixed_transfers(CALL_2 + CALL_1 + 8, size_of::<[DeferredExpressionState; 8]>(), || builder.replace_deferred_state(state)).await
    }

    async fn restore_deferred(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, state: DeferredExpressionState) -> RunResult<()> {
        self.local_with_fixed_transfers(4, size_of::<[DeferredExpressionState; 2]>(), || builder.deferred_state = state).await
    }

    async fn enter_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, context: DefaultContext) -> RunResult<DefaultFlagState> {
        self.local_with_fixed_transfers(ENTER_DEFAULT, ENTER_DEFAULT * size_of::<(DefaultContext, InferenceFlags, bool, DefaultFlagState)>(), || {
            let (flag, enabled) = match context {
                DefaultContext::ParamSpec => (InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, true),
                DefaultContext::ParamSpecList => (InferenceFlags::ALLOW_PARAMSPEC_TYPE_EXPR, false),
                DefaultContext::TypeVarTuple => (InferenceFlags::IN_VALID_UNPACK_CONTEXT, true),
            };
            DefaultFlagState { flag, enabled: builder.context.inference_flags.replace(flag, enabled) }
        }).await
    }

    async fn restore_flags(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, flags: DefaultFlagState) -> RunResult<()> {
        self.local_with_fixed_transfers(RESTORE_DEFAULT, RESTORE_DEFAULT * size_of::<(DefaultFlagState, InferenceFlags, bool)>(), || builder.context.inference_flags.set(flags.flag, flags.enabled)).await
    }

    async fn type_expression(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> RunResult<Type<'db>> {
        #[cfg(test)]
        {
            let child = local::deferred_parameter_child_entered(builder, expression);
            let future = self.boxed_future_with_fixed_transfers(
                Ok((14, size_of::<[(&mut TypeInferenceBuilder<'db, 'ast>, &ast::Expr, TypeExpressionMode, &Self); 2]>())),
                || local::source::type_expression(builder, expression, TypeExpressionMode::Scoped, self),
            ).await?;
            let ty = local::observe_deferred_parameter_child_polling(child, future).await?;
            local::deferred_parameter_child_completed(child, builder);
            Ok(ty)
        }
        #[cfg(not(test))]
        self.boxed_future_with_fixed_transfers(
            Ok((14, size_of::<[(&mut TypeInferenceBuilder<'db, 'ast>, &ast::Expr, TypeExpressionMode, &Self); 2]>())),
            || local::source::type_expression(builder, expression, TypeExpressionMode::Scoped, self),
        ).await?.await
    }

    async fn ellipsis(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::ExprEllipsisLiteral) -> RunResult<Type<'db>> {
        self.boxed_future_with_fixed_transfers(
            Ok((11, size_of::<[(&mut TypeInferenceBuilder<'db, 'ast>, &ast::ExprEllipsisLiteral, &Self); 2]>())),
            || SourceExpressionEffects::ellipsis_literal(self, builder, expression),
        ).await?.await
    }

    async fn new_types(&self) -> RunResult<Vec<Type<'db>>> {
        self.local_quoted_with_fixed_transfers(const { empty_vec_quote::<Type<'db>>() }, Vec::new).await
    }

    async fn next_element<'expr>(&self, elements: &'expr [ast::Expr], cursor: &mut usize) -> RunResult<Option<&'expr ast::Expr>> {
        self.local_with_fixed_transfers(22, size_of::<[(&[ast::Expr], usize, Option<&ast::Expr>, bool); 3]>(), || {
            let element = elements.get(*cursor);
            if element.is_some() { *cursor += 1; }
            element
        }).await
    }

    async fn push_type(&self, types: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        let preparation = const {
            match buffer_quote_preparation(BufferQuotePreparation::VecReserveExact) {
                Ok((work, bytes)) => match (work.checked_add(17 + 42 + 8 * 11 + 9 * 8 + 5 * 5 + 56), bytes.checked_add(300 * size_of::<RunResult<TypeAppend>>())) {
                    (Some(work), Some(bytes)) => Ok((work, bytes)),
                    _ => Err(RunError::Contract("parameter append preparation overflow")),
                },
                Err(error) => Err(error),
            }
        };
        let admission = self.local_quoted_with_fixed_transfers(preparation, || -> RunResult<TypeAppend> {
            let len = types.len();
            let capacity = types.capacity();
            let required = Self::checked(len.checked_add(1))?;
            let target = if required > capacity { Self::checked(capacity.checked_mul(2))?.max(required).max(4) } else { capacity };
            let reserve = if required > capacity { target - len } else { 0 };
            let (reserve_work, reserve_bytes) = vec_reserve_exact_quote::<Type<'db>>(len, capacity, reserve)?;
            let (push_work, push_bytes) = const { prepared_vec_push_quote::<Type<'db>>() }?;
            let work = Self::checked(reserve_work.checked_add(push_work).and_then(|work| work.checked_add(1)))?;
            let bytes = Self::checked(reserve_bytes.checked_add(push_bytes))?;
            Ok(TypeAppend { quote: StorageQuote { work, bytes }, reserve })
        }).await??;
        self.local_with_fixed_transfers(admission.quote.work, admission.quote.bytes, || {
            types.reserve_exact(admission.reserve);
            types.push(ty);
        }).await
    }

    async fn copy_types(&self, types: &[Type<'db>]) -> RunResult<Vec<Type<'db>>> {
        let mut copied = DeferredTypeParameterEffects::new_types(self).await?;
        let mut cursor = 0;
        loop {
            let next = self.local_with_fixed_transfers(22, size_of::<[(&[Type<'db>], usize, Option<Type<'db>>); 3]>(), || {
                let value = types.get(cursor).copied();
                if value.is_some() { cursor += 1; }
                value
            }).await?;
            let Some(ty) = next else { break; };
            DeferredTypeParameterEffects::push_type(self, &mut copied, ty).await?;
        }
        Ok(copied)
    }

    async fn tuple_type(&self, builder: &TypeInferenceBuilder<'db, 'ast>, types: Vec<Type<'db>>) -> RunResult<Type<'db>> {
        let types = self.boxed_parameter_types(types).await?;
        let spec = self.local_with_fixed_transfers(11, size_of::<[(Box<[Type<'db>]>, FixedLengthTuple<Type<'db>>, TupleSpec<'db>); 2]>(), || TupleSpec::from(FixedLengthTuple::from(types))).await?;
        let program = self.boxed_future_with_fixed_transfers(
            Ok((8, size_of::<[(&Self, &crate::ProgramEnvironment<'db>); 2]>())),
            || self.environment_program(builder.program_environment()),
        ).await?.await?;
        let tuple = self.boxed_future_with_fixed_transfers(
            Ok((11, size_of::<[(crate::Program<'db>, TupleSpec<'db>, &A); 2]>())),
            || self.access.intern_tuple(program, spec),
        ).await?.await?;
        self.local_with_fixed_transfers(3, size_of::<Type<'db>>(), || Type::tuple(tuple)).await
    }

    async fn store(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, ty: Type<'db>) -> RunResult<()> {
        self.boxed_future_with_fixed_transfers(
            Ok((14, size_of::<[(&Self, &mut TypeInferenceBuilder<'db, 'ast>, &ast::Expr, Type<'db>); 2]>())),
            || SourceExpressionEffects::store_expression(self, builder, expression, ty),
        ).await?.await
    }

    async fn intern_constraints(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, types: Vec<Type<'db>>) -> RunResult<TypeVarConstraints<'db>> {
        let types = self.boxed_parameter_types(types).await?;
        self.boxed_future_with_fixed_transfers(
            Ok((8, size_of::<[(Box<[Type<'db>]>, &A); 2]>())),
            || self.access.intern_typevar_constraints(types),
        ).await?.await
    }

    async fn has_typevar(&self, builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> RunResult<bool> {
        self.boxed_future_with_fixed_transfers(
            Ok((11, size_of::<[(&Self, &TypeInferenceBuilder<'db, 'ast>, Type<'db>); 2]>())),
            || DeferredAssignmentEffects::has_typevar(self, builder, ty),
        ).await?.await
    }

    async fn generic_constraint(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::Expr) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(DeferredInferenceOperation::AssignmentConstraintDiagnostic)).await
    }

    async fn generic_bound(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::Expr) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(DeferredInferenceOperation::AssignmentBoundDiagnostic)).await
    }

    async fn outer_default(&self, builder: &TypeInferenceBuilder<'db, 'ast>, _ty: Type<'db>, _expression: &ast::Expr, _name: &str) -> RunResult<bool> {
        let node = self.boxed_future_with_fixed_transfers(
            Ok((14, size_of::<[(&Self, &TypeInferenceBuilder<'db, 'ast>, ty_python_core::scope::ScopeId<'db>); 2]>())),
            || ScopeEffects::scope_node(self, builder, builder.scope()),
        ).await?.await?;
        let needs_outer_check = self.local_with_fixed_transfers(4, size_of::<[(&NodeWithScopeKind, bool); 2]>(), || matches!(node, NodeWithScopeKind::FunctionTypeParameters(_) | NodeWithScopeKind::TypeAliasTypeParameters(_))).await?;
        if needs_outer_check {
            return self.unavailable(SourceOperation::Deferred(DeferredInferenceOperation::TypeParameterOuterScopeDefault)).await;
        }
        Ok(false)
    }

    async fn validate_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, name: Option<&str>, bounds: Option<TypeVarBoundOrConstraints<'db>>, ty: Type<'db>, expression: &ast::Expr, nodes: Option<BoundOrConstraintsNodes<'ast>>) -> RunResult<()> {
        self.boxed_future_with_fixed_transfers(
            Ok((28, size_of::<[(&mut TypeInferenceBuilder<'db, 'ast>, Option<&str>, Option<TypeVarBoundOrConstraints<'db>>, Type<'db>, &ast::Expr, Option<BoundOrConstraintsNodes<'ast>>, &Self); 2]>())),
            || validate_typevar_default_with(builder, name, bounds, ty, expression, nodes, self),
        ).await?.await
    }

    async fn paramspec_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, name: Option<&str>) -> RunResult<()> {
        self.infer_paramspec_default_source(builder, expression, name).await
    }

    async fn typevartuple_default(&self, builder: &mut TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr, name: Option<&str>) -> RunResult<()> {
        self.infer_typevartuple_default_source(builder, expression, name).await
    }

    async fn is_paramspec(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> RunResult<bool> {
        self.local_with_fixed_transfers(5, size_of::<[Type<'db>; 2]>(), || ()).await?;
        let kind = match ty {
            Type::TypeVar(variable) => self.deferred_bound_kind(variable).await?,
            Type::KnownInstance(KnownInstanceType::TypeVar(variable)) => {
                self.boxed_future_with_fixed_transfers(
                    Ok((8, size_of::<[(crate::types::typevar::TypeVarInstance<'db>, &Self); 2]>())),
                    || TypeVarBindingEffects::kind(self, variable),
                ).await?.await?
            }
            Type::KnownInstance(KnownInstanceType::TypeAliasType(_) | KnownInstanceType::MethodWrapper(_)) => {
                return self.unavailable(SourceOperation::Deferred(DeferredInferenceOperation::ParamSpecDefaultClass)).await;
            }
            _ => return Ok(false),
        };
        self.local_with_fixed_transfers(TEST_KIND, TEST_KIND * size_of::<(TypeVarKind, bool)>(), || kind.is_paramspec()).await
    }

    async fn is_typevartuple(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, ty: Type<'db>) -> RunResult<bool> {
        self.local_with_fixed_transfers(3, size_of::<[Type<'db>; 2]>(), || ()).await?;
        let Type::TypeVar(variable) = ty else { return Ok(false); };
        let kind = self.deferred_bound_kind(variable).await?;
        self.local_with_fixed_transfers(TEST_KIND, TEST_KIND * size_of::<(TypeVarKind, bool)>(), || kind.is_typevartuple()).await
    }

    async fn is_unpack(&self, builder: &TypeInferenceBuilder<'db, 'ast>, expression: &ast::Expr) -> RunResult<bool> {
        let flags = self.boxed_future_with_fixed_transfers(
            Ok((11, size_of::<[(&Self, &TypeInferenceBuilder<'db, 'ast>, &ast::Expr); 2]>())),
            || self.source_type_expression_flags(builder, expression),
        ).await?.await?;
        self.local_with_fixed_transfers(TEST_UNPACK, TEST_UNPACK * size_of::<(TypeExpressionFlags, bool)>(), || flags.contains(TypeExpressionFlags::UNPACK)).await
    }

    async fn invalid_paramspec(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::Expr) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(DeferredInferenceOperation::ParamSpecDefaultDiagnostic)).await
    }

    async fn invalid_typevartuple(&self, _builder: &TypeInferenceBuilder<'db, 'ast>, _expression: &ast::Expr) -> RunResult<()> {
        self.unavailable(SourceOperation::Deferred(DeferredInferenceOperation::TypeVarTupleDefaultDiagnostic)).await
    }
}
