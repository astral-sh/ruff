//! Admitted ParamSpec-default conversion retains each owned buffer until its children drain.

use salsa::execution_probe::{RunError, RunResult};

use super::super::class_selection::FixedFieldBorrow;
use super::super::lint_diagnostic_cost::{
    BufferQuotePreparation, arc_owner_quote, buffer_quote_preparation, fixed_box_quote,
    prepared_vec_push_quote, single_inline_smallvec_quote, vec_into_boxed_slice_quote, vector_with_capacity_quote,
};
use super::super::{FixedFieldCopy, SourceAccess, SourceEffects};
use crate::types::callable::CallableTypeKind;
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::local_transfer::generated_field_quote;
use crate::types::mapping::source::MappingSourceEffects;
use crate::types::signatures::{CallableSignature, ParametersKind, Signature};
use crate::types::tuple::TupleType;
use crate::types::typevar::default::lazy::paramspec::{
    ParamSpecDefaultEffects, ParamSpecDefaultInput, ParamSpecRecovery, fixed_elements, next_type,
};
use crate::types::typevar::{BoundTypeVarIdentity, TypeVarIdentity, TypeVarInstance};
use crate::types::{BoundTypeVarInstance, KnownClass, NominalInstanceType, Parameter, Parameters, Type};

const CALL_1: usize = 5;
const CALL_2: usize = 8;
const CALL_3: usize = 11;
const VECTOR_METADATA: usize = 17 + 42;
// Parameter constructor fields, annotated-type replacement, and shallow retirement of all fields.
const POSITIONAL_PARAMETER: usize = CALL_1 + CALL_2 + CALL_1 + 26;
// CharStr's inline-name drop checks its representation tag without touching heap storage.
const INLINE_NAME_RETIREMENT: usize = 4 * CALL_1 + 9;
// Two parameter constructors and annotations, names, array conversion and gradual-kind dispatch.
const RECOVERY_PARAMETERS: usize = 2 * POSITIONAL_PARAMETER + 2 * INLINE_NAME_RETIREMENT
    + 3 * CALL_1 + CALL_2 + 18;
// Signature::new, into_paramspec_value, single's wrapper, passive fields and shallow retirement.
const SIGNATURE_INPUT: usize = CALL_2 + 4 * CALL_1 + 34;

/// Adds fixed caller operations and transfers to an inline-constant storage quotation.
const fn with_callers(quote: RunResult<(usize, usize)>, work: usize, bytes: usize) -> RunResult<(usize, usize)> {
    match quote {
        Ok((base_work, base_bytes)) => match (base_work.checked_add(work), base_bytes.checked_add(bytes)) {
            (Some(work), Some(bytes)) => Ok((work, bytes)),
            _ => Err(RunError::Contract("ParamSpec default quotation overflow")),
        },
        Err(error) => Err(error),
    }
}

