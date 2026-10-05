//! Passive payload profiles for canonical function identities, dataclass-transform metadata,
//! and their attached memo slots.

use salsa::execution_probe::{
    FixedQueryKeyProfile as CopyMemoProfile, InternedValues, PassiveMemoGroup, PassiveMemoProfile,
    RegistryBuilder, RunResult,
};
use salsa::plumbing::function::Configuration;
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::{
    DataclassTransformerParams, FunctionType, OverloadLiteral, function_last_definition_signature,
    function_literal_signature, implementation_body_kind, overloads_and_implementation_inner,
};
use crate::Db;
use crate::types::call::bind::{CallableDescription, callable_description_ingredient};
use crate::types::dedicated::pytest::test_function_definition_ingredient;
use crate::types::signatures::{CallableSignature, Signature};

impl FiniteInternedConfiguration for OverloadLiteral<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Only the name has variable-sized inline comparison/hash work. Other fields are
        // scalars or handles; none of their referenced semantic data is visited.
        7usize.checked_add(fields.0.len())
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

impl FiniteInternedConfiguration for FunctionType<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        Self::field_work_with(fields, &mut || Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        Self::field_work_with(fields, &mut || fuel.consume(1))
    }
}

impl FunctionType<'static> {
    fn field_work_with(
        fields: &<Self as salsa::plumbing::interned::Configuration>::Fields<'_>,
        admit: &mut impl FnMut() -> Result<(), QuoteError>,
    ) -> Result<usize, QuoteError> {
        admit()?;
        let mut work = 3usize;
        if let Some(updated) = &fields.1 {
            work = work.checked_add(3).ok_or(QuoteError::Overflow)?;
            if let Some(signature) = &updated.signature {
                work = work
                    .checked_add(signature.field_work_with(admit)?)
                    .ok_or(QuoteError::Overflow)?;
            }
            if let Some(callables) = &updated.implementation_callables {
                work = work
                    .checked_add(1)
                    .and_then(|work| work.checked_add(callables.len()))
                    .ok_or(QuoteError::Overflow)?;
            }
        }
        Ok(work)
    }
}

impl FiniteInternedConfiguration for DataclassTransformerParams<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        Self::field_work_with(fields, &mut || Ok(())).ok()
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        Self::field_work_with(fields, &mut || fuel.consume(1))
    }
}

impl DataclassTransformerParams<'static> {
    /// Counts flag and slice bookkeeping, then each stored type's inline hash/equality work.
    /// The admission callback funds metadata inspection before the root and each entry are read.
    fn field_work_with(
        fields: &<Self as salsa::plumbing::interned::Configuration>::Fields<'_>,
        admit: &mut impl FnMut() -> Result<(), QuoteError>,
    ) -> Result<usize, QuoteError> {
        admit()?;
        let mut work = 3usize;
        for field_specifier in &fields.1 {
            admit()?;
            // Stored Type values compare and drop without following their semantic handles.
            work = work
                .checked_add(1)
                .and_then(|work| work.checked_add(field_specifier.inline_payload_bytes()))
                .ok_or(QuoteError::Overflow)?;
        }
        Ok(work)
    }
}

/// Registers canonical dataclass-transform metadata, which has no attached query memos.
pub(in crate::types) fn register_dataclass_transformer_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<InternedValues<'db, DataclassTransformerParams<'static>, ()>> {
    registry.finite_interned_values_with_memos(
        DataclassTransformerParams::ingredient(db.zalsa()),
        (),
    )
}

pub(in crate::types) struct CallableSignatureProfile;

impl<C> PassiveMemoProfile<C> for CallableSignatureProfile
where
    C: for<'db> Configuration<Output<'db> = CallableSignature<'db>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output.retirement_work()
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        output.retirement_work_bounded(fuel)
    }
}

pub(in crate::types) struct SignatureProfile;

impl<C> PassiveMemoProfile<C> for SignatureProfile
where
    C: for<'db> Configuration<Output<'db> = Signature<'db>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output.retirement_work()
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        output.retirement_work_bounded(fuel)
    }
}

pub(in crate::types) struct OverloadsProfile;

impl<C> PassiveMemoProfile<C> for OverloadsProfile
where
    C: for<'db> Configuration<
        Output<'db> = (Box<[OverloadLiteral<'db>]>, Option<OverloadLiteral<'db>>),
    >,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        3usize.checked_add(output.0.len())
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct CallableDescriptionProfile;

