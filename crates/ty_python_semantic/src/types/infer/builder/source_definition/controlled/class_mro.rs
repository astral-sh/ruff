//! Canonical static MRO construction and lazy traversal through source-owned dependencies.

use std::alloc::Layout;
use std::collections::VecDeque;

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::ScopeId;

use super::storage::{StorageQuote, dense_finish, sequence_merge};
use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::types::class::context::AsyncClassContextEffects;
use crate::types::class::{
    ClassDefaultSpecializationEffects, class_default_specialization_with,
    interpret_class_literal_lookup,
};
use crate::types::class_base::ClassBase;
use crate::types::class_base::conversion::{
    ClassBaseConversion, ClassBaseDependency, ClassBaseResolutionEffects, resolve_class_base_with,
};
use crate::types::class_base::specialization::apply_optional_base_specialization_with;
use crate::types::generics::tuple_runtime::{
    TupleRuntimeEffects, TupleRuntimeFacts, TupleRuntimeWork, tuple_runtime_specialization_with,
};
use crate::types::mro::base::{
    self, BaseMroEffects, BaseMroFacts, BaseMroStart, BaseMroWork, collect_base_mro_with,
    collect_single_base_mro_with,
};
use crate::types::mro::c3::{self, C3Effects, C3Work, c3_merge_with};
use crate::types::mro::collection::base::{collect_start_with, collect_start_with_root_with};
use crate::types::mro::collection::{
    ClassLiteralCollectionEffects, MroCollectionEffects, MroCollectionWork,
    collect_class_literals_with,
};
use crate::types::mro::construction::{
    self, StaticMroEffects, StaticMroFacts, StaticMroWork, static_mro_cycle_with, static_mro_with,
};
use crate::types::mro::field_reads::{MroFieldReads, MroIdentity};
use crate::types::mro::iteration::{MroCursor, MroDirection, MroIterationEffects, MroIterationWork};
use crate::types::mro::root::{
    self, MroRootEffects, MroRootFacts, MroRootWork, MroTailRequest,
    apply_optional_class_specialization_with, mro_first_with,
};
use crate::types::mro::{Mro, StaticMroError, StaticMroErrorKind};
use crate::types::subclass_of::SubclassConstructionEffects;
use crate::types::tuple::{TupleSpec, TupleType, VariableSegment};
use crate::types::{
    ClassLiteral, ClassType, GenericAlias, GenericContext, KnownClass, MaterializationKind,
    Specialization, StaticClassLiteral, Type,
};

fn mro_growth<T>(len: usize, capacity: usize) -> Option<StorageQuote> {
    let mut quote = sequence_merge::<T>(len, capacity, 1)?;
    quote.work = quote.work.checked_add(2)?;
    if quote.bytes != 0 {
        Layout::from_size_align(quote.bytes, align_of::<T>()).ok()?;
        let replacement_capacity = quote.bytes.checked_div(size_of::<T>())?;
        // Deque growth may also move its wrapped segment. Prepay both retained storage
        // retirement and the replacement allocation's disposal before either can change.
        quote.work = quote
            .work
            .checked_add(len.checked_mul(2)?)?
            .checked_add(capacity)?
            .checked_add(replacement_capacity.checked_mul(2)?)?;
    }
    quote.bytes = quote.bytes.checked_add(size_of::<T>().checked_mul(2)?)?;
    Some(quote)
}

fn mro_capacity<T>(entries: usize) -> Option<StorageQuote> {
    let layout = Layout::array::<T>(entries).ok()?;
    Some(StorageQuote {
        work: entries.checked_mul(2)?.checked_add(4)?,
        bytes: layout.size(),
    })
}

