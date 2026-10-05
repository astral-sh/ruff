//! Storage operations for retained signature and tuple mapping.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use smallvec::SmallVec;

use super::super::lint_diagnostic_cost::{
    BufferQuotePreparation, buffer_quote_preparation, fixed_box_quote, name_clone_retirement_quote,
    prepared_smallvec_push_quote, prepared_vec_push_quote, smallvec_metadata_quote, smallvec_with_capacity_quote, vec_into_boxed_slice_quote,
    vec_into_smallvec_quote, vec_reserve_exact_quote, vector_with_capacity_quote,
};
use super::{SourceAccess, SourceEffects};
use crate::types::constraints::control::sequence_growth;
use crate::types::signatures::{Parameter, ParameterKind, Signature};
use crate::types::tuple::buffer::{fixed_spec, variable_spec};
use crate::types::tuple::{TupleSpec, VariableSegment};
use crate::types::Type;

const CALL_1: usize = 5;
const CALL_2: usize = 8;
const VECTOR_METADATA: usize = 17 + 42;

const fn with_callers(quote: RunResult<(usize, usize)>, work: usize, bytes: usize) -> RunResult<(usize, usize)> {
    match quote {
        Ok((base_work, base_bytes)) => match (base_work.checked_add(work), base_bytes.checked_add(bytes)) {
            (Some(work), Some(bytes)) => Ok((work, bytes)),
            _ => Err(RunError::Contract("mapping storage quotation overflow")),
        },
        Err(error) => Err(error),
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    async fn mapping_vector<T>(&self, capacity: usize) -> RunResult<Vec<T>> {
        let quote = self.local_quoted_with_fixed_transfers(
            const { buffer_quote_preparation(BufferQuotePreparation::VectorWithCapacity) },
            || vector_with_capacity_quote::<T>(capacity),
        ).await??;
        self.local_quoted_with_fixed_transfers(Ok(quote), || Vec::with_capacity(capacity)).await
    }

    pub(super) async fn new_mapping_parameters(&self, capacity: usize) -> RunResult<Vec<Parameter<'db>>> {
        self.function_mapping_child(|| self.mapping_vector(capacity)).await
    }

    pub(super) async fn push_mapping_parameter(&self, parameters: &mut Vec<Parameter<'db>>, parameter: Parameter<'db>) -> RunResult<()> {
        self.local_with_fixed_transfers(VECTOR_METADATA + 4, size_of::<usize>() * 6, || {
            if parameters.len() < parameters.capacity() { Ok(()) }
            else { Err(RunError::Contract("mapping parameter push exceeds reserved capacity")) }
        }).await??;
        self.local_quoted_with_fixed_transfers(
            const { with_callers(prepared_vec_push_quote::<Parameter<'db>>(), 8, size_of::<Parameter<'db>>()) },
            || parameters.push(parameter),
        ).await
    }

    pub(super) async fn new_mapping_overloads(&self, capacity: usize) -> RunResult<SmallVec<[Signature<'db>; 1]>> {
        let quote = self.local_quoted_with_fixed_transfers(
            const { buffer_quote_preparation(BufferQuotePreparation::SmallVecWithCapacity) },
            || smallvec_with_capacity_quote::<Signature<'db>, 1>(capacity),
        ).await??;
        self.local_quoted_with_fixed_transfers(Ok(quote), || SmallVec::with_capacity(capacity)).await
    }

    pub(super) async fn push_mapping_overload(&self, overloads: &mut SmallVec<[Signature<'db>; 1]>, signature: Signature<'db>) -> RunResult<()> {
        self.local_quoted_with_fixed_transfers(const { with_callers(smallvec_metadata_quote::<Signature<'db>, 1>(), 4, size_of::<usize>() * 2) }, || {
            if overloads.len() < overloads.capacity() { Ok(()) }
            else { Err(RunError::Contract("mapping overload push exceeds reserved capacity")) }
        }).await??;
        self.local_quoted_with_fixed_transfers(
            const { with_callers(prepared_smallvec_push_quote::<Signature<'db>, 1>(), 8, size_of::<Signature<'db>>()) },
            || overloads.push(signature),
        ).await
    }

    pub(super) async fn mapping_box_local<T, U>(&self, action: impl FnOnce() -> U) -> RunResult<U> {
        self.local_quoted_with_fixed_transfers(
            const { with_callers(fixed_box_quote::<T>(), CALL_2 + 2 * CALL_1 + 12, size_of::<T>() * 2) },
            action,
        ).await
    }

    pub(super) async fn mapping_parameter_kind_local(&self, action: impl FnOnce() -> ParameterKind<'db>) -> RunResult<ParameterKind<'db>> {
        self.local_quoted_with_fixed_transfers(
            const { with_callers(name_clone_retirement_quote(), CALL_1 + 18, size_of::<ParameterKind<'db>>() * 4 + size_of::<Option<Name>>() * 2) },
            action,
        ).await
    }

    pub(super) async fn new_mapping_tuple_elements(&self, capacity: usize) -> RunResult<Vec<Type<'db>>> {
        self.function_mapping_child(|| self.mapping_vector(capacity)).await
    }

    pub(super) async fn push_mapping_tuple_element(&self, elements: &mut Vec<Type<'db>>, ty: Type<'db>) -> RunResult<()> {
        let (len, capacity) = self.local_with_fixed_transfers(VECTOR_METADATA + 2, size_of::<usize>() * 6, || {
            (elements.len(), elements.capacity())
        }).await?;
        let additional = self.local_with_fixed_transfers(
            4 * CALL_2 + 4 * CALL_1 + 32,
            size_of::<usize>() * 24 + size_of::<Option<usize>>() * 12,
            || {
                if len != capacity { return Ok(None); }
                let required = len.checked_add(1).ok_or(RunError::Contract("tuple buffer length overflow"))?;
                let growth = sequence_growth::<Type<'db>, Infallible>(capacity, required)
                    .map_err(|_| RunError::Contract("tuple buffer growth overflow"))?;
                Ok(Some(growth.requested_capacity - len))
            },
        ).await??;
        if let Some(additional) = additional {
            let quote = self.local_quoted_with_fixed_transfers(
                const { buffer_quote_preparation(BufferQuotePreparation::VecReserveExact) },
                || vec_reserve_exact_quote::<Type<'db>>(len, capacity, additional),
            ).await??;
            self.local_quoted_with_fixed_transfers(Ok(quote), || elements.reserve_exact(additional)).await?;
        }
        self.local_quoted_with_fixed_transfers(
            const { with_callers(prepared_vec_push_quote::<Type<'db>>(), 1, size_of::<Type<'db>>()) },
            || elements.push(ty),
        ).await
    }

    pub(super) async fn finish_mapping_tuple_elements(&self, elements: &mut Vec<Type<'db>>, variable: Option<(usize, VariableSegment<'db>)>) -> RunResult<TupleSpec<'db>> {
        let (len, capacity) = self.local_with_fixed_transfers(VECTOR_METADATA + 2, size_of::<usize>() * 6, || {
            (elements.len(), elements.capacity())
        }).await?;
        let quote = match variable {
            None => self.local_quoted_with_fixed_transfers(
                const { buffer_quote_preparation(BufferQuotePreparation::VecIntoBoxedSlice) },
                || vec_into_boxed_slice_quote::<Type<'db>>(len, capacity),
            ).await??,
            Some(_) => self.local_quoted_with_fixed_transfers(
                const { buffer_quote_preparation(BufferQuotePreparation::VecIntoSmallVec) },
                || vec_into_smallvec_quote::<Type<'db>, 0>(len, capacity),
            ).await??,
        };
        let quote = self.local_with_fixed_transfers(8, size_of::<usize>() * 8, || {
            with_callers(Ok(quote), 3 * CALL_1 + CALL_2 + 12, size_of::<(Vec<Type<'db>>, TupleSpec<'db>, Option<(usize, VariableSegment<'db>)>)>() * 3)
        }).await?;
        // Borrow the buffer until admission succeeds, so refusal drains children before its owner.
        self.local_quoted_with_fixed_transfers(quote, || {
            let elements = std::mem::take(elements);
            match variable {
                None => fixed_spec(elements),
                Some((prefix, variable)) => variable_spec(elements, prefix, variable),
            }
        }).await
    }
}
