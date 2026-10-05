use std::convert::Infallible;

use ruff_python_ast as ast;
use salsa::execution_probe::{RunError, RunResult};

use super::storage::sequence_merge;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::types::class::protocol_status::static_is_protocol_with;
use crate::types::constraints::ConstraintSetBuilder;
use crate::types::context::InferContext;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::infer::builder::TypeInferenceBuilder;
use crate::types::infer::builder::local::SpecializationTarget;
use crate::types::infer::builder::source_expression::SourceExpressionEffects;
use crate::types::infer::builder::subscript::legacy_generic::LegacyGenericEffects;
use crate::types::infer::builder::subscript::specialization::{
    Body, ClassSubclassEffects, ClassSubclassFacts, ExplicitSpecializationEffects,
    ExplicitSpecializationFacts, Packing, Report, SavedFlags, TypeArgument, class_subclass_with,
    pack_fixed_with,
};
use crate::types::infer::{InferenceFlags, TypeExpressionFlags};
use crate::types::legacy_typevars::LegacyTypeVarTraversalEffects;
use crate::types::relation::source::resources::RelationResourceAccess;
use crate::types::subclass_of::SubclassInstanceEffects;
use crate::types::tuple::construction::tuple_type;
use crate::types::tuple::{TupleSpec, TupleSpecBuilder};
use crate::types::type_expression_conversion::TypeExpressionConversionEffects;
use crate::types::typevar::bounds::{TypeVarBoundsEffects, typevar_bounds_with};
use crate::types::typevar::{
    TypeVarBoundOrConstraintsEvaluation, TypeVarConstraints, TypeVarInstance,
};
use crate::types::{
    BoundTypeVarInstance, ClassType, GenericContext, StaticClassLiteral, SubclassOfInner, Type,
    TypeVarBoundOrConstraints, TypeVarKind,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassSubclassEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // The selector copies the specialized class and optional instance into its finite facts,
        // then transfers their class/protocol result into the subclass child. Child futures admit
        // their own storage, result transfers, and semantic work.
        let bytes = size_of::<(
            [ClassType<'db>; 2],
            [Type<'db>; 2],
            [SubclassOfInner<'db>; 3],
        )>();
        self.local_with_fixed_transfers(16, bytes, || ()).await
    }

    async fn specialize(
        &self,
        class: StaticClassLiteral<'db>,
        generic_context: GenericContext<'db>,
        types: &[Option<Type<'db>>],
    ) -> RunResult<ClassType<'db>> {
        let bytes = size_of::<[
            (&Self, StaticClassLiteral<'db>, GenericContext<'db>, &[Option<Type<'db>>]);
            2
        ]>();
        self.boxed_future_with_fixed_transfers(Ok((14, bytes)), || {
            self.apply_class_specialization(class, generic_context, types)
        })
        .await?
        .await
    }

    async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let bytes = size_of::<[(StaticClassLiteral<'db>, &Self); 2]>();
        self.boxed_future_with_fixed_transfers(Ok((8, bytes)), || {
            static_is_protocol_with(class, self)
        })
        .await?
        .await
    }

    async fn instance(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
    ) -> RunResult<Type<'db>> {
        let bytes = size_of::<[(&Self, &ProgramEnvironment<'db>, ClassType<'db>); 2]>();
        self.boxed_future_with_fixed_transfers(Ok((11, bytes)), || {
            TypeExpressionConversionEffects::instance(self, env, class)
        })
        .await?
        .await
    }

    async fn subclass(
        &self,
        env: &ProgramEnvironment<'db>,
        inner: SubclassOfInner<'db>,
    ) -> RunResult<Type<'db>> {
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::signature_annotations::subclass_constructing(
            self.db(),
            inner,
        );
        let bytes = size_of::<[(&Self, &ProgramEnvironment<'db>, SubclassOfInner<'db>); 2]>();
        self.boxed_future_with_fixed_transfers(Ok((11, bytes)), || {
            SubclassInstanceEffects::subclass(self, env, inner)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    async fn specialization_buffer<T: Copy>(&self, capacity: usize) -> RunResult<Vec<T>> {
        let quote = sequence_merge::<T>(0, 0, capacity).ok_or(RunError::Contract(
            "explicit-specialization buffer quotation overflow",
        ))?;
        let allocation_capacity = if quote.bytes == 0 {
            0
        } else {
            Self::checked(quote.bytes.checked_div(size_of::<T>()))?
        };
        let work = Self::checked(
            allocation_capacity
                .checked_mul(2)
                .and_then(|retirement| quote.work.checked_add(retirement)),
        )?;
        self.local(work, quote.bytes, || Vec::with_capacity(capacity))
            .await
    }

    async fn specialization_push<T: Copy>(&self, values: &mut Vec<T>, value: T) -> RunResult<()> {
        let quote = sequence_merge::<T>(values.len(), values.capacity(), 1).ok_or(
            RunError::Contract("explicit-specialization append quotation overflow"),
        )?;
        // Capacity growth also admits disposal of the retained buffer after an interrupted child.
        let work = Self::checked(
            quote
                .bytes
                .checked_mul(2)
                .and_then(|retirement| quote.work.checked_add(retirement)),
        )?;
        self.local(work, quote.bytes, || values.push(value)).await
    }
}

impl<'run, 'db: 'run, 'ast, A: SourceAccess<'run, 'db>> ExplicitSpecializationEffects<'db, 'ast>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type Builder = <A::Resources as RelationResourceAccess<'run, 'db>>::Builder;
    type Target = SpecializationTarget<'db, Infallible>;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_is_protocol_with(class, self).await
    }

    async fn declares_class_member(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        let scope = self
            .field(
                class
                    .field_requests(self.access.endpoint().field_request_context())
                    .body_scope(),
            )
            .await?;
        let table = self.access.place_table(scope).await?;
        let work = Self::checked(table.symbol_lookup_work("__class__".len()))?;
        self.local(work, 0, || table.symbol_id("__class__").is_some())
            .await
    }

    async fn protocol_writable_member(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _class: StaticClassLiteral<'db>,
        _context: GenericContext<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ExplicitSpecializationProtocolMember)
            .await
    }

    async fn replace_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        flag: InferenceFlags,
        value: bool,
    ) -> RunResult<bool> {
        self.local(1, 0, || {
            builder.context.inference_flags.replace(flag, value)
        })
        .await
    }

    async fn restore_flag(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        flag: InferenceFlags,
        value: bool,
    ) -> RunResult<()> {
        self.local(1, 0, || builder.context.inference_flags.set(flag, value))
            .await
    }

    async fn new_builder(&self) -> RunResult<Self::Builder> {
        self.access
            .resources()
            .invocation_builder(self.access.endpoint())
            .await
    }

    async fn exactly_one_paramspec(&self, context: GenericContext<'db>) -> RunResult<bool> {
        let variables = TypeVarBindingEffects::variables(self, context).await?;
        let variable = self
            .local(2, 0, || {
                if variables.len() == 1 {
                    GenericContext::variable_at_in(variables, 0)
                } else {
                    None
                }
            })
            .await?;
        match variable {
            Some(variable) => {
                let kind = LegacyTypeVarTraversalEffects::variable_kind(self, variable).await?;
                self.local(1, 0, || kind.is_paramspec()).await
            }
            None => Ok(false),
        }
    }

    async fn variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<Vec<BoundTypeVarInstance<'db>>> {
        let variables = TypeVarBindingEffects::variables(self, context).await?;
        let len = self.local(1, 0, || variables.len()).await?;
        let mut result = self.specialization_buffer(len).await?;
        let mut cursor = 0;
        while let Some(variable) =
            TypeVarBindingEffects::next_variable(self, variables, &mut cursor).await?
        {
            self.specialization_push(&mut result, variable).await?;
        }
        Ok(result)
    }

    async fn variable_kind(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<TypeVarKind> {
        LegacyTypeVarTraversalEffects::variable_kind(self, variable).await
    }

    async fn new_inferred(&self, len: usize) -> RunResult<Vec<Option<Type<'db>>>> {
        let mut inferred = self.specialization_buffer(len).await?;
        let work = Self::checked(len.checked_add(1))?;
        self.local(work, 0, || inferred.resize(len, None)).await?;
        Ok(inferred)
    }

    async fn new_arguments<'expr>(
        &self,
        capacity: usize,
    ) -> RunResult<Vec<TypeArgument<'db, 'expr>>> {
        self.specialization_buffer(capacity).await
    }

    async fn new_types(&self, capacity: usize) -> RunResult<Vec<Option<Type<'db>>>> {
        self.specialization_buffer(capacity).await
    }

    async fn push_argument<'expr>(
        &self,
        arguments: &mut Vec<TypeArgument<'db, 'expr>>,
        argument: TypeArgument<'db, 'expr>,
    ) -> RunResult<()> {
        self.specialization_push(arguments, argument).await
    }

    async fn push_type(
        &self,
        types: &mut Vec<Option<Type<'db>>>,
        ty: Option<Type<'db>>,
    ) -> RunResult<()> {
        self.specialization_push(types, ty).await
    }

    async fn extend_arguments<'expr>(
        &self,
        arguments: &mut Vec<TypeArgument<'db, 'expr>>,
        suffix: Vec<TypeArgument<'db, 'expr>>,
    ) -> RunResult<()> {
        let quote = sequence_merge::<TypeArgument<'db, 'expr>>(
            arguments.len(),
            arguments.capacity(),
            suffix.len(),
        )
        .ok_or(RunError::Contract(
            "explicit-specialization extension quotation overflow",
        ))?;
        let work = Self::checked(
            quote
                .bytes
                .checked_mul(2)
                .and_then(|retirement| quote.work.checked_add(retirement))
                .and_then(|work| work.checked_add(suffix.capacity())),
        )?;
        self.local(work, quote.bytes, || arguments.extend(suffix))
            .await
    }

    async fn push_missing(
        &self,
        variables: &mut Vec<BoundTypeVarInstance<'db>>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.specialization_push(variables, variable).await
    }

    async fn expression_flags(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        expression: &ast::Expr,
    ) -> RunResult<TypeExpressionFlags> {
        let work = Self::checked(
            builder
                .type_expression_flags
                .capacity()
                .checked_mul(4)
                .and_then(|work| work.checked_add(4)),
        )?;
        self.local(work, 0, || builder.type_expression_flags(expression))
            .await
    }

    async fn exact_tuple(&self, ty: Type<'db>) -> RunResult<Option<&'db TupleSpec<'db>>> {
        LegacyGenericEffects::exact_tuple(self, ty).await
    }

    async fn new_tuple_builder(&self, _capacity: usize) -> RunResult<TupleSpecBuilder<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationVariadic)
            .await
    }

    async fn tuple_concat(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _tuple: &mut TupleSpecBuilder<'db>,
        _other: &TupleSpec<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ExplicitSpecializationVariadic)
            .await
    }

    async fn tuple_concat_typevar(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _tuple: &mut TupleSpecBuilder<'db>,
        _variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ExplicitSpecializationVariadic)
            .await
    }

    async fn tuple_push(
        &self,
        _tuple: &mut TupleSpecBuilder<'db>,
        _ty: Type<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ExplicitSpecializationVariadic)
            .await
    }

    async fn finish_tuple(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _tuple: TupleSpecBuilder<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationVariadic)
            .await
    }

    async fn paramspec(
        &self,
        _builder: &mut TypeInferenceBuilder<'db, 'ast>,
        _expression: &ast::Expr,
        _exactly_one: bool,
    ) -> RunResult<Result<Type<'db>, ()>> {
        self.unavailable(SourceOperation::ExplicitSpecializationParamSpec)
            .await
    }

    async fn pack_fixed<'expr>(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        packing: &mut Packing<'db, 'expr>,
        argument: TypeArgument<'db, 'expr>,
        suffix: bool,
    ) -> RunResult<()> {
        pack_fixed_with(
            builder,
            packing,
            argument,
            suffix,
            ExplicitSpecializationFacts,
            self,
        )
        .await
    }

    async fn unknown_paramspec(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationParamSpec)
            .await
    }

    async fn unknown_variadic(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationVariadic)
            .await
    }

    async fn default_type(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.access.bound_typevar_default(variable).await
    }

    async fn bound_or_constraints(
        &self,
        builder: &TypeInferenceBuilder<'db, 'ast>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        let typevar = TypeVarBindingEffects::bound_typevar(self, variable).await?;
        typevar_bounds_with(typevar, builder.program_environment(), self).await
    }

    async fn bind_validation(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _ty: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationBoundMapping)
            .await
    }

    async fn constraints_type(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _constraints: TypeVarConstraints<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ExplicitSpecializationBounds)
            .await
    }

    async fn never_assignable(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _constraints: &ConstraintSetBuilder<'db>,
        _source: Type<'db>,
        _target: Type<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ExplicitSpecializationBounds)
            .await
    }

    async fn report(
        &self,
        _builder: &TypeInferenceBuilder<'db, 'ast>,
        _report: Report<'db, '_, '_>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::ExplicitSpecializationDiagnostic)
            .await
    }

    async fn store_inferred(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        slice: &ast::Expr,
        types: Vec<Option<Type<'db>>>,
    ) -> RunResult<()> {
        let bytes = Self::checked(TupleSpec::fixed_clone_requested_bytes(types.len()))?;
        let retirement = Self::checked(TupleSpec::fixed_retirement_work(types.len()))?;
        let work = Self::checked(
            bytes
                .checked_mul(2)
                .and_then(|work| work.checked_add(retirement))
                .and_then(|work| work.checked_add(types.capacity()))
                .and_then(|work| work.checked_add(types.len().checked_mul(2)?))
                .and_then(|work| work.checked_add(4)),
        )?;
        let spec = self
            .local(work, bytes, || {
                TupleSpec::heterogeneous(types.into_iter().map(|ty| ty.unwrap_or(Type::unknown())))
            })
            .await?;
        let tuple = tuple_type(builder.db(), builder.program_environment(), &spec, self).await?;
        SourceExpressionEffects::store_expression(self, builder, slice, Type::tuple(tuple)).await
    }

    async fn finish_target(
        &self,
        builder: &mut TypeInferenceBuilder<'db, 'ast>,
        target: Self::Target,
        generic_context: GenericContext<'db>,
        types: &[Option<Type<'db>>],
    ) -> RunResult<Type<'db>> {
        match target {
            SpecializationTarget::ClassObject(class) => Ok(Type::from(
                self.apply_class_specialization(class, generic_context, types)
                    .await?,
            )),
            SpecializationTarget::ClassSubclass(class) => {
                #[cfg(test)]
                crate::types::infer::source_runtime::tests::signature_annotations::subclass_target_entered(
                    class,
                );
                let bytes = size_of::<(
                    [
                        (
                            &ProgramEnvironment<'db>,
                            StaticClassLiteral<'db>,
                            GenericContext<'db>,
                            &[Option<Type<'db>>],
                            ClassSubclassFacts,
                            &Self,
                        );
                        2
                    ],
                    [&TypeInferenceBuilder<'db, 'ast>; 2],
                    [&InferContext<'db, 'ast>; 2],
                    [&ProgramEnvironment<'db>; 3],
                )>();
                self.boxed_future_with_fixed_transfers(Ok((31, bytes)), || {
                    class_subclass_with(
                        builder.program_environment(),
                        class,
                        generic_context,
                        types,
                        ClassSubclassFacts,
                        self,
                    )
                })
                .await?
                .await
            }
            SpecializationTarget::Custom(never) => match never {},
        }
    }

    async fn retire_body<'expr>(
        &self,
        body: Body<'db, 'expr, Self::Builder>,
    ) -> RunResult<SavedFlags> {
        self.local(size_of_val(&body) + 1, 0, || body.retire())
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVarBoundsEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn stored_bounds(
        &self,
        typevar: TypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraintsEvaluation<'db>>> {
        let request = self.local_with_fixed_transfers(16, 0, || {
            typevar.bound_or_constraints_request(self.access.endpoint().field_request_context())
        }).await?;
        self.field_with_profile(request, &super::FixedFieldCopy).await
    }

    async fn lazy_upper_bound(
        &self,
        _typevar: TypeVarInstance<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::ExplicitSpecializationLazyUpperBound)
            .await
    }

    async fn lazy_constraints(
        &self,
        _typevar: TypeVarInstance<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<TypeVarConstraints<'db>>> {
        self.unavailable(SourceOperation::ExplicitSpecializationLazyConstraints)
            .await
    }
}
