//! Finite payload accounting for canonical tuples and their class-conversion memo.

use salsa::execution_probe::{
    FixedQueryKeyProfile as CopyMemoProfile, NativeValueOperation, NativeValueQuote,
    RegistryBuilder, RetainedInput, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{ToClassTypeConfiguration, Tuple, TupleSpec, TupleType, VariableSegment, to_class_type};
use crate::Db;
use crate::types::Type;
use crate::types::infer::local_with_fixed_transfers_at;

/// Quotes generated tuple-handle conversion and equality of class discriminants and handles.
/// Neither operation follows tuple elements, class definitions, or generic specialization data.
pub(in crate::types) async fn quote_class_conversion_native_value<'call, 'run, 'db: 'run>(
    endpoint: TaskEndpoint<'run, 'db>,
    operation: NativeValueOperation<'call, 'db, ToClassTypeConfiguration>,
) -> RunResult<NativeValueQuote> {
    local_with_fixed_transfers_at(&endpoint, 3, 0, || {
            match operation {
                NativeValueOperation::InputConversion(RetainedInput::SalsaStruct(_)) => {
                    Ok(NativeValueQuote {
                        work: 1,
                        requested_bytes: size_of::<TupleType<'db>>(),
                        cleanup_work: 0,
                    })
                }
                NativeValueOperation::InputConversion(RetainedInput::Interned(_)) => {
                    Err(RunError::Contract("tuple class input requires a generated tuple handle"))
                }
                NativeValueOperation::Comparison { .. } => Ok(NativeValueQuote {
                    work: 9,
                    requested_bytes: size_of::<bool>(),
                    cleanup_work: 0,
                }),
            }
        })
        .await?
}

impl FiniteInternedConfiguration for TupleType<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        fields.1.field_work()?.checked_add(1)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        fields
            .1
            .field_work_with(&mut || fuel.consume(1))?
            .checked_add(1)
            .ok_or(QuoteError::Overflow)
    }
}

impl TupleSpec<'_> {
    pub(in crate::types) fn storage_len(&self) -> usize {
        match self {
            Tuple::Fixed(tuple) => tuple.0.len(),
            Tuple::Variable(tuple) => tuple.fixed_elements.len(),
        }
    }

    pub(in crate::types) fn clone_requested_bytes(&self) -> Option<usize> {
        Self::fixed_clone_requested_bytes(self.storage_len())
    }

    pub(in crate::types) fn fixed_clone_requested_bytes(length: usize) -> Option<usize> {
        length.checked_mul(size_of::<Type<'_>>())
    }

    pub(in crate::types) fn fixed_retirement_work(length: usize) -> Option<usize> {
        length.checked_add(2)
    }

    pub(in crate::types) fn retirement_work(&self) -> Option<usize> {
        // Elements contain only inline data and interned handles; dropping a tuple does not
        // recursively dispose of the semantic values referenced by those handles.
        match self {
            Tuple::Fixed(tuple) => Self::fixed_retirement_work(tuple.0.len()),
            Tuple::Variable(tuple) => tuple.fixed_elements.len().checked_add(4),
        }
    }

    pub(in crate::types) fn field_work(&self) -> Option<usize> {
        self.field_work_with(&mut || Ok(())).ok()
    }

    fn field_work_with(
        &self,
        admit: &mut impl FnMut() -> Result<(), QuoteError>,
    ) -> Result<usize, QuoteError> {
        admit()?;
        let (elements, mut work) = match self {
            Tuple::Fixed(tuple) => (tuple.all_elements(), 2usize),
            Tuple::Variable(tuple) => {
                let segment_work = match tuple.variable_segment {
                    VariableSegment::Homogeneous(element) => 1usize
                        .checked_add(element.inline_payload_bytes())
                        .ok_or(QuoteError::Overflow)?,
                    VariableSegment::TypeVarTuple(_) => 1,
                };
                (
                    &*tuple.fixed_elements,
                    4usize
                        .checked_add(segment_work)
                        .ok_or(QuoteError::Overflow)?,
                )
            }
        };
        // Hash/Eq inspect the stored Type payloads, but never follow interned semantic handles.
        for element in elements {
            admit()?;
            work = work
                .checked_add(1)
                .and_then(|work| work.checked_add(element.inline_payload_bytes()))
                .ok_or(QuoteError::Overflow)?;
        }
        Ok(work)
    }
}

pub(in crate::types) type TupleMemoSchema<'db> = (
    salsa::execution_probe::PassiveMemo<
        'db,
        crate::types::tuple::TupleType<'static>,
        crate::types::tuple::ToClassTypeConfiguration,
        salsa::execution_probe::FixedQueryKeyProfile,
    >,
);

pub(in crate::types) fn register_tuple_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::tuple::TupleType<'static>,
        crate::types::tuple::runtime::TupleMemoSchema<'db>,
    >,
> {
    let owner = TupleType::ingredient(db.zalsa());
    let class = registry.passive_memo::<_, _, CopyMemoProfile>(
        owner,
        to_class_type::fn_ingredient_(db, db.zalsa()),
    )?;
    registry.finite_interned_values_with_memos(owner, (class,))
}
