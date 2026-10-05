//! Prepared static MRO construction and C3 execution under their active semantic owner.

use std::collections::VecDeque;
use ty_python_core::scope::ScopeId;

use super::mapping::PreparedMroMappingEffects;
use super::member_lookup::{LookupFailure, LookupOperation};
use super::source::declarations::PreparedDeclarations;
use super::{Boundary, Router};
use crate::types::class_base::ClassBase;
use crate::types::generics::Specialization;
use crate::types::mro::base::{
    BaseMroEffects, BaseMroFacts, BaseMroStart, BaseMroWork, collect_base_mro_with,
    collect_single_base_mro_with, sealed as base_sealed,
};
use crate::types::mro::c3::{C3Effects, C3Work, c3_merge_with, sealed as c3_sealed};
use crate::types::mro::construction::{
    StaticMroEffects, StaticMroFacts, StaticMroWork, sealed, static_mro_with,
};
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::root::{
    MroRootEffects, MroRootFacts, MroRootWork, apply_optional_class_specialization_with,
    sealed as root_sealed,
};
use crate::types::mro::{Mro, StaticMroError, StaticMroErrorKind};
use crate::types::{
    ApplyTypeMappingVisitor, ClassType, GenericAlias, GenericContext, StaticClassLiteral, Type,
};
use crate::{Db, ProgramEnvironment};

mod cursor;
pub(super) mod task;

use cursor::PreparedBaseMroCursor;
pub(super) use cursor::PreparedMroCursor;
pub(in crate::types::callable::scheduled_probe) use task::{
    MroNodeId, PreparedMroWork, StaticMroOutcome, StaticMroRequest, StaticMroResultId,
};

/// Copies borrowed input only after admitting its storage and eventual cleanup.
pub(super) async fn merge_mro_sequences<'db>(
    db: &'db dyn Db,
    router: &Router<'db, '_>,
    sequences: &[VecDeque<ClassBase<'db>>],
) -> Result<Option<Mro<'db>>, LookupFailure<'db>> {
    let work = PreparedMroWork::consumer(router);
    work.checkpoint(c3_input_work_units(C3InputWork::OuterCapacity {
        len: sequences.len(),
    })?)
    .await?;
    let mut owned = Vec::with_capacity(sequences.len());
    let mut sequences = sequences.iter();
    loop {
        work.checkpoint(c3_input_work_units(C3InputWork::SequenceAdvance)?)
            .await?;
        let Some(sequence) = sequences.next() else {
            break;
        };
        work.checkpoint(c3_input_work_units(C3InputWork::CopySequence {
            len: sequence.len(),
        })?)
        .await?;
        let copied = sequence.iter().copied().collect();
        work.checkpoint(c3_input_work_units(C3InputWork::AppendSequence)?)
            .await?;
        owned.push(copied);
    }
    c3_merge_with(
        crate::types::mro::field_reads::MroFieldReads::new(db),
        owned,
        &PreparedC3Effects { work: &work },
    )
    .await
}

#[derive(Clone, Copy)]
enum C3InputWork {
    OuterCapacity { len: usize },
    SequenceAdvance,
    CopySequence { len: usize },
    AppendSequence,
}

fn mro_storage_units(len: usize) -> Result<usize, Boundary> {
    len.checked_mul(16)
        .and_then(|units| units.checked_add(8))
        .ok_or(Boundary::CostOverflow)
}

// Each caller owns its output buffer across the checkpoint, so spare capacity remains available
// until the admitted push.
fn mro_append_units(prefix_len: usize, capacity: usize) -> Result<usize, Boundary> {
    if prefix_len < capacity {
        Ok(8)
    } else {
        mro_storage_units(prefix_len.checked_add(1).ok_or(Boundary::CostOverflow)?)
    }
}

fn c3_input_work_units(work: C3InputWork) -> Result<usize, Boundary> {
    match work {
        // Queue ownership includes the header visit and allocation release on early return.
        C3InputWork::OuterCapacity { len } | C3InputWork::CopySequence { len } => {
            mro_storage_units(len)
        }
        C3InputWork::SequenceAdvance | C3InputWork::AppendSequence => Ok(8),
    }
}