/// Quotes the shared Parameters owner and its handoff, excluding the array and nested entry owners.
const fn parameters_owner_quote<'db>() -> RunResult<(usize, usize)> {
    match Parameters::<'db>::allocation_layout() {
        Ok(layout) => with_callers(
            arc_owner_quote(layout),
            2 * CALL_2 + CALL_1 + 12,
            (2 * CALL_2 + CALL_1 + 12) * size_of::<(Box<[Parameter<'db>]>, ParametersKind<'db>, Parameters<'db>)>(),
        ),
        Err(_) => Err(RunError::Contract("parameter owner layout overflow")),
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Transfers an admitted parameter buffer into its boxed array and shared owner with an explicit kind.
    /// Every live entry, including nested names or defaults, must already have prepaid retirement.
    pub(in crate::types::infer::builder) async fn finish_owned_parameters(
        &self,
        parameters: Vec<Parameter<'db>>,
        kind: ParametersKind<'db>,
    ) -> RunResult<Parameters<'db>> {
        let preparation = const { with_callers(
            buffer_quote_preparation(BufferQuotePreparation::VecIntoBoxedSlice),
            VECTOR_METADATA + 2 * CALL_1 + CALL_2 + 8,
            (VECTOR_METADATA + 2 * CALL_1 + CALL_2 + 8) * size_of::<(&Vec<Parameter<'db>>, usize, RunResult<(usize, usize)>)>(),
        ) };
        let quote = self.local_quoted_with_fixed_transfers(preparation, || {
            vec_into_boxed_slice_quote::<Parameter<'db>>(parameters.len(), parameters.capacity())
        }).await??;
        let boxed = self.local_quoted_with_fixed_transfers(Ok(quote), || parameters.into_boxed_slice()).await?;
        self.local_quoted_with_fixed_transfers(const { parameters_owner_quote() }, || {
            Parameters::from_owned(boxed, kind)
        }).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ParamSpecDefaultEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn classify(&self, ty: Type<'db>) -> RunResult<ParamSpecDefaultInput<'db>> {
        self.local_quoted_with_fixed_transfers(
            const { Ok((CALL_1 + 24, (CALL_1 + 24) * size_of::<(Type<'db>, ParamSpecDefaultInput<'db>)>())) },
            || ParamSpecDefaultInput::new(ty),
        ).await
    }

    async fn nominal_known(&self, instance: NominalInstanceType<'db>) -> RunResult<Option<KnownClass>> {
        self.boxed_future_with_fixed_transfers(
            const { Ok((CALL_3 + 4, size_of::<[(NominalInstanceType<'db>, NominalClassFacts, &Self); 3]>())) },
            || nominal_known_class_with(instance, NominalClassFacts, self),
        ).await?.await
    }

    async fn exact_tuple(&self, instance: NominalInstanceType<'db>) -> RunResult<Option<TupleType<'db>>> {
        self.local_quoted_with_fixed_transfers(const { Ok((CALL_1 + 6, size_of::<[(NominalInstanceType<'db>, Option<TupleType<'db>>); 3]>())) }, || instance.exact_tuple()).await
    }

    async fn elements(&self, tuple: TupleType<'db>) -> RunResult<Option<&'db [Type<'db>]>> {
        let spec = self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |tuple: TupleType<'db>, context| tuple.field_requests(context),
                |tuple: TupleType<'db>, context| tuple.field_requests(context).tuple(),
            ),
            || self.access.endpoint().read_field(tuple.field_requests(self.access.endpoint().field_request_context()).tuple(), &FixedFieldBorrow),
        ).await?.await;
        self.local_quoted_with_fixed_transfers(const { Ok((3 * CALL_1 + 9, size_of::<[&[Type<'db>]; 12]>())) }, || fixed_elements(spec)).await
    }

    async fn bound_is_paramspec(&self, variable: BoundTypeVarInstance<'db>) -> RunResult<bool> {
        let identity = self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |variable: BoundTypeVarInstance<'db>, context| variable.field_requests(context),
                |variable: BoundTypeVarInstance<'db>, context| variable.identity_request(context),
            ),
            || self.access.endpoint().read_field(variable.identity_request(self.access.endpoint().field_request_context()), &FixedFieldCopy),
        ).await?.await;
        let identity = self.local_with_fixed_transfers(4, size_of::<[BoundTypeVarIdentity<'db>; 3]>(), || identity.identity).await?;
        self.boxed_future_with_fixed_transfers(
            const { Ok((CALL_2 + 4, size_of::<[(TypeVarIdentity<'db>, &Self); 3]>())) },
            || self.paramspec_identity_kind(identity),
        ).await?.await
    }

    async fn unbound_is_paramspec(&self, variable: TypeVarInstance<'db>) -> RunResult<bool> {
        let identity = self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |variable: TypeVarInstance<'db>, context| variable.field_requests(context),
                |variable: TypeVarInstance<'db>, context| variable.field_requests(context).identity(),
            ),
            || self.access.endpoint().read_field(variable.field_requests(self.access.endpoint().field_request_context()).identity(), &FixedFieldCopy),
        ).await?.await;
        self.boxed_future_with_fixed_transfers(
            const { Ok((CALL_2 + 4, size_of::<[(TypeVarIdentity<'db>, &Self); 3]>())) },
            || self.paramspec_identity_kind(identity),
        ).await?.await
    }

    async fn new_parameters(&self, elements: &[Type<'db>]) -> RunResult<Vec<Parameter<'db>>> {
        let quote = self.local_quoted_with_fixed_transfers(
            const { with_callers(buffer_quote_preparation(BufferQuotePreparation::VectorWithCapacity), CALL_1 + 5, (CALL_1 + 5) * size_of::<(&[Type<'db>], usize)>()) },
            || vector_with_capacity_quote::<Parameter<'db>>(elements.len()),
        ).await??;
        self.local_quoted_with_fixed_transfers(Ok(quote), || Vec::with_capacity(elements.len())).await
    }

    async fn next(&self, remaining: &mut &'db [Type<'db>]) -> RunResult<Option<Type<'db>>> {
        self.local_quoted_with_fixed_transfers(
            const { Ok((3 * CALL_1 + 14, (3 * CALL_1 + 14) * size_of::<(&[Type<'db>], Option<(&Type<'db>, &[Type<'db>])>, Option<Type<'db>>)>())) },
            || next_type(remaining),
        ).await
    }

    async fn push(&self, parameters: &mut Vec<Parameter<'db>>, ty: Type<'db>) -> RunResult<()> {
        self.local_quoted_with_fixed_transfers(
            const { with_callers(prepared_vec_push_quote::<Parameter<'db>>(), POSITIONAL_PARAMETER, POSITIONAL_PARAMETER * size_of::<(Parameter<'db>, Type<'db>)>()) },
            || {
                parameters.push(Parameter::positional_only(None).with_annotated_type(ty));
                #[cfg(test)]
                crate::types::infer::source_runtime::tests::lazy_defaults::paramspec_conversion::parameter_appended(self.db());
            },
        ).await
    }

    async fn finish(&self, parameters: Vec<Parameter<'db>>) -> RunResult<Parameters<'db>> {
        let parameters = self.boxed_future_with_fixed_transfers(
            const { Ok((CALL_3 + 4, size_of::<[(Vec<Parameter<'db>>, ParametersKind<'db>, &Self); 3]>())) },
            || self.finish_owned_parameters(parameters, ParametersKind::Standard),
        ).await?.await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::lazy_defaults::paramspec_conversion::parameters_constructed(self.db());
        Ok(parameters)
    }

    async fn recovery(&self, recovery: ParamSpecRecovery) -> RunResult<Parameters<'db>> {
        let quote = const {
            match fixed_box_quote::<[Parameter<'db>; 2]>() {
                Ok((work, bytes)) => with_callers(parameters_owner_quote(), work + RECOVERY_PARAMETERS, bytes + RECOVERY_PARAMETERS * size_of::<[Parameter<'db>; 2]>()),
                Err(error) => Err(error),
            }
        };
        self.local_quoted_with_fixed_transfers(quote, || recovery.parameters()).await
    }

    async fn callable(&self, parameters: Parameters<'db>) -> RunResult<Type<'db>> {
        let signatures = self.local_quoted_with_fixed_transfers(
            const { with_callers(single_inline_smallvec_quote::<Signature<'db>>(), SIGNATURE_INPUT, SIGNATURE_INPUT * size_of::<(Signature<'db>, CallableSignature<'db>, Parameters<'db>)>()) },
            || CallableSignature::single(Signature::new(parameters, Type::unknown()).into_paramspec_value()),
        ).await?;
        let callable = self.boxed_future_with_fixed_transfers(
            const { Ok((14 + 4, size_of::<[(CallableSignature<'db>, CallableTypeKind, Option<crate::types::function::OverloadLiteral<'db>>, &Self); 3]>())) },
            || MappingSourceEffects::owned_mapped_callable(self, signatures, CallableTypeKind::ParamSpecValue, None),
        ).await?.await?;
        self.local_with_fixed_transfers(4, size_of::<[Type<'db>; 3]>(), || Type::Callable(callable)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Returns whether an admitted identity's kind is a ParamSpec, for either bound or unbound inputs.
    async fn paramspec_identity_kind(&self, identity: TypeVarIdentity<'db>) -> RunResult<bool> {
        let kind = self.boxed_future_with_fixed_transfers(
            generated_field_quote(
                |identity: TypeVarIdentity<'db>, context| identity.field_requests(context),
                |identity: TypeVarIdentity<'db>, context| identity.field_requests(context).kind(),
            ),
            || self.access.endpoint().read_field(identity.field_requests(self.access.endpoint().field_request_context()).kind(), &FixedFieldCopy),
        ).await?.await;
        self.local_quoted_with_fixed_transfers(const { Ok((CALL_1 + 5, (CALL_1 + 5) * size_of::<(crate::types::TypeVarKind, bool)>())) }, || kind.is_paramspec()).await
    }
}