fn mro_finish<T>(len: usize, capacity: usize) -> Option<StorageQuote> {
    Layout::array::<T>(len).ok()?;
    let mut quote = dense_finish::<T>(len, capacity)?;
    let result_capacity = if quote.bytes == 0 {
        0
    } else {
        quote.bytes.checked_div(size_of::<T>())?
    };
    quote.work = quote
        .work
        .checked_add(capacity)?
        .checked_add(result_capacity.checked_mul(2)?)?;
    Some(quote)
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    async fn mro_storage(&self, quote: Option<StorageQuote>) -> RunResult<()> {
        let quote = quote.ok_or(RunError::Contract("MRO storage quotation overflow"))?;
        self.local(quote.work, quote.bytes, || ()).await
    }

    /// Computes the class-literal slice returned by the canonical `class_mro_literals` query.
    pub(in crate::types::infer) async fn infer_class_mro_literals(
        &self,
        class: ClassLiteral<'db>,
    ) -> RunResult<Box<[ClassLiteral<'db>]>> {
        let fields = self
            .local_with_fixed_transfers(2, 0, || MroFieldReads::new(self.db()))
            .await?;
        collect_class_literals_with(fields, class, self).await
    }

    pub(in crate::types::infer) async fn infer_static_mro(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>> {
        self.check_mro_program(class).await?;
        match static_mro_with(MroFieldReads::new(self.db()), class, specialization, self).await? {
            Ok(mro) => Ok(Ok(mro)),
            Err(error) => Ok(Err(self.box_mro_error(error).await?)),
        }
    }

    pub(in crate::types::infer) async fn initial_static_mro(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<Result<Mro<'db>, Box<StaticMroError<'db>>>> {
        self.check_mro_program(class).await?;
        let error =
            static_mro_cycle_with(MroFieldReads::new(self.db()), class, specialization, self)
                .await?;
        Ok(Err(self.box_mro_error(error).await?))
    }

    async fn check_mro_program(&self, class: StaticClassLiteral<'db>) -> RunResult<()> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await
    }

    async fn box_mro_error(
        &self,
        error: StaticMroError<'db>,
    ) -> RunResult<Box<StaticMroError<'db>>> {
        // Error construction prepays its payload's destruction; this admission covers the
        // additional box before it exists, including disposal if publication is interrupted.
        self.local(2, size_of::<StaticMroError<'db>>(), || ())
            .await?;
        Ok(Box::new(error))
    }

    async fn mro_object(&self, env: &ProgramEnvironment<'db>) -> RunResult<ClassBase<'db>> {
        let program = self.environment_program(env).await?;
        let lookup = self
            .access
            .known_class_lookup(program, KnownClass::Object)
            .await?;
        let class = self
            .local(1, 0, || interpret_class_literal_lookup(lookup))
            .await?;
        let Some(class) = class else {
            return self
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::MroObject))
                .await;
        };
        self.check_mro_program(class).await?;
        // The ordinary object conversion uses its class literal directly and does not request
        // object's generic context or MRO.
        Ok(ClassBase::Class(ClassType::NonGeneric(class.into())))
    }

    async fn stored_static_mro(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<&'db Result<Mro<'db>, Box<StaticMroError<'db>>>> {
        self.check_mro_program(class).await?;
        if let Some(specialization) = specialization {
            let alias = self
                .access
                .intern_generic_alias(class, specialization)
                .await?;
            return self.access.source_alias_mro(alias).await;
        }
        self.access.static_mro(class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> construction::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticMroFacts<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticMroEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn body_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        self.field(class.field_requests(self.db()).body_scope())
            .await
    }

    async fn is_object(&self, class: ClassType<'db>) -> RunResult<bool> {
        SubclassConstructionEffects::is_object(self, class).await
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.static_class_identity(class).await
    }

    async fn explicit_bases<'call>(
        &'call self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'call [Type<'db>]>
    where
        'db: 'call,
    {
        AsyncClassContextEffects::explicit_bases(self, class).await
    }

    async fn has_pep_695_type_params(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        Ok(
            AsyncClassContextEffects::pep695_generic_context(self, class)
                .await?
                .is_some(),
        )
    }

    async fn converted_explicit_base(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        _index: usize,
        ty: Type<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        self.environment_program(env).await?;
        let conversion = self
            .local(4, 0, || ClassBaseConversion::from_explicit_type(ty))
            .await?;
        resolve_class_base_with(conversion, env, Some(class.into()), self).await
    }

    async fn object_base(&self, env: &ProgramEnvironment<'db>) -> RunResult<ClassBase<'db>> {
        self.mro_object(env).await
    }

    async fn checkpoint(&self, work: StaticMroWork) -> RunResult<()> {
        match work {
            StaticMroWork::FixedMro { entries } => {
                // The shared driver boxes an array immediately after this checkpoint. Prepay
                // both filling that allocation and dropping it at any later suspension point.
                let units = Self::checked(entries.checked_mul(2).and_then(|n| n.checked_add(4)))?;
                let bytes = Self::checked(entries.checked_mul(size_of::<ClassBase<'db>>()))?;
                self.local(units, bytes, || ()).await
            }
            StaticMroWork::GenericProtocolScan { len }
            | StaticMroWork::GenericAliasScan { len } => {
                self.work(Self::checked(len.checked_add(1))?).await
            }
            StaticMroWork::ResolvedBaseAppend { prefix_len, capacity } => {
                self.mro_storage(mro_growth::<ClassBase<'db>>(prefix_len, capacity)).await
            }
            StaticMroWork::InvalidBaseAppend { prefix_len, capacity } => {
                self.mro_storage(mro_growth::<(usize, Type<'db>)>(prefix_len, capacity)).await
            }
            StaticMroWork::InvalidBasesBox { len, capacity } => {
                self.mro_storage(mro_finish::<(usize, Type<'db>)>(len, capacity)).await
            }
            StaticMroWork::SequenceStart { bases } => {
                self.mro_storage(bases.checked_add(2)
                    .and_then(mro_capacity::<VecDeque<ClassBase<'db>>>)
                    .and_then(|quote| quote.checked_add(mro_capacity::<ClassBase<'db>>(1)?)))
                    .await
            }
            StaticMroWork::SequenceAppend { prefix_len, capacity } => {
                self.mro_storage(mro_growth::<VecDeque<ClassBase<'db>>>(prefix_len, capacity)).await
            }
            StaticMroWork::DirectSequenceCapacity { len } => {
                self.mro_storage(mro_capacity::<ClassBase<'db>>(len)).await
            }
            StaticMroWork::DirectBaseAppend { len, capacity } => {
                self.mro_storage(mro_growth::<ClassBase<'db>>(len, capacity)).await
            }
            StaticMroWork::Begin
            | StaticMroWork::RootRequest
            | StaticMroWork::ExplicitBases
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
            | StaticMroWork::C3Request
            | StaticMroWork::ErrorRequest
            | StaticMroWork::ErrorDetailsRequest
            | StaticMroWork::Publish => self.work(4).await,
        }
    }

    async fn root_class(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<ClassType<'db>> {
        apply_optional_class_specialization_with(
            MroFieldReads::new(self.db()),
            class,
            specialization,
            self,
        )
        .await
    }

    async fn static_mro_is_cycle(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<bool> {
        let result = self.stored_static_mro(class, specialization).await?;
        self.local(1, 0, || {
            result.as_ref().is_err_and(|error| error.is_cycle())
        })
        .await
    }

    async fn collect_single_base_mro(
        &self,
        env: &ProgramEnvironment<'db>,
        root: ClassType<'db>,
        base: ClassBase<'db>,
        additional: Option<Specialization<'db>>,
    ) -> RunResult<Mro<'db>> {
        collect_single_base_mro_with(
            MroFieldReads::new(self.db()),
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
    ) -> RunResult<VecDeque<ClassBase<'db>>> {
        collect_base_mro_with(MroFieldReads::new(self.db()), env, base, additional, self).await
    }

    async fn specialize_base(
        &self,
        base: ClassBase<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> RunResult<ClassBase<'db>> {
        self.allocate_future(|| {
            apply_optional_base_specialization_with(self.db(), base, specialization, self)
        })
        .await?
        .await
    }

    async fn c3_merge(
        &self,
        sequences: Vec<VecDeque<ClassBase<'db>>>,
    ) -> RunResult<Option<Mro<'db>>> {
        c3_merge_with(MroFieldReads::new(self.db()), sequences, self).await
    }

    async fn make_error(
        &self,
        env: &ProgramEnvironment<'db>,
        class: ClassType<'db>,
        kind: StaticMroErrorKind<'db>,
    ) -> RunResult<StaticMroError<'db>> {
        match kind {
            StaticMroErrorKind::InheritanceCycle
            | StaticMroErrorKind::Pep695ClassWithGenericInheritance => {}
            _ => {
                return self
                    .unavailable(SourceOperation::ClassCheck(
                        ClassCheckOperation::MroErrorDetails,
                    ))
                    .await;
            }
        }
        let object = self.mro_object(env).await?;
        // Scalar error kinds own only the three-entry fallback. Include its full passive
        // retirement cost before construction so interruption can dispose of it immediately.
        self.local(14, 3 * size_of::<ClassBase<'db>>(), || ())
            .await?;
        Ok(kind.into_mro_error_with_object(class, object))
    }

    async fn failed_c3(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class_literal: StaticClassLiteral<'db>,
        _class: ClassType<'db>,
        _original_bases: &[Type<'db>],
        _resolved_bases: &[ClassBase<'db>],
    ) -> RunResult<Result<Mro<'db>, StaticMroError<'db>>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::MroErrorDetails,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassBaseResolutionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(4).await
    }

    async fn dependency(
        &self,
        _env: &ProgramEnvironment<'db>,
        _subclass: Option<ClassLiteral<'db>>,
        dependency: ClassBaseDependency<'db>,
    ) -> RunResult<Option<ClassBase<'db>>> {
        match dependency {
            ClassBaseDependency::DefaultSpecialization(class) => Ok(Some(
                mro_first_with(MroFieldReads::new(self.db()), class, None, self).await?,
            )),
            _ => {
                self.unavailable(SourceOperation::ClassCheck(
                    ClassCheckOperation::MroBaseConversion,
                ))
                .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> root::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MroRootFacts<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MroRootEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.access.class_generic_context(class).await
    }

    async fn generic_alias(
        &self,
        class: StaticClassLiteral<'db>,
        specialization: Specialization<'db>,
    ) -> RunResult<ClassType<'db>> {
        ClassDefaultSpecializationEffects::generic_alias(self, class, specialization).await
    }

    async fn checkpoint(&self, _work: MroRootWork) -> RunResult<()> {
        self.local_with_fixed_transfers(
            16,
            size_of::<MroRootWork>()
                + size_of::<ClassLiteral<'db>>() * 2
                + size_of::<ClassType<'db>>() * 2
                + size_of::<Option<GenericContext<'db>>>() * 2
                + size_of::<Option<Specialization<'db>>>() * 2
                + size_of::<RunResult<ClassType<'db>>>() * 2,
            || (),
        )
        .await
    }

    async fn default_class_specialization(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ClassType<'db>> {
        class_default_specialization_with(class, self).await
    }

    async fn tuple_runtime_specialization(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<Specialization<'db>> {
        tuple_runtime_specialization_with(specialization, self, TupleRuntimeFacts).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TupleRuntimeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: TupleRuntimeWork) -> RunResult<()> {
        // Prepay the shared decision's fixed values; field reads and canonical interning
        // retain their own admissions. Representation widths contribute bytes, not work.
        let (units, bytes) = match work {
            TupleRuntimeWork::Inspect => (
                14,
                size_of::<Specialization<'db>>() * 2
                    + size_of::<Option<TupleType<'db>>>() * 2
                    + size_of::<&TupleSpec<'db>>() * 2
                    + size_of::<VariableSegment<'db>>() * 2
                    + size_of::<bool>() * 4,
            ),
            TupleRuntimeWork::Intern => (
                8,
                size_of::<GenericContext<'db>>() * 2
                    + size_of::<Type<'db>>() * 2
                    + size_of::<[Type<'db>; 1]>() * 2
                    + size_of::<Option<MaterializationKind>>() * 2
                    + size_of::<Option<TupleType<'db>>>() * 2,
            ),
            TupleRuntimeWork::Publish => (4, size_of::<Specialization<'db>>() * 3),
        };
        self.local_with_fixed_transfers(
            units,
            bytes + size_of::<TupleRuntimeWork>() * 2,
            || (),
        )
        .await
    }

    async fn tuple_inner(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<TupleType<'db>>> {
        self.field(specialization.tuple_request(self.access.endpoint().field_request_context()))
            .await
    }

    async fn tuple_spec(&self, tuple: TupleType<'db>) -> RunResult<&'db TupleSpec<'db>> {
        self.field(
            tuple
                .field_requests(self.access.endpoint().field_request_context())
                .tuple(),
        )
        .await
    }

    async fn generic_context(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<GenericContext<'db>> {
        self.field(
            specialization
                .field_requests(self.access.endpoint().field_request_context())
                .generic_context(),
        )
        .await
    }

    async fn materialization_kind(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<Option<MaterializationKind>> {
        self.field(
            specialization
                .field_requests(self.access.endpoint().field_request_context())
                .materialization_kind(),
        )
        .await
    }

    async fn intern(
        &self,
        context: GenericContext<'db>,
        types: [Type<'db>; 1],
        kind: Option<MaterializationKind>,
        tuple: Option<TupleType<'db>>,
    ) -> RunResult<Specialization<'db>> {
        let quote = dense_finish::<Type<'db>>(1, 1).ok_or(RunError::Contract(
            "runtime tuple specialization payload quotation overflow",
        ))?;
        let types: Box<[Type<'db>]> = self
            .local(quote.work, quote.bytes, || Box::new(types))
            .await?;
        self.access
            .intern_specialization(context, types, kind, tuple)
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MroIterationEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn iteration_checkpoint(&self, _work: MroIterationWork) -> RunResult<()> {
        self.local_with_fixed_transfers(
            32,
            size_of::<MroCursor<'db>>() * 2
                + size_of::<MroDirection>()
                + size_of::<MroFieldReads<'db>>() * 2
                + size_of::<MroTailRequest<'db>>() * 2
                + size_of::<Option<std::slice::Iter<'db, ClassBase<'db>>>>() * 2
                + size_of::<RunResult<Option<ClassBase<'db>>>>() * 4,
            || (),
        )
        .await
    }

    async fn full_mro(&self, request: MroTailRequest<'db>) -> RunResult<&'db Mro<'db>> {
        let MroTailRequest::Static(class, specialization) = request else {
            return self
                .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::MroDynamic))
                .await;
        };
        let result = self.stored_static_mro(class, specialization).await?;
        self.local_with_fixed_transfers(2, 0, || match result {
            Ok(mro) => mro,
            Err(error) => error.fallback_mro(),
        })
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> base::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BaseMroFacts<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BaseMroEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn alias_origin(&self, alias: GenericAlias<'db>) -> RunResult<StaticClassLiteral<'db>> {
        self.field(alias.field_requests(self.db()).origin()).await
    }

    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.field(alias.field_requests(self.db()).specialization())
            .await
    }

    async fn object_base(&self, env: &ProgramEnvironment<'db>) -> RunResult<ClassBase<'db>> {
        self.mro_object(env).await
    }

    async fn checkpoint(&self, _work: BaseMroWork) -> RunResult<()> {
        self.work(4).await
    }

    async fn compose_specialization(
        &self,
        base: Specialization<'db>,
        additional: Specialization<'db>,
    ) -> RunResult<Specialization<'db>> {
        self.compose_source_specialization(base, additional).await
    }

    async fn collect_start(&self, start: BaseMroStart<'db>) -> RunResult<VecDeque<ClassBase<'db>>> {
        collect_start_with(MroFieldReads::new(self.db()), start, self).await
    }

    async fn collect_start_with_root(
        &self,
        root: ClassType<'db>,
        start: BaseMroStart<'db>,
    ) -> RunResult<Mro<'db>> {
        collect_start_with_root_with(MroFieldReads::new(self.db()), root, start, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MroCollectionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn collection_checkpoint(&self, work: MroCollectionWork) -> RunResult<()> {
        match work {
            MroCollectionWork::Append { len, capacity } => {
                self.mro_storage(mro_growth::<ClassBase<'db>>(len, capacity))
                    .await
            }
            MroCollectionWork::BoxOutput { len, capacity } => {
                self.mro_storage(mro_finish::<ClassBase<'db>>(len, capacity))
                    .await
            }
            MroCollectionWork::Begin
            | MroCollectionWork::Classify
            | MroCollectionWork::Publish => self.work(4).await,
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassLiteralCollectionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn literal_collection_checkpoint(&self, work: MroCollectionWork) -> RunResult<()> {
        let quote = self
            .local_with_fixed_transfers(48, 0, || match work {
                MroCollectionWork::Begin => Some(StorageQuote {
                    work: 8,
                    bytes: size_of::<MroCursor<'db>>() + size_of::<Vec<ClassLiteral<'db>>>(),
                }),
                MroCollectionWork::Classify => Some(StorageQuote {
                    work: 4,
                    bytes: size_of::<Option<ClassBase<'db>>>()
                        + size_of::<ClassBase<'db>>()
                        + size_of::<ClassType<'db>>(),
                }),
                MroCollectionWork::Append { len, capacity } => {
                    mro_growth::<ClassLiteral<'db>>(len, capacity)
                }
                MroCollectionWork::BoxOutput { len, capacity } => {
                    mro_finish::<ClassLiteral<'db>>(len, capacity).and_then(|quote| {
                        quote.checked_add(StorageQuote {
                            work: 2,
                            bytes: size_of::<Vec<ClassLiteral<'db>>>()
                                + size_of::<Box<[ClassLiteral<'db>]>>(),
                        })
                    })
                }
                MroCollectionWork::Publish => Some(StorageQuote {
                    work: 2,
                    bytes: size_of::<Box<[ClassLiteral<'db>]>>()
                        + size_of::<RunResult<Box<[ClassLiteral<'db>]>>>(),
                }),
            })
            .await?;
        self.local_quoted_with_fixed_transfers(
            quote
                .map(|quote| (quote.work, quote.bytes))
                .ok_or(RunError::Contract("class literal collection quotation overflow")),
            || (),
        )
        .await
    }

    async fn class_literal(
        &self,
        _fields: MroFieldReads<'db>,
        class: ClassType<'db>,
    ) -> RunResult<ClassLiteral<'db>> {
        let class = self.local_with_fixed_transfers(2, 0, || class).await?;
        match class {
            ClassType::NonGeneric(literal) => {
                self.local_with_fixed_transfers(1, 0, || literal).await
            }
            ClassType::Generic(alias) => {
                let request = self
                    .local_with_fixed_transfers(4, 0, || {
                        alias
                            .field_requests(self.access.endpoint().field_request_context())
                            .origin()
                    })
                    .await?;
                let origin = self.field_with_profile(request, &FixedFieldCopy).await?;
                self.local_with_fixed_transfers(1, 0, || ClassLiteral::Static(origin))
                    .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> c3::sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> C3Effects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: C3Work) -> RunResult<()> {
        match work {
            C3Work::OutputCapacity { entries } => {
                self.mro_storage(mro_capacity::<ClassBase<'db>>(entries))
                    .await
            }
            C3Work::OutputAppend {
                prefix_len,
                capacity,
            } => {
                self.mro_storage(mro_growth::<ClassBase<'db>>(prefix_len, capacity))
                    .await?;
                #[cfg(test)]
                if prefix_len > 0 {
                    crate::types::infer::source_runtime::tests::explicit_specialization::observe_c3_append(self.db());
                }
                Ok(())
            }
            C3Work::BoxOutput { len, capacity } => {
                self.mro_storage(mro_finish::<ClassBase<'db>>(len, capacity))
                    .await
            }
            C3Work::RetainSequences { len } => {
                self.local(
                    Self::checked(len.checked_mul(4).and_then(|work| work.checked_add(4)))?,
                    Self::checked(len.checked_mul(size_of::<VecDeque<ClassBase<'db>>>()))?,
                    || (),
                )
                .await
            }
            C3Work::IdentityComparison { todo_bytes } | C3Work::RemoveHead { todo_bytes } => {
                self.local(
                    Self::checked(todo_bytes.checked_add(16))?,
                    size_of::<Type<'db>>() * 2,
                    || (),
                )
                .await
            }
            C3Work::CandidateAdvance
            | C3Work::TailSequenceAdvance
            | C3Work::TailEntryAdvance
            | C3Work::SelectedIdentity
            | C3Work::RemovalSequenceAdvance
            | C3Work::Publish => self.work(4).await,
        }
    }

    async fn mro_identity(
        &self,
        _fields: MroFieldReads<'db>,
        base: ClassBase<'db>,
    ) -> RunResult<Type<'db>> {
        match self.local(1, 0, || MroIdentity::of(base)).await? {
            MroIdentity::Type(ty) => Ok(ty),
            MroIdentity::GenericAlias(alias) => {
                #[cfg(test)]
                crate::types::infer::source_runtime::tests::explicit_specialization::observe_c3_origin(self.db(), alias, false);
                let origin = BaseMroEffects::alias_origin(self, alias).await?;
                #[cfg(test)]
                crate::types::infer::source_runtime::tests::explicit_specialization::observe_c3_origin(self.db(), alias, true);
                Ok(Type::ClassLiteral(origin.into()))
            }
        }
    }
}