impl<C> PassiveMemoProfile<C> for CallableDescriptionProfile
where
    C: for<'db> Configuration<Output<'db> = CallableDescription<'static>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        2usize.checked_add(output.name().len())
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) type FunctionMemoSchema<'db> = salsa::execution_probe::PassiveMemoGroup<
    (
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::FunctionType<'static>,
            crate::types::function::FunctionLiteralSignatureConfiguration,
            crate::types::function::runtime::CallableSignatureProfile,
        >,
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::FunctionType<'static>,
            crate::types::function::FunctionLastDefinitionSignatureConfiguration,
            crate::types::function::runtime::SignatureProfile,
        >,
    ),
    (
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::FunctionType<'static>,
            crate::types::dedicated::pytest::TestFunctionDefinitionConfiguration,
            salsa::execution_probe::FixedQueryKeyProfile,
        >,
    ),
>;

pub(in crate::types) type OverloadMemoSchema<'db> = salsa::execution_probe::PassiveMemoGroup<
    (
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::function::OverloadLiteral<'static>,
            crate::types::function::OverloadsAndImplementationInnerConfiguration,
            crate::types::function::runtime::OverloadsProfile,
        >,
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::function::OverloadLiteral<'static>,
            crate::types::function::ImplementationBodyKindConfiguration,
            salsa::execution_probe::FixedQueryKeyProfile,
        >,
    ),
    (
        salsa::execution_probe::PassiveMemo<
            'db,
            crate::types::function::OverloadLiteral<'static>,
            crate::types::call::bind::CallableDescriptionFromOverloadConfiguration,
            crate::types::function::runtime::CallableDescriptionProfile,
        >,
    ),
>;

pub(in crate::types) fn register_function_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<(
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::FunctionType<'static>,
        crate::types::function::runtime::FunctionMemoSchema<'db>,
    >,
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::function::OverloadLiteral<'static>,
        crate::types::function::runtime::OverloadMemoSchema<'db>,
    >,
)> {
    let owner = FunctionType::ingredient(db.zalsa());
    let signature = registry.passive_memo::<_, _, CallableSignatureProfile>(
        owner,
        function_literal_signature::fn_ingredient_(db, db.zalsa()),
    )?;
    let last_signature = registry.passive_memo::<_, _, SignatureProfile>(
        owner,
        function_last_definition_signature::fn_ingredient_(db, db.zalsa()),
    )?;
    let definition = registry
        .passive_memo::<_, _, CopyMemoProfile>(owner, test_function_definition_ingredient(db))?;
    let functions = registry.finite_interned_values_with_memos(
        owner,
        PassiveMemoGroup::new((signature, last_signature), (definition,)),
    )?;

    let owner = OverloadLiteral::ingredient(db.zalsa());
    let overloads = registry.passive_memo::<_, _, OverloadsProfile>(
        owner,
        overloads_and_implementation_inner::fn_ingredient_(db, db.zalsa()),
    )?;
    let body_kind = registry.passive_memo::<_, _, CopyMemoProfile>(
        owner,
        implementation_body_kind::fn_ingredient_(db, db.zalsa()),
    )?;
    let description = registry.passive_memo::<_, _, CallableDescriptionProfile>(
        owner,
        callable_description_ingredient(db),
    )?;
    let overloads = registry.finite_interned_values_with_memos(
        owner,
        PassiveMemoGroup::new((overloads, body_kind), (description,)),
    )?;
    Ok((functions, overloads))
}

#[cfg(test)]
mod tests {
    use ruff_db::files::system_path_to_file;
    use ruff_db::parsed::parsed_module;
    use ruff_db::system::DbWithWritableSystem;
    use ruff_python_ast::{Stmt, name::Name};
    use salsa::attempt_probe::AttemptOutcome;
    use salsa::execution_probe::{ExecutionLimits, try_with_execution_budget};
    use ty_python_core::semantic_index;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::callable::CallableTypeKind;
    use crate::types::constraints::OwnedConstraintSet;
    use crate::types::function::{DataclassTransformerFlags, UpdatedFunctionSignatures};
    use crate::types::signatures::{Parameter, Parameters};
    use crate::types::{CallableType, Type, binding_type, todo_type};

