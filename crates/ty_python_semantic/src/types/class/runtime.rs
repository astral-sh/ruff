//! Passive profiles for canonical class identities and every attached memo slot.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{
    FixedQueryKeyProfile as CopyMemoProfile, InternedValues, PassiveMemoGroup, PassiveMemoProfile,
    RegistryBuilder, RunResult,
};
use salsa::plumbing::function::{Configuration, IngredientImpl};
use salsa::plumbing::interned::FiniteInternedConfiguration;
use salsa::plumbing::{QuoteError, QuoteFuel};

use super::slots::{self, InstanceLayout, SlotDefinition};
use super::static_literal::{self, StaticClassLiteral};
use super::{
    ClassMetaclass, GenericAlias, MetaclassError, code_generator_of_static_class,
    nearest_disjoint_base, try_mro,
};
use crate::types::abstract_methods::{self, AbstractMethod};
use crate::types::dedicated::pydantic;
use crate::types::enums::{self, EnumMetadata};
use crate::types::mro::source::source_alias_mro_ingredient;
use crate::types::mro::{Mro, StaticMroError};
use crate::types::typed_dict::{self, TypedDictSchema};
use crate::types::{
    ClassLiteral, MetaclassTransformInfo, Type, class_mro_literals, protocol_class,
};
use crate::{Db, FxIndexMap};

macro_rules! class_memo_schema {
    (
        $schema_vis:vis type $schema:ident<$db:lifetime> = $owner:ty;
        $register_vis:vis fn $register:ident;
        $(($query:ident, $profile:ty)),* $(,)?
    ) => {
        $schema_vis type $schema<$db> =
            crate::types::class::runtime::class_memo_schema!(@type $db, $owner; $(($query, $profile)),*);

        $register_vis fn $register<'run, $db: 'run>(
            db: &$db dyn crate::Db,
            registry: &mut salsa::execution_probe::RegistryBuilder<'run, $db>,
        ) -> salsa::execution_probe::RunResult<$schema<$db>> {
            let owner = <$owner>::ingredient(db.zalsa());
            Ok(crate::types::class::runtime::class_memo_schema!(registry, owner, db; $(($query, $profile)),*))
        }
    };
    (@type $db:lifetime, $owner:ty;) => { () };
    (@type $db:lifetime, $owner:ty; ($query:ident, $profile:ty) $(, $rest:tt)*) => {
        salsa::execution_probe::PassiveMemoGroup<
            (salsa::execution_probe::PassiveMemo<$db, $owner, <$query as salsa::plumbing::TrackedFunctionConfiguration>::Configuration, $profile>,),
            crate::types::class::runtime::class_memo_schema!(@type $db, $owner; $($rest),*)
        >
    };
    ($registry:ident, $owner:ident, $db:ident;) => { () };
    ($registry:ident, $owner:ident, $db:ident; ($query:ident, $profile:ty) $(, $rest:tt)* $(,)?) => {
        salsa::execution_probe::PassiveMemoGroup::new(
            ($registry.passive_memo::<_, _, $profile>(
                $owner, $query::fn_ingredient_($db, $db.zalsa()),
            )?,),
            crate::types::class::runtime::class_memo_schema!($registry, $owner, $db; $($rest),*)
        )
    };
}
pub(in crate::types) use class_memo_schema;