pub(super) fn c3_work_units(work: C3Work) -> Result<usize, Boundary> {
    match work {
        C3Work::OutputCapacity { entries } => mro_storage_units(entries),
        C3Work::RetainSequences { len } | C3Work::BoxOutput { len, .. } => mro_storage_units(len),
        C3Work::OutputAppend {
            prefix_len,
            capacity,
        } => mro_append_units(prefix_len, capacity),
        C3Work::IdentityComparison { todo_bytes } | C3Work::RemoveHead { todo_bytes } => {
            todo_bytes.checked_add(8).ok_or(Boundary::CostOverflow)
        }
        C3Work::CandidateAdvance
        | C3Work::TailSequenceAdvance
        | C3Work::TailEntryAdvance
        | C3Work::SelectedIdentity
        | C3Work::RemovalSequenceAdvance
        | C3Work::Publish => Ok(8),
    }
}

/// The first entry and the proper tail use the same owner and monotonically increasing work IDs.
pub(super) struct PreparedMroRootEffects<'work, 'eval, 'db, 'c> {
    db: &'db dyn Db,
    work: &'work PreparedMroWork<'eval, 'db, 'c>,
}

impl<'work, 'eval, 'db, 'c> PreparedMroRootEffects<'work, 'eval, 'db, 'c> {
    pub(super) fn new(db: &'db dyn Db, work: &'work PreparedMroWork<'eval, 'db, 'c>) -> Self {
        Self { db, work }
    }

    pub(super) fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, LookupFailure<'db>> {
        if specialization.tuple(self.db).is_some() {
            return Err(LookupFailure::Unsupported(
                LookupOperation::TupleRuntimeSpecialization,
            ));
        }
        Ok(specialization.tuple_runtime_element_specialization(self.db))
    }
}

impl root_sealed::Sealed for PreparedMroRootEffects<'_, '_, '_, '_> {}

impl<'db> MroRootFacts<'db> for PreparedMroRootEffects<'_, '_, 'db, '_> {
    type Error = LookupFailure<'db>;
}

impl<'db> MroRootEffects<'db> for PreparedMroRootEffects<'_, '_, 'db, '_> {
    async fn generic_alias(
        &self,
        class: crate::types::StaticClassLiteral<'db>,
        specialization: crate::types::generics::Specialization<'db>,
    ) -> Result<crate::types::ClassType<'db>, Self::Error> {
        Ok(crate::types::ClassType::Generic(
            crate::types::GenericAlias::new(self.db, class, specialization),
        ))
    }

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<GenericContext<'db>>, Self::Error> {
        self.work
            .router()
            .declarations
            .as_deref()
            .ok_or(Boundary::SourcePreparation)?
            .context(class)
            .map_err(Into::into)
    }

    async fn checkpoint(&self, work: MroRootWork) -> Result<(), Self::Error> {
        let units = match work {
            MroRootWork::GenericContext
            | MroRootWork::DefaultSpecialization
            | MroRootWork::TupleRuntimeSpecialization => 8,
            MroRootWork::GenericAlias => 16,
        };
        self.work.checkpoint(units).await.map_err(Into::into)
    }

    async fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        if self.generic_context(class).await?.is_none() {
            return Ok(ClassType::NonGeneric(class.into()));
        }
        Err(LookupFailure::Unsupported(
            LookupOperation::DefaultSpecialization,
        ))
    }

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        PreparedMroRootEffects::tuple_runtime_specialization(self, specialization)
    }
}

#[derive(Clone, Copy)]
enum CollectionWork {
    Append { prefix_len: usize, capacity: usize },
    Box { len: usize },
    Publish,
}

fn collection_work_units(work: CollectionWork) -> Result<usize, Boundary> {
    match work {
        CollectionWork::Append {
            prefix_len,
            capacity,
        } => mro_append_units(prefix_len, capacity),
        CollectionWork::Box { len } => mro_storage_units(len),
        CollectionWork::Publish => Ok(8),
    }
}

pub(super) struct PreparedStaticMroEffects<'work, 'eval, 'db, 'c> {
    db: &'db dyn Db,
    env: &'work ProgramEnvironment<'db>,
    work: &'work PreparedMroWork<'eval, 'db, 'c>,
}