    #[test]
    fn dataclass_transformer_field_work_counts_inline_payloads() {
        // The shared quotation routine charges root/entry work separately from inline payload
        // bytes, and a fully admitted pass returns the same total as the unbounded profile.
        let fields = (
            DataclassTransformerFlags::EQ_DEFAULT,
            Box::from([Type::bool_literal(true), todo_type!("field payload")]),
        );
        let expected = 5 + if cfg!(debug_assertions) {
            "field payload".len()
        } else {
            0
        };
        let mut remaining = 3usize;
        let bounded = DataclassTransformerParams::field_work_with(&fields, &mut || {
            remaining = remaining.checked_sub(1).ok_or(QuoteError::Exhausted)?;
            Ok(())
        });
        assert_eq!(
            DataclassTransformerParams::field_work(&fields),
            Some(expected)
        );
        assert_eq!(bounded, Ok(expected));
        assert_eq!(remaining, 0);
    }

    #[test]
    fn dataclass_transformer_field_work_stops_before_unpaid_visit() {
        // A refused admission stops the common metadata walk at the root or the next entry;
        // no later admission is attempted after the callback reports exhaustion.
        let fields = (
            DataclassTransformerFlags::ORDER_DEFAULT,
            Box::from([todo_type!("first field"), todo_type!("second field")]),
        );
        let mut root_attempts = 0usize;
        let root_refusal = DataclassTransformerParams::field_work_with(&fields, &mut || {
            root_attempts += 1;
            Err(QuoteError::Exhausted)
        });
        assert_eq!(root_refusal, Err(QuoteError::Exhausted));
        assert_eq!(root_attempts, 1);

        let mut entry_attempts = 0usize;
        let mut remaining = 2usize;
        let entry_refusal = DataclassTransformerParams::field_work_with(&fields, &mut || {
            entry_attempts += 1;
            remaining = remaining.checked_sub(1).ok_or(QuoteError::Exhausted)?;
            Ok(())
        });
        assert_eq!(entry_refusal, Err(QuoteError::Exhausted));
        assert_eq!(entry_attempts, 3);
        assert_eq!(remaining, 0);
    }

    #[test]
    fn complete_function_schemas_register_without_executing_queries() {
        let mut db = setup_db();
        db.clear_salsa_events();
        let outcome = try_with_execution_budget(
            &db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 1_000_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let _values = register_function_values(&db, &mut registry)?;
                registry.seal()?.run(|_| async { Ok(()) })
            },
        );
        assert!(matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))));
        assert!(
            db.take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
    }

    #[test]
    fn modified_function_payloads_use_the_original_interner() -> anyhow::Result<()> {
        let mut db = setup_db();
        db.write_file("src/main.py", "def function(): ...\n")?;
        let file = system_path_to_file(&db, "src/main.py")?;
        let program_file = db.program_file(file);
        let module = parsed_module(&db, program_file.python_file(&db)).load(&db);
        let Stmt::FunctionDef(node) = &module.syntax().body[0] else {
            anyhow::bail!("expected a function definition");
        };
        let definition = semantic_index(&db, program_file).expect_single_definition(node);
        let Some(function) = binding_type(&db, definition).as_function_literal() else {
            anyhow::bail!("expected a function literal");
        };
        let signature = Signature::new(
            Parameters::standard([Parameter::positional_or_keyword(Name::new("parameter"))
                .with_default_type(Type::bool_literal(true))]),
            Type::bool_literal(false),
        )
        .with_source_overload_index(Some(2))
        .with_probe_receiver_constraints(OwnedConstraintSet::always());
        let signature = CallableSignature::single(signature);
        let callable = CallableType::new(&db, signature.clone(), CallableTypeKind::FunctionLike);
        let fields = (
            function.literal(&db),
            UpdatedFunctionSignatures::new(
                Some(signature),
                Some(vec![callable].into_boxed_slice()),
            ),
            Some(CallableTypeKind::FunctionLike),
        );
        assert!(FunctionType::field_work(&fields).is_some_and(|work| work > 3));
        let expected_fields = fields.clone();
        let outcome = try_with_execution_budget(
            &db,
            ExecutionLimits {
                semantic_work: 100_000,
                requested_bytes: 1_000_000,
            },
            |budget| {
                let mut registry = RegistryBuilder::with_budget(&db, &budget)?;
                let (functions, _overloads) = register_function_values(&db, &mut registry)?;
                registry.seal()?.run(|endpoint| async move {
                    Ok(endpoint.intern_value(&functions, fields).await)
                })
            },
        );
        let Ok(AttemptOutcome::Complete(Ok(actual))) = outcome else {
            anyhow::bail!("modified function interning did not complete: {outcome:?}");
        };
        let expected = FunctionType::new_internal(
            &db,
            expected_fields.0,
            expected_fields.1,
            expected_fields.2,
        );
        assert_eq!(actual, expected);
        Ok(())
    }
}