impl FiniteInternedConfiguration for StaticClassLiteral<'static> {
    fn field_work(fields: &Self::Fields<'_>) -> Option<usize> {
        // Hashing and comparing the name visits its bytes. All other fields are scalars or
        // interned handles and do not inspect the semantic data referenced by those handles.
        12usize.checked_add(fields.0.len())
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

impl FiniteInternedConfiguration for GenericAlias<'static> {
    fn field_work(_: &Self::Fields<'_>) -> Option<usize> {
        Some(2)
    }

    fn field_work_bounded(
        fields: &Self::Fields<'_>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        Self::field_work(fields).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) type GenericAliasValues<'db, M> =
    InternedValues<'db, GenericAlias<'static>, M>;

pub(in crate::types) type GenericAliasMemoSchema<'db> = salsa::execution_probe::PassiveMemoGroup<
    salsa::execution_probe::PassiveMemoGroup<
        salsa::execution_probe::PassiveMemoGroup<
            (
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::GenericAlias<'static>,
                    crate::types::class::TryMroConfiguration,
                    crate::types::class::runtime::MroProfile,
                >,
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::GenericAlias<'static>,
                    crate::types::mro::source::SourceAliasMroConfiguration,
                    crate::types::class::runtime::MroProfile,
                >,
            ),
            (
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::GenericAlias<'static>,
                    crate::types::class::NearestDisjointBaseConfiguration,
                    salsa::execution_probe::FixedQueryKeyProfile,
                >,
            ),
        >,
        crate::types::abstract_methods::GenericAliasMemoSchema<'db>,
    >,
    salsa::execution_probe::PassiveMemoGroup<
        crate::types::protocol_class::GenericAliasMemoSchema<'db>,
        crate::types::typed_dict::GenericAliasMemoSchema<'db>,
    >,
>;

pub(in crate::types) fn register_generic_alias_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<GenericAliasValues<'db, crate::types::class::runtime::GenericAliasMemoSchema<'db>>> {
    let owner = GenericAlias::ingredient(db.zalsa());
    let mro = registry
        .passive_memo::<_, _, MroProfile>(owner, try_mro::fn_ingredient_(db, db.zalsa()))?;
    let source_mro =
        registry.passive_memo::<_, _, MroProfile>(owner, source_alias_mro_ingredient(db))?;
    let nearest = registry.passive_memo::<_, _, CopyMemoProfile>(
        owner,
        nearest_disjoint_base::fn_ingredient_(db, db.zalsa()),
    )?;
    let schema = PassiveMemoGroup::new((mro, source_mro), (nearest,));
    let schema = PassiveMemoGroup::new(
        schema,
        abstract_methods::register_generic_alias_memos(db, registry)?,
    );
    let schema = PassiveMemoGroup::new(
        schema,
        PassiveMemoGroup::new(
            protocol_class::register_generic_alias_memos(db, registry)?,
            typed_dict::register_generic_alias_memos(db, registry)?,
        ),
    );
    registry.finite_interned_values_with_memos(owner, schema)
}

pub(in crate::types) struct TypeSliceProfile;

impl<C> PassiveMemoProfile<C> for TypeSliceProfile
where
    C: for<'db> Configuration<Output<'db> = Box<[Type<'db>]>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        1usize.checked_add(output.len())
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) fn class_mro_literals_ingredient(
    db: &dyn Db,
) -> &IngredientImpl<crate::types::ClassMroLiteralsConfiguration> {
    class_mro_literals::fn_ingredient_(db, db.zalsa())
}

pub(in crate::types) struct ClassSliceProfile;

impl<C> PassiveMemoProfile<C> for ClassSliceProfile
where
    C: for<'db> Configuration<Output<'db> = Box<[ClassLiteral<'db>]>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        1usize.checked_add(output.len())
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct MroProfile;

impl<C> PassiveMemoProfile<C> for MroProfile
where
    C: for<'db> Configuration<Output<'db> = Result<Mro<'db>, Box<StaticMroError<'db>>>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        match output {
            Ok(mro) => 2usize.checked_add(mro.len()),
            Err(error) => error.retirement_work()?.checked_add(1),
        }
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct MetaclassProfile;

impl<C> PassiveMemoProfile<C> for MetaclassProfile
where
    C: for<'db> Configuration<
        Output<'db> = Result<
            (ClassMetaclass<'db>, Option<MetaclassTransformInfo<'db>>),
            MetaclassError<'db>,
        >,
    >,
{
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        // Both the success and error variants contain only scalars and interned handles.
        (!std::mem::needs_drop::<C::Output<'db>>()).then_some(0)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Unsupported)
    }
}

pub(in crate::types) struct SlotDefinitionProfile;

impl<C> PassiveMemoProfile<C> for SlotDefinitionProfile
where
    C: for<'db> Configuration<Output<'db> = SlotDefinition>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        match output {
            SlotDefinition::Names(names) => 2usize.checked_add(names.len()),
            SlotDefinition::NonEmpty | SlotDefinition::DynamicOrNone => Some(1),
        }
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct InstanceLayoutProfile;

impl<C> PassiveMemoProfile<C> for InstanceLayoutProfile
where
    C: for<'db> Configuration<Output<'db> = InstanceLayout>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output.retirement_work()
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct EnumMetadataProfile;

impl<C> PassiveMemoProfile<C> for EnumMetadataProfile
where
    C: for<'db> Configuration<Output<'db> = Option<EnumMetadata<'db>>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        output
            .as_ref()
            .map_or(Some(1), EnumMetadata::retirement_work)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct TypedDictSchemaProfile;

impl<C> PassiveMemoProfile<C> for TypedDictSchemaProfile
where
    C: for<'db> Configuration<Output<'db> = TypedDictSchema<'db>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        // Each entry owns one fixed-size name handle and a field containing scalar data.
        // The per-entry quote also covers the B-tree nodes retaining those entries.
        1usize.checked_add(output.len().checked_mul(5)?)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) struct AbstractMethodsProfile;

impl<C> PassiveMemoProfile<C> for AbstractMethodsProfile
where
    C: for<'db> Configuration<Output<'db> = FxIndexMap<Name, AbstractMethod<'db>>>,
{
    fn retired_output_work<'db>(output: &C::Output<'db>) -> Option<usize> {
        // The map owns name handles and abstract-method records containing scalar data.
        if std::mem::needs_drop::<AbstractMethod<'db>>() {
            return None;
        }
        1usize.checked_add(output.len().checked_mul(4)?)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        if std::mem::needs_drop::<AbstractMethod<'db>>() {
            return Err(QuoteError::Unsupported);
        }
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) type BaseClassMemoSchema<'db> = salsa::execution_probe::PassiveMemoGroup<
    salsa::execution_probe::PassiveMemoGroup<
        salsa::execution_probe::PassiveMemoGroup<
            salsa::execution_probe::PassiveMemoGroup<
                crate::types::class::static_literal::ClassMemoSchema<'db>,
                crate::types::class::slots::ClassMemoSchema<'db>,
            >,
            salsa::execution_probe::PassiveMemoGroup<
                salsa::execution_probe::PassiveMemoGroup<
                    crate::types::dedicated::pydantic::ClassMemoSchema<'db>,
                    crate::types::protocol_class::ClassMemoSchema<'db>,
                >,
                salsa::execution_probe::PassiveMemoGroup<
                    crate::types::enums::ClassMemoSchema<'db>,
                    crate::types::typed_dict::ClassMemoSchema<'db>,
                >,
            >,
        >,
        salsa::execution_probe::PassiveMemoGroup<
            (
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::StaticClassLiteral<'static>,
                    crate::types::class::CodeGeneratorOfStaticClassConfiguration,
                    salsa::execution_probe::FixedQueryKeyProfile,
                >,
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::StaticClassLiteral<'static>,
                    crate::types::class::NearestDisjointBaseConfiguration,
                    salsa::execution_probe::FixedQueryKeyProfile,
                >,
            ),
            (
                salsa::execution_probe::PassiveMemo<
                    'db,
                    crate::types::StaticClassLiteral<'static>,
                    crate::types::ClassMroLiteralsConfiguration,
                    crate::types::class::runtime::ClassSliceProfile,
                >,
            ),
        >,
    >,
    crate::types::abstract_methods::ClassMemoSchema<'db>,