impl<'work, 'eval, 'db, 'c> PreparedStaticMroEffects<'work, 'eval, 'db, 'c> {
    pub(super) fn new(
        db: &'db dyn Db,
        env: &'work ProgramEnvironment<'db>,
        work: &'work PreparedMroWork<'eval, 'db, 'c>,
    ) -> Self {
        Self { db, env, work }
    }

    fn prepared(&self) -> Result<&PreparedDeclarations<'db>, LookupFailure<'db>> {
        self.work
            .router()
            .declarations
            .as_deref()
            .ok_or_else(|| Boundary::SourcePreparation.into())
    }

    fn validate_environment(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<(), LookupFailure<'db>> {
        if env.program(self.db) != self.env.program(self.db) {
            return Err(Boundary::ProgramDomain.into());
        }
        Ok(())
    }
}

impl base_sealed::Sealed for PreparedStaticMroEffects<'_, '_, '_, '_> {}

impl<'db> BaseMroFacts<'db> for PreparedStaticMroEffects<'_, '_, 'db, '_> {
    type Error = LookupFailure<'db>;
}

impl<'db> BaseMroEffects<'db> for PreparedStaticMroEffects<'_, '_, 'db, '_> {
    async fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(MroFieldReads::new(self.db).alias_origin(alias))
    }

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(MroFieldReads::new(self.db).alias_specialization(alias))
    }

    async fn object_base(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        StaticMroEffects::object_base(self, env).await
    }

    async fn checkpoint(&self, work: BaseMroWork) -> Result<(), Self::Error> {
        let units = match work {
            BaseMroWork::ClassDispatch
            | BaseMroWork::BaseDispatch
            | BaseMroWork::ObjectBase
            | BaseMroWork::CompositionRequest
            | BaseMroWork::CollectionRequest
            | BaseMroWork::SingleCollectionRequest
            | BaseMroWork::Publish => 8,
        };
        self.work.checkpoint(units).await.map_err(Into::into)
    }

    async fn compose_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        let program = self.env.program(self.db);
        if base.generic_context(self.db).program(self.db) != program
            || additional.generic_context(self.db).program(self.db) != program
        {
            return Err(Boundary::ProgramDomain.into());
        }
        let env =
            ProgramEnvironment::from_program(additional.generic_context(self.db).program(self.db));
        let visitor = ApplyTypeMappingVisitor::new(&env);
        base.apply_specialization_with(
            self.db,
            additional,
            &visitor,
            &PreparedMroMappingEffects::new(self.db, self.work),
        )
        .await
        .map_err(Into::into)
    }

    async fn collect_start(
        &self,
        start: BaseMroStart<'db>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error> {
        let mut cursor = PreparedBaseMroCursor::new(start);
        let mut result = VecDeque::new();
        while let Some(entry) = cursor.advance(self.db, self.work).await? {
            self.work
                .checkpoint(collection_work_units(CollectionWork::Append {
                    prefix_len: result.len(),
                    capacity: result.capacity(),
                })?)
                .await?;
            result.push_back(entry);
        }
        self.work
            .checkpoint(collection_work_units(CollectionWork::Publish)?)
            .await?;
        Ok(result)
    }

    async fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: BaseMroStart<'db>,
    ) -> Result<Mro<'db>, Self::Error> {
        let mut cursor = PreparedBaseMroCursor::new(start);
        let mut result = Vec::new();
        self.work
            .checkpoint(collection_work_units(CollectionWork::Append {
                prefix_len: 0,
                capacity: result.capacity(),
            })?)
            .await?;
        result.push(root.into());
        while let Some(entry) = cursor.advance(self.db, self.work).await? {
            self.work
                .checkpoint(collection_work_units(CollectionWork::Append {
                    prefix_len: result.len(),
                    capacity: result.capacity(),
                })?)
                .await?;
            result.push(entry);
        }
        self.work
            .checkpoint(collection_work_units(CollectionWork::Box {
                len: result.len(),
            })?)
            .await?;
        Ok(Mro::from(result))
    }
}

struct PreparedC3Effects<'work, 'eval, 'db, 'c> {
    work: &'work PreparedMroWork<'eval, 'db, 'c>,
}