>;

#[cfg(not(test))]
pub(in crate::types) type ClassMemoSchema<'db> =
    crate::types::class::runtime::BaseClassMemoSchema<'db>;

#[cfg(test)]
pub(in crate::types) type ClassMemoSchema<'db> = salsa::execution_probe::PassiveMemoGroup<
    crate::types::class::runtime::BaseClassMemoSchema<'db>,
    crate::types::class::instance_flags::OriginalClassMemoSchema<'db>,
>;

pub(in crate::types) fn register_class_values<'run, 'db: 'run>(
    db: &'db dyn Db,
    registry: &mut RegistryBuilder<'run, 'db>,
) -> RunResult<
    salsa::execution_probe::InternedValues<
        'db,
        crate::types::StaticClassLiteral<'static>,
        crate::types::class::runtime::ClassMemoSchema<'db>,
    >,
> {
    let owner = StaticClassLiteral::ingredient(db.zalsa());
    macro_rules! memo {
        ($query:path, $profile:ty) => {{
            use $query as query;
            registry.passive_memo::<_, _, $profile>(owner, query::fn_ingredient_(db, db.zalsa()))?
        }};
    }

    let schema = PassiveMemoGroup::new(
        PassiveMemoGroup::new(
            static_literal::register_class_memos(db, registry)?,
            slots::register_class_memos(db, registry)?,
        ),
        PassiveMemoGroup::new(
            PassiveMemoGroup::new(
                pydantic::register_class_memos(db, registry)?,
                protocol_class::register_class_memos(db, registry)?,
            ),
            PassiveMemoGroup::new(
                enums::register_class_memos(db, registry)?,
                typed_dict::register_class_memos(db, registry)?,
            ),
        ),
    );
    let schema = PassiveMemoGroup::new(
        schema,
        PassiveMemoGroup::new(
            (
                memo!(code_generator_of_static_class, CopyMemoProfile),
                memo!(nearest_disjoint_base, CopyMemoProfile),
            ),
            (memo!(class_mro_literals, ClassSliceProfile),),
        ),
    );
    let schema = PassiveMemoGroup::new(
        schema,
        abstract_methods::register_class_memos(db, registry)?,
    );
    #[cfg(test)]
    let schema = PassiveMemoGroup::new(
        schema,
        super::instance_flags::register_original_class_memo(db, registry)?,
    );
    registry.finite_interned_values_with_memos(owner, schema)
}

#[cfg(test)]
mod tests {
    use salsa::attempt_probe::AttemptOutcome;
    use salsa::execution_probe::{ExecutionLimits, try_with_execution_budget};

    use super::*;
    use crate::db::tests::setup_db;

    #[test]
    fn complete_class_schema_registers_without_executing_queries() {
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
                let _values = register_class_values(&db, &mut registry)?;
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
    fn complete_generic_alias_schema_registers_without_executing_queries() {
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
                let _values = register_generic_alias_values(&db, &mut registry)?;
                registry.seal()?.run(|_| async { Ok(()) })
            },
        );
        assert!(
            matches!(outcome, Ok(AttemptOutcome::Complete(Ok(())))),
            "{outcome:?}"
        );
        assert!(
            db.take_salsa_events()
                .iter()
                .all(|event| !matches!(event.kind, salsa::EventKind::WillExecute { .. }))
        );
    }
}