impl c3_sealed::Sealed for PreparedC3Effects<'_, '_, '_, '_> {}

impl<'db> C3Effects<'db> for PreparedC3Effects<'_, '_, 'db, '_> {
    type Error = LookupFailure<'db>;

    async fn checkpoint(&self, work: C3Work) -> Result<(), Self::Error> {
        self.work
            .checkpoint(c3_work_units(work)?)
            .await
            .map_err(Into::into)
    }

    async fn mro_identity(
        &self,
        fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        Ok(fields.mro_identity(base))
    }
}

pub(super) async fn static_mro<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    router: &Router<'db, '_>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
) -> Result<Result<Mro<'db>, StaticMroError<'db>>, LookupFailure<'db>> {
    let work = PreparedMroWork::consumer(router);
    work.checkpoint(8).await?;
    router.validate_declarations(db, env)?;
    if class.program_file(db).program(db) != env.program(db) {
        return Err(Boundary::ProgramDomain.into());
    }
    evaluate_static_mro(db, &work, class, specialization).await
}

pub(super) async fn evaluate_static_mro<'db>(
    db: &'db dyn Db,
    work: &PreparedMroWork<'_, 'db, '_>,
    class: StaticClassLiteral<'db>,
    specialization: Option<Specialization<'db>>,
) -> StaticMroOutcome<'db> {
    work.checkpoint(16).await?;
    let env = ProgramEnvironment::from_scope(class.body_scope(db));
    work.router().validate_declarations(db, &env)?;
    if specialization.is_some_and(|specialization| {
        specialization.generic_context(db).program(db) != env.program(db)
    }) {
        return Err(Boundary::ProgramDomain.into());
    }
    static_mro_with(
        crate::types::mro::field_reads::MroFieldReads::new(db),
        class,
        specialization,
        &PreparedStaticMroEffects::new(db, &env, work),
    )
    .await
}

impl sealed::Sealed for PreparedStaticMroEffects<'_, '_, '_, '_> {}

impl<'db> StaticMroFacts<'db> for PreparedStaticMroEffects<'_, '_, 'db, '_> {
    type Error = LookupFailure<'db>;
}

pub(super) fn work_units(work: StaticMroWork) -> Result<usize, Boundary> {
    let scaled = |len: usize, scale: usize| {
        len.checked_mul(scale)
            .and_then(|units| units.checked_add(8))
            .ok_or(Boundary::CostOverflow)
    };
    match work {
        StaticMroWork::Begin => Ok(32),
        StaticMroWork::RootRequest => Ok(16),
        StaticMroWork::SequenceStart { bases } => {
            mro_storage_units(bases.checked_add(2).ok_or(Boundary::CostOverflow)?)?
                .checked_add(mro_storage_units(1)?)
                .ok_or(Boundary::CostOverflow)
        }
        StaticMroWork::GenericProtocolScan { len } | StaticMroWork::GenericAliasScan { len } => {
            scaled(len, 8)
        }
        // Admission pays for retaining the new entry, relocating the prefix, and dropping it
        // if a later dependency fails or the consumer is cancelled.
        StaticMroWork::ResolvedBaseAppend { prefix_len, .. }
        | StaticMroWork::SequenceAppend { prefix_len, .. } => {
            scaled(prefix_len.checked_add(1).ok_or(Boundary::CostOverflow)?, 16)
        }
        StaticMroWork::InvalidBaseAppend { prefix_len, .. } => {
            scaled(prefix_len.checked_add(1).ok_or(Boundary::CostOverflow)?, 24)
        }
        StaticMroWork::InvalidBasesBox { len, .. } => scaled(len, 24),
        StaticMroWork::DirectSequenceCapacity { len } => scaled(len, 16),
        StaticMroWork::FixedMro { entries } => scaled(entries, 16),
        StaticMroWork::ExplicitBases
        | StaticMroWork::KnownClass
        | StaticMroWork::Pep695Classification
        | StaticMroWork::ObjectBase
        | StaticMroWork::RawBaseAdvance
        | StaticMroWork::ConvertBase { .. }
        | StaticMroWork::BaseCycleDispatch
        | StaticMroWork::StaticCycleRequest
        | StaticMroWork::SingleBaseCollectionRequest
        | StaticMroWork::ResolvedBaseAdvance
        | StaticMroWork::BaseCollectionRequest
        | StaticMroWork::DirectBaseAdvance
        | StaticMroWork::DirectBaseSpecializationRequest
        | StaticMroWork::DirectBaseAppend { .. }
        | StaticMroWork::C3Request
        | StaticMroWork::ErrorRequest
        | StaticMroWork::ErrorDetailsRequest
        | StaticMroWork::Publish => Ok(8),
    }
}

impl<'db> StaticMroEffects<'db> for PreparedStaticMroEffects<'_, '_, 'db, '_> {
    async fn body_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(MroFieldReads::new(self.db).body_scope(class))
    }

    async fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error> {
        Ok(MroFieldReads::new(self.db).is_object(class))
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> Result<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>, Self::Error> {
        Ok(MroFieldReads::new(self.db).static_class_literal(class))
    }

    async fn explicit_bases<'call>(
        &'call self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'call [Type<'db>], Self::Error>
    where
        'db: 'call,
    {
        self.prepared()?.explicit_bases(class).map_err(Into::into)
    }

    async fn has_pep_695_type_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.prepared()?
            .has_pep_695_type_params(class)
            .map_err(Into::into)
    }

    async fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        index: usize,
        _ty: Type<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.validate_environment(env)?;
        self.prepared()?
            .converted_explicit_base(class, index)
            .map_err(Into::into)
    }

    async fn object_base(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        self.validate_environment(env)?;
        self.prepared()?
            .object_base(env.program(self.db))
            .map_err(Into::into)
    }

    async fn checkpoint(&self, work: StaticMroWork) -> Result<(), Self::Error> {
        self.work
            .checkpoint(work_units(work)?)
            .await
            .map_err(Into::into)
    }

    async fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassType<'db>, Self::Error> {
        apply_optional_class_specialization_with(
            crate::types::mro::field_reads::MroFieldReads::new(self.db),
            class,
            specialization,
            &PreparedMroRootEffects::new(self.db, self.work),
        )
        .await
    }

    async fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<bool, Self::Error> {
        let result = self
            .work
            .router()
            .demand_static_mro(self.db, self.work, class, specialization)
            .await?;
        self.work
            .router()
            .static_mro_is_cycle(self.work, result)
            .await
    }

    async fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<Mro<'db>, Self::Error> {
        self.validate_environment(env)?;
        collect_single_base_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(self.db),
            env,
            root,
            base,
            additional,
            self,
        )
        .await
    }

    async fn collect_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> Result<VecDeque<ClassBase<'db>>, Self::Error> {
        self.validate_environment(env)?;
        collect_base_mro_with(
            crate::types::mro::field_reads::MroFieldReads::new(self.db),
            env,
            base,
            additional,
            self,
        )
        .await
    }

    async fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<ClassBase<'db>, Self::Error> {
        if specialization.is_some_and(|specialization| {
            specialization.generic_context(self.db).program(self.db) != self.env.program(self.db)
        }) {
            return Err(Boundary::ProgramDomain.into());
        }
        base.apply_optional_specialization_with(
            self.db,
            specialization,
            &PreparedMroMappingEffects::new(self.db, self.work),
        )
        .await
        .map_err(Into::into)
    }

    async fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> Result<Option<Mro<'db>>, Self::Error> {
        c3_merge_with(
            crate::types::mro::field_reads::MroFieldReads::new(self.db),
            sequences,
            &PreparedC3Effects { work: self.work },
        )
        .await
    }

    async fn make_error(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: ClassType<'db>,
        _kind: StaticMroErrorKind<'db>,
    ) -> Result<StaticMroError<'db>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::StaticMroErrorConstruction,
        ))
    }

    async fn failed_c3(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class_literal: StaticClassLiteral<'db>,
        _class: ClassType<'db>,
        _original_bases: &[Type<'db>],
        _resolved_bases: &[ClassBase<'db>],
    ) -> Result<Result<Mro<'db>, StaticMroError<'db>>, Self::Error> {
        Err(LookupFailure::Unsupported(
            LookupOperation::StaticMroErrorDetails,
        ))
    }
}
