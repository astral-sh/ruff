//! Canonical member queries backed by the original declaration and descriptor producers.

use crate::types::visitor::SearchWork;
use std::future::Future;

use crate::types::call::dunder::{
    DunderCallRequest, DunderCallResult, DunderEffects, DunderLookup, DunderRead, DunderWork,
};
use crate::types::call::{Bindings, CallArguments, CallDunderError, CallError};
use crate::types::callable::FunctionDescriptorEffects;
use crate::types::class::namespace::{
    self, NamespaceLookupEffects, NamespaceLookupRequest, NamespaceLookupWork,
};
use crate::types::descriptor::effects::DescriptorEffects;
use crate::types::descriptor::{DescriptorInvocationRequest, DescriptorMemberRequest};
use crate::types::instance::effects::{InstanceEffects, InstanceWork};
use crate::types::{
    DescriptorGetCallContext, DescriptorOrigin, InstanceFallbackShadowsNonDataDescriptor,
    IntersectionBuilder, IntersectionType, LookupDescriptorEffects, LookupFacts, LookupParts,
    MemberEntryEffects, NewType, NominalInstanceType, PropertyDeprecations, SlotDescriptorType,
    TypeContext, TypeDispatchEffects, TypeQualifiers, UnionType,
};

use crate::place::source_effects::PublicLookupEffects;
use crate::place::{
    LookupError, LookupResult, Place, PlaceFromDeclarationsResult, PlaceWithDefinition, Provenance,
};
use crate::types::class::implicit_attributes::{
    self, AugmentedBindings, ImplicitAttribute, ImplicitNameSearchControl,
};
use crate::types::class::instance_flags::{
    self, InstanceFlagFacts, InstanceFlagsWork, QueuedInstanceFlags,
};
use crate::types::class::instance_storage::{
    self, ClassInstanceStorageEffects, InstanceClassificationEffects, InstanceStorageWork,
    StaticInstanceStorageEffects,
};
use crate::types::class::member_lookup::{
    self as mro_members, InstanceMroEffects, InstanceMroWork, MemberFinalizationEffects,
    MemberFinalizationWork, MroClassMemberRequest, MroImplicitAttribute, MroMemberEffects,
    MroMemberWork, MroPendingBindings,
};
use crate::types::class::member_source::{
    self, ImplicitAttributeEffects, MemberSourceEffects, MemberSourceWork, RawClassMemberEffects,
    RawClassMemberFacts, StaticCodeGeneratorEffects, StaticInstanceMemberEffects,
};
use crate::types::class::own_member::{
    self, ClassTypeOwnMemberEffects, ClassTypeOwnMemberRequest, ClassTypeOwnMemberWork,
    OwnMemberEffects, OwnMemberLookupRequest,
};
use crate::types::class::slots::{
    self, InstanceLayout, SlotDefinition, SlotSelectorEffects, SlotSelectorWork,
};
use crate::types::class::static_literal::{self, StaticFinalityEffects};
use crate::types::class::synthesized_member::{
    self, SynthesizedMemberEffects, SynthesizedMemberWork,
};
use crate::types::class::{
    self, ClassInstanceFlags, ClassMetaclass, CodeGeneratorKind, DynamicClassLiteral,
    DynamicEnumLiteral, DynamicNamedTupleLiteral, DynamicTypedDictLiteral, FrozenDataclassMethod,
    InstanceMemberResult, MethodDecorator,
};
use crate::types::generics::Specialization;
use crate::types::instance::{self, NominalClassEffects, NominalClassFacts, NominalInstanceClass};
use crate::types::member::Member;
use crate::types::member_lookup::general::{
    GeneralMemberBranch, GeneralMemberEffects, GeneralMemberFacts, GeneralMemberName,
    GeneralMemberOperation, GeneralMemberPredicate,
};
use crate::types::mro::iteration::MroCursor;
use crate::types::storage_quote::{buffer_push_quote, buffer_retirement};
use crate::types::subclass_of::{
    self, SubclassConstructionEffects, SubclassConstructionFacts, SubclassOfInner,
};
use crate::types::tuple::{TupleSpec, TupleType};
use crate::types::{
    ClassBase, ClassLiteral, ClassType, DataclassFlags, DataclassParams, FunctionType,
    GenericAlias, GenericContext, KnownClass, KnownFunction, KnownInstanceType, StaticClassLiteral,
    UnionBuilder,
};
use crate::{FxOrderSet, ProgramEnvironment};
use ruff_python_ast::PythonVersion;
use ruff_python_ast::name::Name;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{
    BindingWithConstraintsIterator, DeclarationsIterator, ImportedFinalCandidatesIterator,
    PlaceTable, UseDefMap,
};

use salsa::execution_probe::{
    CallableRoute, ExecutionWork, InternedValues, NativeValueOperation, NativeValueQuote,
    PassiveMemoSchema, QueryKeys, RunError, RunResult, TaskEndpoint,
};
use salsa::plumbing::AsId;
use salsa::plumbing::function::Configuration;
use ty_python_core::place::ScopedPlaceId;
use ty_python_core::scope::ScopeId;

use super::runtime_profile::quote_native_value;
pub(in crate::types) use super::runtime_profile::{
    DescriptorConfiguration, DescriptorLookupKeyProfile, PlaceConfiguration, PlaceLookupKeyProfile,
};
use super::{intern_member_lookup_key, intern_member_lookup_key_from_str};
use crate::place::{ConsideredDefinitions, PlaceAndQualifiers, RequiresExplicitReExport};
use crate::types::descriptor::{self, DescriptorRequest, DescriptorResult};
use crate::types::{MemberLookupKey, MemberLookupPolicy, MemberLookupResult, Type};
use crate::{Db, Program};

pub(in crate::types) trait MemberQueryAccess<'run, 'db: 'run>:
    Clone + 'run
{
    const AVAILABLE: bool;
    fn member<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &'call str,
        policy: MemberLookupPolicy,
    ) -> impl Future<Output = RunResult<MemberLookupResult<'db>>> + 'call
    where
        'run: 'call;

    async fn member_name<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: GeneralMemberName<'call>,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        self.member(endpoint, program, ty, name.as_str(), policy)
            .await
    }
}

#[derive(Clone, Copy)]
pub(in crate::types) struct NoMemberQueries;

impl<'run, 'db: 'run> MemberQueryAccess<'run, 'db> for NoMemberQueries {
    const AVAILABLE: bool = false;
    async fn member<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        _program: Program<'db>,
        _ty: Type<'db>,
        _name: &'call str,
        _policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted,
                ))
            })
            .await)
    }
}

pub(in crate::types) struct MemberQueries<'run, 'db: 'run, CL, ML, PP, DG, S>
where
    CL: Configuration,
    ML: Configuration,
    PP: PlaceConfiguration,
    DG: DescriptorConfiguration,
    S: PassiveMemoSchema<'db, MemberLookupKey<'static>>,
{
    pub(in crate::types) class: CallableRoute<'run, 'db, CL>,
    pub(in crate::types) member: CallableRoute<'run, 'db, ML>,
    pub(in crate::types) place: CallableRoute<'run, 'db, PP>,
    pub(in crate::types) descriptor: CallableRoute<'run, 'db, DG>,
    pub(in crate::types) values: &'run InternedValues<'db, MemberLookupKey<'static>, S>,
    pub(in crate::types) strings:
        &'run InternedValues<'db, crate::types::literal::StringLiteralType<'static>, ()>,
    pub(in crate::types) place_keys: &'run QueryKeys<'db, PP, PlaceLookupKeyProfile>,
    pub(in crate::types) descriptor_keys: &'run QueryKeys<'db, DG, DescriptorLookupKeyProfile>,
}

impl<'run, 'db: 'run, CL, ML, PP, DG, S> Clone for MemberQueries<'run, 'db, CL, ML, PP, DG, S>
where
    CL: Configuration,
    ML: Configuration,
    PP: PlaceConfiguration,
    DG: DescriptorConfiguration,
    S: PassiveMemoSchema<'db, MemberLookupKey<'static>>,
{
    fn clone(&self) -> Self {
        Self {
            class: self.class.clone(),
            member: self.member.clone(),
            place: self.place.clone(),
            descriptor: self.descriptor.clone(),
            values: self.values,
            strings: self.strings,
            place_keys: self.place_keys,
            descriptor_keys: self.descriptor_keys,
        }
    }
}

impl<'run, 'db: 'run, CL, ML, PP, DG, S> MemberQueries<'run, 'db, CL, ML, PP, DG, S>
where
    CL: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = PlaceAndQualifiers<'a>,
        >,
    ML: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = MemberLookupResult<'a>,
        >,
    PP: PlaceConfiguration,
    DG: DescriptorConfiguration,
    S: PassiveMemoSchema<'db, MemberLookupKey<'static>> + 'run,
{
    pub(in crate::types) async fn class<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &'call str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        let key =
            intern_member_lookup_key_from_str(endpoint, self.values, program, ty, name, policy)
                .await;
        Ok(endpoint
            .child_call(|| async { Ok(*endpoint.fetch_ref(&self.class, key.as_id())?.await?) })
            .await)
    }

    pub(in crate::types) async fn place<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        definitions: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        let key = endpoint
            .intern_query_key(self.place_keys, (scope, place, reexport, definitions))
            .await;
        Ok(endpoint
            .child_call(|| async { Ok(*endpoint.fetch_ref(&self.place, key)?.await?) })
            .await)
    }

    pub(in crate::types) async fn descriptor<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>>
    where
        'run: 'call,
    {
        let key = endpoint
            .intern_query_key(
                self.descriptor_keys,
                (program, request.ty, request.instance, request.owner),
            )
            .await;
        Ok(endpoint
            .child_call(|| async { Ok(*endpoint.fetch_ref(&self.descriptor, key)?.await?) })
            .await)
    }
}

impl<'run, 'db: 'run, CL, ML, PP, DG, S> MemberQueryAccess<'run, 'db>
    for MemberQueries<'run, 'db, CL, ML, PP, DG, S>
where
    CL: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = PlaceAndQualifiers<'a>,
        >,
    ML: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = MemberLookupResult<'a>,
        >,
    PP: PlaceConfiguration,
    DG: DescriptorConfiguration,
    S: PassiveMemoSchema<'db, MemberLookupKey<'static>> + 'run,
{
    const AVAILABLE: bool = true;

    async fn member<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &'call str,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        self.member_name(endpoint, program, ty, GeneralMemberName::Text(name), policy)
            .await
    }

    async fn member_name<'call>(
        &'call self,
        endpoint: &'call TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: GeneralMemberName<'call>,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        let key = match name {
            GeneralMemberName::Text(name) => {
                intern_member_lookup_key_from_str(endpoint, self.values, program, ty, name, policy)
                    .await
            }
            GeneralMemberName::Shared(name) => {
                let name = endpoint
                    .local_call(|| {
                        endpoint.admit_work(1)?;
                        endpoint.check_completion()?;
                        Ok(name.clone())
                    })
                    .await;
                intern_member_lookup_key(endpoint, self.values, program, ty, name, policy).await
            }
        };
        Ok(endpoint
            .child_call(|| async { Ok(*endpoint.fetch_ref(&self.member, key.as_id())?.await?) })
            .await)
    }
}

pub(in crate::types) trait MemberSourcesAccess<'run, 'db: 'run>: 'run {
    fn record_unsupported(&self, operation: &'static str);
    fn db(&self) -> &'db dyn Db;
    async fn places(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db ty_python_core::PlaceTable>;
    async fn uses(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db ty_python_core::UseDefMap<'db>>;
    async fn public_place(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        definitions: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>>;
    async fn binding_place<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &crate::ProgramEnvironment<'db>,
        bindings: ty_python_core::BindingWithConstraintsIterator<'map, 'db>,
    ) -> RunResult<crate::place::PlaceWithDefinition<'db>>;
    async fn declaration_place<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &crate::ProgramEnvironment<'db>,
        declarations: ty_python_core::DeclarationsIterator<'map, 'db>,
    ) -> RunResult<crate::place::PlaceFromDeclarationsResult<'db>>;
    async fn imported_final<'map>(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        env: &crate::ProgramEnvironment<'db>,
        result: crate::place::PlaceFromDeclarationsResult<'db>,
        imported: ty_python_core::ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> RunResult<crate::place::PlaceFromDeclarationsResult<'db>>;
    async fn explicit_bases(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]>;
    async fn context(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::GenericContext<'db>>>;
    async fn mro_start(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::ClassType<'db>,
    ) -> RunResult<crate::types::mro::iteration::MroCursor<'db>>;
    async fn mro_next(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        cursor: &mut crate::types::mro::iteration::MroCursor<'db>,
    ) -> RunResult<Option<crate::types::ClassBase<'db>>>;
    async fn known_class(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        known: crate::types::KnownClass,
    ) -> RunResult<Type<'db>>;
    async fn known_instance(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        known: crate::types::KnownClass,
    ) -> RunResult<Type<'db>>;
    async fn enum_metadata(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::ClassLiteral<'db>,
    ) -> RunResult<Option<&'db crate::types::enums::EnumMetadata<'db>>>;
    async fn enum_class(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::ClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::enums::EnumClassLiteral<'db>>>;
    async fn decorators(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> RunResult<&'db [Type<'db>]>;
    async fn code_generator(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        class: crate::types::StaticClassLiteral<'db>,
    ) -> RunResult<Option<crate::types::class::CodeGeneratorKind<'db>>>;
    async fn implicit_names(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
    ) -> RunResult<&'db [ruff_python_ast::name::Name]>;
}

pub(in crate::types) trait MemberDependencies<'run, 'db: 'run>:
    MemberQueryAccess<'run, 'db>
{
    async fn name_literal(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        name: &str,
    ) -> RunResult<Type<'db>>;
    async fn class_lookup(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>>;
    async fn public_place(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        definitions: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>>;
    async fn descriptor_get(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>>;
}

impl<'run, 'db: 'run, CL, ML, PP, DG, S> MemberDependencies<'run, 'db>
    for MemberQueries<'run, 'db, CL, ML, PP, DG, S>
where
    CL: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = PlaceAndQualifiers<'a>,
        >,
    ML: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = MemberLookupResult<'a>,
        >,
    PP: PlaceConfiguration,
    DG: DescriptorConfiguration,
    S: PassiveMemoSchema<'db, MemberLookupKey<'static>> + 'run,
{
    async fn name_literal(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        name: &str,
    ) -> RunResult<Type<'db>> {
        Ok(crate::types::literal::intern_member_name_literal(endpoint, self.strings, name).await)
    }
    async fn class_lookup(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.class(endpoint, program, ty, name, policy).await
    }
    async fn public_place(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        scope: ScopeId<'db>,
        place: ScopedPlaceId,
        reexport: RequiresExplicitReExport,
        definitions: ConsideredDefinitions,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.place(endpoint, scope, place, reexport, definitions)
            .await
    }
    async fn descriptor_get(
        &self,
        endpoint: &TaskEndpoint<'run, 'db>,
        program: Program<'db>,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        self.descriptor(endpoint, program, request).await
    }
}

pub(in crate::types) struct PlaceProvider<'run, S> {
    pub(in crate::types) sources: &'run S,
}

impl<'run, 'db: 'run, PP: PlaceConfiguration, S: MemberSourcesAccess<'run, 'db>>
    salsa::execution_probe::CallableRouteProvider<'run, 'db, PP> for PlaceProvider<'run, S>
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, PP>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        quote_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        (scope, place, reexport, definitions): PP::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        self.sources
            .public_place(&endpoint, scope, place, reexport, definitions)
            .await
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        _input: PP::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(crate::place::Place::bound(Type::divergent(id)).into())
            })
            .await)
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call PlaceAndQualifiers<'db>,
        _value: PlaceAndQualifiers<'db>,
        _input: PP::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted,
                ))
            })
            .await)
    }
}

pub(in crate::types) struct MemberEffects<'call, 'run: 'call, 'db: 'run, Q, S> {
    pub(in crate::types) endpoint: &'call TaskEndpoint<'run, 'db>,
    pub(in crate::types) queries: &'call Q,
    pub(in crate::types) sources: &'call S,
    pub(in crate::types) env: crate::ProgramEnvironment<'db>,
    pub(in crate::types) fields: crate::types::mro::field_reads::MroFieldReads<'db>,
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> MemberEffects<'call, 'run, 'db, Q, S>
{
    fn db(&self) -> &'db dyn Db {
        self.sources.db()
    }
    fn program(&self) -> Program<'db> {
        self.env.program(self.db())
    }
    async fn local<T>(&self, action: impl FnOnce() -> RunResult<T>) -> RunResult<T> {
        Ok(self.endpoint.local_call(action).await)
    }
    fn admit_local_work(&self, extra: usize) -> RunResult<()> {
        let units = extra.checked_add(1).ok_or(RunError::Refused(
            salsa::attempt_probe::Incomplete::Allowance,
        ))?;
        self.endpoint.admit_work(units)
    }
    async fn work(&self, units: usize) -> RunResult<()> {
        self.local(|| self.admit_local_work(units)).await
    }
    async fn push_mro_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> RunResult<()> {
        self.local(|| {
            self.admit_local_work(0)?;
            let quote = buffer_push_quote::<(ClassType<'db>, AugmentedBindings<'db>)>((
                pending.len(),
                pending.capacity(),
                pending.capacity() != 0,
            ))
            .ok_or(RunError::Contract("MRO pending buffer quotation overflow"))?;
            self.admit_local_work(quote.work)?;
            if quote.bytes != 0 {
                self.endpoint.admit(ExecutionWork::Resource {
                    requested_bytes: quote.bytes,
                })?;
            }
            self.endpoint.check_completion()?;
            pending.push((class, bindings));
            Ok(())
        })
        .await
    }
    async fn clear_mro_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        self.local(|| {
            self.admit_local_work(0)?;
            let work = buffer_retirement::<(ClassType<'db>, AugmentedBindings<'db>)>((
                pending.len(),
                0,
                false,
            ))
            .ok_or(RunError::Contract("MRO pending buffer quotation overflow"))?;
            self.admit_local_work(work)?;
            self.endpoint.check_completion()?;
            pending.clear();
            Ok(())
        })
        .await
    }
    async fn finish_mro_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> RunResult<()> {
        let work = self
            .local(|| {
                self.admit_local_work(0)?;
                buffer_retirement::<(ClassType<'db>, AugmentedBindings<'db>)>((
                    pending.len(),
                    pending.capacity(),
                    pending.capacity() != 0,
                ))
                .ok_or(RunError::Contract("MRO pending buffer quotation overflow"))
            })
            .await?;
        self.work(work).await?;
        drop(pending);
        Ok(())
    }
    async fn refuse<T>(&self, operation: &'static str) -> RunResult<T> {
        self.local(|| {
            self.admit_local_work(0)?;
            self.sources.record_unsupported(operation);
            Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Interrupted,
            ))
        })
        .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::class::member_source::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::class::own_member::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::class::synthesized_member::sealed::Sealed
    for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::class::instance_storage::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::class::member_lookup::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::class::instance_flags::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::place::source_effects::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::descriptor::effects::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::signatures::effects::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::call::dunder::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::class::namespace::sealed::Sealed for MemberEffects<'call, 'run, 'db, Q, S>
{
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> MemberSourceEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn place_table(&self, scope: ScopeId<'db>) -> Result<&'db PlaceTable, Self::Error> {
        self.sources.places(self.endpoint, scope).await
    }
    async fn use_def_map(&self, scope: ScopeId<'db>) -> Result<&'db UseDefMap<'db>, Self::Error> {
        self.sources.uses(self.endpoint, scope).await
    }
    async fn symbol_id(
        &self,
        table: &'db PlaceTable,
        name: &str,
    ) -> Result<Option<ScopedSymbolId>, Self::Error> {
        self.local(|| {
            let work = table
                .symbol_lookup_work(name.len())
                .ok_or(RunError::Contract("symbol lookup work overflow"))?;
            self.admit_local_work(work)?;
            Ok(table.symbol_id(name))
        })
        .await
    }
    async fn binding_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Self::Error> {
        self.sources
            .binding_place(self.endpoint, env, bindings)
            .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> RawClassMemberEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn public_class_place(
        &self,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.queries
            .public_place(
                self.endpoint,
                scope,
                symbol.into(),
                RequiresExplicitReExport::No,
                ConsideredDefinitions::EndOfScope,
            )
            .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> StaticCodeGeneratorEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .dataclass_params(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .known(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn has_explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .has_explicit_bases(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn has_explicit_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .has_explicit_metaclass(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    type Error = RunError;
    async fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn code_generator_query(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        self.sources.code_generator(self.endpoint, class).await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> ImplicitAttributeEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn body_scope(
        &self,
        value: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .body_scope(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }
    type Error = RunError;
    async fn checkpoint(&self, work: MemberSourceWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn names(&self, scope: ScopeId<'db>) -> Result<&'db [Name], Self::Error> {
        self.sources.implicit_names(self.endpoint, scope).await
    }
    async fn find_name(
        &self,
        names: &'db [Name],
        name: &str,
    ) -> Result<Option<usize>, Self::Error> {
        self.local(|| implicit_attributes::implicit_name_index_with(names, name, self))
            .await
    }
    async fn infer_named_attribute(
        &self,
        _scope: ScopeId<'db>,
        _name: &'db Name,
        _target: MethodDecorator,
    ) -> Result<ImplicitAttribute<'db>, Self::Error> {
        self.refuse("infer_named_attribute").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> StaticInstanceMemberEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn body_scope(
        &self,
        value: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .body_scope(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }
    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        class::static_code_generator_with(class, self).await
    }
    async fn has_own_named_tuple_field(
        &self,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> Result<bool, Self::Error> {
        self.refuse("has_own_named_tuple_field").await
    }
    async fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error> {
        self.sources
            .declaration_place(self.endpoint, env, declarations)
            .await
    }
    async fn imported_final<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        result: PlaceFromDeclarationsResult<'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error> {
        self.sources
            .imported_final(self.endpoint, env, result, imported)
            .await
    }
    async fn implicit_member(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(implicit_attributes::implicit_attribute_bindings_with(
            class,
            name,
            MethodDecorator::None,
            self,
        )
        .await?
        .member())
    }
    async fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(ty.is_instance_of(self.db(), KnownClass::KwOnly))
        })
        .await
    }
    async fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(class.file(self.db()).is_stub(self.db()))
        })
        .await
    }
    async fn has_instance_slot(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error> {
        slots::instance_slot_with(class, name, self).await
    }
    async fn is_own_dataclass_instance_field(
        &self,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> Result<bool, Self::Error> {
        self.refuse("is_own_dataclass_instance_field").await
    }
    async fn getter_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.queries
            .class_lookup(
                self.endpoint,
                self.local(|| Ok(env.program(self.db()))).await?,
                ty,
                "__get__",
                MemberLookupPolicy::default(),
            )
            .await
    }
    async fn union_two(
        &self,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("union_two").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> SlotSelectorEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn body_scope(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ScopeId<'db>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .body_scope(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<DataclassParams<'db>>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .dataclass_params(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn dataclass_flags(
        &self,
        params: DataclassParams<'db>,
    ) -> Result<DataclassFlags, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                params
                    .field_requests(self.endpoint.field_request_context())
                    .flags(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .known(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn has_explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .has_explicit_bases(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn slot_checkpoint(&self, work: SlotSelectorWork) -> Result<(), Self::Error> {
        self.endpoint
            .local_call(|| {
                let units = work
                    .work_units()
                    .and_then(|units| units.checked_add(1))
                    .ok_or(RunError::Refused(
                        salsa::attempt_probe::Incomplete::Allowance,
                    ))?;
                self.endpoint.admit_work(units)
            })
            .await;
        Ok(())
    }
    async fn next_binding_has_definition<'map>(
        &self,
        bindings: &mut BindingWithConstraintsIterator<'map, 'db>,
    ) -> Result<Option<bool>, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(slots::next_slot_binding_has_definition(bindings))
        })
        .await
    }
    async fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<&'db [Type<'db>], Self::Error> {
        self.sources.explicit_bases(self.endpoint, class).await
    }
    async fn source_python_version(
        &self,
        scope: ScopeId<'db>,
    ) -> Result<PythonVersion, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(ProgramEnvironment::from_scope(scope).python_version(self.db()))
        })
        .await
    }
    async fn slot_definition(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<&'db SlotDefinition, Self::Error> {
        self.refuse("slot_definition").await
    }
    async fn instance_layout(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<&'db InstanceLayout, Self::Error> {
        self.refuse("instance_layout").await
    }
    async fn is_stub(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(class.file(self.db()).is_stub(self.db()))
        })
        .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> ClassTypeOwnMemberEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn alias_origin(
        &self,
        value: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .origin(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }
    async fn alias_specialization(
        &self,
        value: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .specialization(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }
    async fn is_tuple(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        let known = self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .known(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await;
        self.work(1).await?;
        Ok(known == Some(KnownClass::Tuple))
    }
    async fn specialization_tuple(
        &self,
        specialization: Specialization<'db>,
    ) -> Result<Option<&'db TupleSpec<'db>>, Self::Error> {
        let fields = self.endpoint.field_request_context();
        let tuple = self
            .endpoint
            .read_field(
                specialization.tuple_request(fields),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await;
        match tuple {
            Some(tuple) => Ok(Some(
                self.endpoint
                    .read_field(
                        tuple.field_requests(fields).tuple(),
                        &salsa::execution_probe::BorrowOrCopy,
                    )
                    .await,
            )),
            None => Ok(None),
        }
    }
    type Error = RunError;
    async fn checkpoint(&self, work: ClassTypeOwnMemberWork) -> Result<(), Self::Error> {
        self.work(match work {
            ClassTypeOwnMemberWork::Admission { name_bytes } => name_bytes,
            _ => 0,
        })
        .await
    }
    async fn dynamic_member(
        &self,
        _class: DynamicClassLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("dynamic_member").await
    }
    async fn named_tuple_member(
        &self,
        _class: DynamicNamedTupleLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("named_tuple_member").await
    }
    async fn typed_dict_member(
        &self,
        _class: DynamicTypedDictLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("typed_dict_member").await
    }
    async fn enum_member(
        &self,
        _class: DynamicEnumLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("enum_member").await
    }
    async fn tuple_len(
        &self,
        _class: ClassType<'db>,
        _specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("tuple_len").await
    }
    async fn tuple_getitem(&self, _tuple: &'db TupleSpec<'db>) -> Result<Member<'db>, Self::Error> {
        self.refuse("tuple_getitem").await
    }
    async fn tuple_new(
        &self,
        _class: ClassType<'db>,
        _specialization: Option<Specialization<'db>>,
        _context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("tuple_new").await
    }
    async fn tuple_runtime_specialization(
        &self,
        _specialization: Specialization<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        self.refuse("tuple_runtime_specialization").await
    }
    async fn static_own_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        own_member::own_class_member_with(request, self).await
    }
    async fn owner_specialize(
        &self,
        _ty: Type<'db>,
        _specialization: Specialization<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("owner_specialize").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> OwnMemberEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        class::static_code_generator_with(class, self).await
    }
    async fn raw_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        self.work(0).await?;
        member_source::raw_class_member_with(
            self.local(|| Ok(self.fields.body_scope(request.class)))
                .await?,
            request.name,
            RawClassMemberFacts,
            self,
        )
        .await
    }
    async fn slot_exists(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        slots::own_slot_descriptor_with(request.class, request.name, self).await
    }
    async fn generated_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        slots::generated_slots_with(class, self).await
    }
    async fn explicit_slots(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        slots::own_class_binding_with(class, "__slots__", self).await
    }
    async fn implicit_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Member<'db>, Self::Error> {
        Ok(implicit_attributes::implicit_attribute_bindings_with(
            request.class,
            request.name,
            MethodDecorator::ClassMethod,
            self,
        )
        .await?
        .member())
    }
    async fn is_kw_only(&self, ty: Type<'db>) -> Result<bool, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(ty.is_instance_of(self.db(), KnownClass::KwOnly))
        })
        .await
    }
    async fn is_enum_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<bool, Self::Error> {
        let metadata = self
            .sources
            .enum_metadata(self.endpoint, request.class.into())
            .await?;
        self.local(|| {
            self.admit_local_work(request.name.len())?;
            Ok(metadata.is_some_and(|metadata| metadata.contains_member(request.name)))
        })
        .await
    }
    async fn is_enum_class(&self, _class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.refuse("is_enum_class").await
    }
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn dataclass_fields(&self) -> Result<Type<'db>, Self::Error> {
        self.refuse("dataclass_fields").await
    }
    async fn named_tuple_field(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("named_tuple_field").await
    }
    async fn named_tuple_property(&self, _field_type: Type<'db>) -> Result<Type<'db>, Self::Error> {
        self.refuse("named_tuple_property").await
    }
    async fn dunder_paramspec(&self, _ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        self.refuse("dunder_paramspec").await
    }
    async fn constructor_context(
        &self,
        _function: FunctionType<'db>,
        _context: GenericContext<'db>,
    ) -> Result<FunctionType<'db>, Self::Error> {
        self.refuse("constructor_context").await
    }
    async fn slot_descriptor(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("slot_descriptor").await
    }
    async fn synthesized_member(
        &self,
        request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        synthesized_member::own_synthesized_member_with(request, self).await
    }
    async fn nonmember_value(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        crate::types::enums::nonmember_value_with(ty, self).await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> SynthesizedMemberEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn total_ordering(&self, value: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                value
                    .field_requests(self.endpoint.field_request_context())
                    .total_ordering(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }
    type Error = RunError;
    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<CodeGeneratorKind<'db>>, Self::Error> {
        class::static_code_generator_with(class, self).await
    }
    async fn checkpoint(&self, work: SynthesizedMemberWork) -> Result<(), Self::Error> {
        self.work(match work {
            SynthesizedMemberWork::Admission { name_bytes } => name_bytes,
            _ => 0,
        })
        .await
    }
    async fn total_ordering_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("total_ordering_member").await
    }
    async fn frozen_subclass_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
        _method: FrozenDataclassMethod,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("frozen_subclass_member").await
    }
    async fn generated_member(
        &self,
        _request: OwnMemberLookupRequest<'_, 'db>,
        _field_policy: CodeGeneratorKind<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("generated_member").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> ClassInstanceStorageEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn alias_origin(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<StaticClassLiteral<'db>, Self::Error> {
        ClassTypeOwnMemberEffects::alias_origin(self, alias).await
    }
    async fn alias_specialization(
        &self,
        alias: GenericAlias<'db>,
    ) -> Result<Specialization<'db>, Self::Error> {
        ClassTypeOwnMemberEffects::alias_specialization(self, alias).await
    }
    type Error = RunError;
    async fn checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn dynamic_instance_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: DynamicClassLiteral<'db>,
        _name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.refuse("dynamic_instance_member").await
    }
    async fn named_tuple_instance_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: DynamicNamedTupleLiteral<'db>,
        _name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.refuse("named_tuple_instance_member").await
    }
    async fn enum_instance_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: DynamicEnumLiteral<'db>,
        _name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.refuse("enum_instance_member").await
    }
    async fn dynamic_own_instance_member(
        &self,
        _class: DynamicClassLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("dynamic_own_instance_member").await
    }
    async fn named_tuple_own_instance_member(
        &self,
        _class: DynamicNamedTupleLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("named_tuple_own_instance_member").await
    }
    async fn enum_own_instance_member(
        &self,
        _class: DynamicEnumLiteral<'db>,
        _name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        self.refuse("enum_own_instance_member").await
    }
    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        instance_storage::static_is_typed_dict_with(class, self).await
    }
    async fn static_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        instance_storage::static_instance_member_with(env, class, specialization, name, self).await
    }
    async fn static_own_instance_member(
        &self,
        env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        member_source::static_own_instance_member_with(env, class, name, self).await
    }
    async fn specialize_place(
        &self,
        member: PlaceAndQualifiers<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.work(0).await?;
        if specialization.is_none() {
            Ok(member)
        } else {
            self.refuse("specialize_place").await
        }
    }
    async fn specialize_member(
        &self,
        member: Member<'db>,
        specialization: Option<Specialization<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        self.work(0).await?;
        if specialization.is_none() {
            Ok(member)
        } else {
            self.refuse("specialize_member").await
        }
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> StaticInstanceStorageEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn storage_checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        instance_storage::static_is_typed_dict_with(class, self).await
    }
    async fn lacks_instance_storage(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> Result<bool, Self::Error> {
        slots::lacks_instance_storage_with(class, name, self).await
    }
    async fn mro_instance_member(
        &self,
        _env: &ProgramEnvironment<'db>,
        class: StaticClassLiteral<'db>,
        specialization: Option<Specialization<'db>>,
        name: &str,
    ) -> Result<InstanceMemberResult<'db>, Self::Error> {
        self.work(0).await?;
        if specialization.is_some() {
            return self.refuse("specialized_instance_mro").await;
        }
        let cursor = self
            .sources
            .mro_start(self.endpoint, ClassType::NonGeneric(class.into()))
            .await?;
        mro_members::mro_instance_member_with(name, cursor, self).await
    }
    async fn typed_dict_fallback(
        &self,
        _env: &ProgramEnvironment<'db>,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.refuse("typed_dict_fallback").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> InstanceClassificationEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn known(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .known(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    async fn has_explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .has_explicit_bases(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }

    type Error = RunError;
    async fn checkpoint(&self, work: InstanceStorageWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn instance_flags(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Self::Error> {
        instance_flags::queued_instance_flags_with(class, InstanceFlagFacts, self).await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> MroMemberEffects<'db, MroCursor<'db>> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn known_class(&self, class: ClassType<'db>) -> RunResult<Option<KnownClass>> {
        self.local(|| {
            Ok(self
                .fields
                .static_class_literal(class)
                .and_then(|(class, _)| self.fields.static_known(class)))
        })
        .await
    }

    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<MroImplicitAttribute<'db>>, Self::Error> {
        self.work(0).await?;
        match self
            .local(|| Ok(self.fields.static_class_literal(class)))
            .await?
        {
            Some((class, _)) => Ok(Some(MroImplicitAttribute::from_attribute(
                implicit_attributes::implicit_attribute_bindings_with(
                    class,
                    name,
                    MethodDecorator::ClassMethod,
                    self,
                )
                .await?,
            ))),
            None => Ok(None),
        }
    }
    async fn checkpoint(&self, work: MroMemberWork) -> Result<(), Self::Error> {
        self.work(match work {
            MroMemberWork::PushAugmented { prefix_len } => prefix_len,
            MroMemberWork::InferAugmented { pending_len }
            | MroMemberWork::ClearAugmented { pending_len } => pending_len,
            _ => 0,
        })
        .await
    }
    async fn advance(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.sources.mro_next(self.endpoint, cursor).await
    }
    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
        context: Option<GenericContext<'db>>,
    ) -> Result<Member<'db>, Self::Error> {
        own_member::class_type_own_member_with(
            ClassTypeOwnMemberRequest {
                class,
                name,
                inherited_generic_context: context,
            },
            self,
        )
        .await
    }
    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error> {
        self.push_mro_pending(pending, class, bindings).await
    }
    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        self.clear_mro_pending(pending).await
    }
    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        self.finish_mro_pending(pending).await
    }
    async fn infer_augmented(
        &self,
        _bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error> {
        self.refuse("infer_augmented").await
    }
    async fn union_augmented(
        &self,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("union_augmented").await
    }
    async fn fall_back_to(
        &self,
        prior: LookupError<'db>,
        member: PlaceAndQualifiers<'db>,
    ) -> Result<LookupResult<'db>, Self::Error> {
        self.work(0).await?;
        prior
            .or_fall_back_to_with(self.db(), &self.env, self, member)
            .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> MemberFinalizationEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self, work: MemberFinalizationWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn intersect_dynamic(
        &self,
        _ty: Type<'db>,
        _dynamic: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("intersect_dynamic").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> InstanceMroEffects<'db, MroCursor<'db>> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self, work: InstanceMroWork) -> Result<(), Self::Error> {
        self.work(match work {
            InstanceMroWork::PushAugmented { prefix_len } => prefix_len,
            InstanceMroWork::InferAugmented { pending_len }
            | InstanceMroWork::ClearAugmented { pending_len } => pending_len,
            _ => 0,
        })
        .await
    }
    async fn new_union(&self) -> Result<UnionBuilder<'db>, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(UnionBuilder::new(self.db(), &self.env))
        })
        .await
    }
    async fn advance(
        &self,
        cursor: &mut MroCursor<'db>,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.sources.mro_next(self.endpoint, cursor).await
    }
    async fn own_instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        instance_storage::class_own_instance_member_with(&self.env, class, name, self).await
    }
    async fn implicit_attribute(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Option<ImplicitAttribute<'db>>, Self::Error> {
        self.work(0).await?;
        match self
            .local(|| Ok(self.fields.static_class_literal(class)))
            .await?
        {
            Some((class, _)) => Ok(Some(
                implicit_attributes::implicit_attribute_bindings_with(
                    class,
                    name,
                    MethodDecorator::None,
                    self,
                )
                .await?,
            )),
            None => Ok(None),
        }
    }
    async fn push_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
        class: ClassType<'db>,
        bindings: AugmentedBindings<'db>,
    ) -> Result<(), Self::Error> {
        self.push_mro_pending(pending, class, bindings).await
    }
    async fn clear_pending(
        &self,
        pending: &mut Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        self.clear_mro_pending(pending).await
    }
    async fn finish_pending(
        &self,
        pending: Vec<(ClassType<'db>, AugmentedBindings<'db>)>,
    ) -> Result<(), Self::Error> {
        self.finish_mro_pending(pending).await
    }
    async fn infer_augmented(
        &self,
        _bindings: MroPendingBindings<'_, 'db>,
    ) -> Result<(Type<'db>, Provenance<'db>), Self::Error> {
        self.refuse("infer_augmented").await
    }
    async fn union_add(
        &self,
        _union: UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error> {
        self.refuse("union_add").await
    }
    async fn own_class_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<Member<'db>, Self::Error> {
        own_member::class_type_own_member_with(
            ClassTypeOwnMemberRequest {
                class,
                name,
                inherited_generic_context: None,
            },
            self,
        )
        .await
    }
    async fn is_definitely_non_data_descriptor(&self, _ty: Type<'db>) -> Result<bool, Self::Error> {
        self.refuse("is_definitely_non_data_descriptor").await
    }
    async fn union_build(&self, _union: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        self.refuse("union_build").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> PublicLookupEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn promote_public_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("promote_public_type").await
    }
    async fn union_two(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _first: Type<'db>,
        _second: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("union_two").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> NominalClassEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn non_tuple_class(&self, class: NominalInstanceClass<'db>) -> RunResult<ClassType<'db>> {
        self.local(|| Ok(NominalClassFacts.class(salsa::FieldReads::new(self.db()), class)))
            .await
    }
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn tuple_class(&self, _tuple: TupleType<'db>) -> Result<ClassType<'db>, Self::Error> {
        self.refuse("tuple_class").await
    }
    async fn version_class(&self) -> Result<Option<ClassType<'db>>, Self::Error> {
        self.refuse("version_class").await
    }
    async fn object_class(&self) -> Result<ClassType<'db>, Self::Error> {
        self.work(0).await?;
        let ty = self
            .sources
            .known_class(self.endpoint, self.program(), KnownClass::Object)
            .await?;
        match self.local(|| Ok(ty.to_class_type(self.db()))).await? {
            Some(class) => Ok(class),
            None => self.refuse("missing_object_class").await,
        }
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> SubclassConstructionEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn is_final(&self, class: ClassType<'db>) -> Result<bool, Self::Error> {
        self.work(0).await?;
        match self
            .local(|| Ok(self.fields.static_class_literal(class)))
            .await?
        {
            Some((class, _)) => static_literal::static_finality_with(class, self).await,
            None => self.refuse("dynamic_finality").await,
        }
    }
    async fn is_object(&self, class: ClassType<'db>) -> Result<bool, Self::Error> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(self.fields.is_object(class))
        })
        .await
    }
    async fn subclass_of_object(&self) -> Result<Type<'db>, Self::Error> {
        self.sources
            .known_instance(self.endpoint, self.program(), KnownClass::Type)
            .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> StaticFinalityEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn has_decorators(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.local(|| Ok(self.fields.has_decorators(class))).await
    }
    async fn has_final_decorator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.work(0).await?;
        for decorator in self.sources.decorators(self.endpoint, class).await? {
            self.work(0).await?;
            if self
                .local(|| {
                    Ok(decorator
                        .as_function_literal()
                        .and_then(|function| function.known(self.db())))
                })
                .await?
                == Some(KnownFunction::Final)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    async fn has_enum_metadata(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        Ok(self
            .sources
            .enum_metadata(self.endpoint, class.into())
            .await?
            .is_some())
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> QueuedInstanceFlags<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    type Cursor = ();
    async fn known_class(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        self.local(|| Ok(InstanceFlagFacts.known(self.fields, class)))
            .await
    }
    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.local(|| Ok(InstanceFlagFacts.explicit_bases(self.fields, class)))
            .await
    }
    async fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.local(|| Ok(InstanceFlagFacts.explicit_metaclass(self.fields, class)))
            .await
    }
    async fn checkpoint(&self, work: InstanceFlagsWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn inherited_flags(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<ClassInstanceFlags, Self::Error> {
        self.refuse("inherited_flags").await
    }
    async fn own_attribute(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        instance_flags::queued_own_getattribute_with(class, self).await
    }
    async fn has_own_symbol(&self, class: StaticClassLiteral<'db>) -> Result<bool, Self::Error> {
        self.work(0).await?;
        let table = self
            .sources
            .places(
                self.endpoint,
                self.local(|| Ok(self.fields.body_scope(class))).await?,
            )
            .await?;
        Ok(
            MemberSourceEffects::symbol_id(self, table, "__getattribute__")
                .await?
                .is_some(),
        )
    }
    async fn metaclass_custom_getattribute(
        &self,
        _class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.refuse("metaclass_custom_getattribute").await
    }
    async fn start_mro(&self, _class: StaticClassLiteral<'db>) -> RunResult<Self::Cursor> {
        self.refuse("inherited_flags").await
    }
    async fn next_base(&self, _cursor: &mut Self::Cursor) -> RunResult<Option<ClassBase<'db>>> {
        self.refuse("inherited_flags").await
    }
    async fn static_class_literal(
        &self,
        _class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.refuse("inherited_flags").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> ImplicitNameSearchControl for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    fn comparison(&self, candidate_bytes: usize, requested_bytes: usize) -> RunResult<()> {
        let units = candidate_bytes
            .checked_add(requested_bytes)
            .and_then(|units| units.checked_add(1))
            .ok_or(RunError::Refused(
                salsa::attempt_probe::Incomplete::Allowance,
            ))?;
        self.endpoint.admit_work(units)
    }
}
impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> crate::types::enums::NonmemberValueEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self) -> RunResult<()> {
        self.work(0).await
    }
    async fn is_nonmember(&self, ty: Type<'db>) -> RunResult<bool> {
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(ty.is_instance_of(self.db(), KnownClass::Nonmember))
        })
        .await
    }
    async fn value(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        let result = self
            .queries
            .member(
                self.endpoint,
                self.program(),
                ty,
                "value",
                MemberLookupPolicy::default(),
            )
            .await?;
        self.local(|| {
            self.admit_local_work(0)?;
            Ok(result
                .unwrap_or_else(|error| error.fallback_member(self.db()))
                .member(self.db())
                .place
                .ignore_possibly_undefined()
                .unwrap_or(Type::unknown()))
        })
        .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> NamespaceLookupEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    type DynamicCursor = MroCursor<'db>;
    async fn checkpoint(&self, _work: NamespaceLookupWork) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn find_in_mro(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<Option<PlaceAndQualifiers<'db>>, Self::Error> {
        self.find_mro(ty, name, policy).await
    }
    async fn inferred_metaclass(
        &self,
        class: ClassType<'db>,
    ) -> Result<ClassMetaclass<'db>, Self::Error> {
        let eligible = self
            .local(|| {
                self.admit_local_work(0)?;
                Ok(self.fields.static_class_literal(class).is_some_and(
                    |(class, specialization)| {
                        specialization.is_none() && class.has_default_metaclass(self.db())
                    },
                ))
            })
            .await?;
        if eligible {
            Ok(ClassMetaclass::Selected(
                self.sources
                    .known_class(self.endpoint, self.program(), KnownClass::Type)
                    .await?,
            ))
        } else {
            self.refuse("metaclass_selection").await
        }
    }
    async fn for_inheritance(
        &self,
        metaclass: ClassMetaclass<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.work(0).await?;
        match metaclass {
            ClassMetaclass::Selected(ty) => Ok(ty),
            ClassMetaclass::ProtocolFallback => {
                self.sources
                    .known_class(self.endpoint, self.program(), KnownClass::Type)
                    .await
            }
        }
    }
    async fn instance_approximation(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.work(0).await?;
        match self.local(|| Ok(ty.to_class_type(self.db()))).await? {
            Some(class) => Ok(Some(
                Type::instance_with(self.db(), &self.env, self, class).await?,
            )),
            None => self.refuse("instance_approximation").await,
        }
    }
    async fn nominal_class(&self, ty: Type<'db>) -> Result<Option<ClassType<'db>>, Self::Error> {
        self.work(0).await?;
        match ty {
            Type::NominalInstance(instance) => Ok(Some(
                instance::nominal_class_with(instance, NominalClassFacts, self).await?,
            )),
            _ => self.refuse("nominal_class").await,
        }
    }
    async fn instance_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        instance_storage::class_instance_member_with(&self.env, class, name, self).await
    }
    async fn own_member(
        &self,
        _request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.refuse("own_member").await
    }
    async fn runtime_binding_absent(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> Result<bool, Self::Error> {
        self.work(0).await?;
        match self
            .local(|| Ok(self.fields.static_class_literal(class)))
            .await?
        {
            Some((class, _)) => {
                member_source::runtime_binding_absent_with(
                    &self.env,
                    self.local(|| Ok(self.fields.body_scope(class))).await?,
                    name,
                    self,
                )
                .await
            }
            None => Ok(false),
        }
    }
    async fn inherited_member(
        &self,
        _request: NamespaceLookupRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.refuse("inherited_member").await
    }
    async fn fall_back_to(
        &self,
        _member: PlaceAndQualifiers<'db>,
        _fallback: PlaceAndQualifiers<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.refuse("fall_back_to").await
    }
    async fn start_dynamic_mro(
        &self,
        class: ClassType<'db>,
    ) -> Result<Self::DynamicCursor, Self::Error> {
        self.sources.mro_start(self.endpoint, class).await
    }
    async fn next_dynamic_base(
        &self,
        cursor: &mut Self::DynamicCursor,
    ) -> Result<Option<ClassBase<'db>>, Self::Error> {
        self.sources.mro_next(self.endpoint, cursor).await
    }
    async fn may_be_data_descriptor(&self, _ty: Type<'db>) -> Result<bool, Self::Error> {
        self.refuse("may_be_data_descriptor").await
    }
    async fn filter_possible_data_descriptors(
        &self,
        _ty: Type<'db>,
    ) -> Result<(Type<'db>, bool), Self::Error> {
        self.refuse("filter_possible_data_descriptors").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> InstanceEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;

    async fn class_literal_and_specialization(
        &self,
        _db: &'db dyn Db,
        class: ClassType<'db>,
    ) -> RunResult<(ClassLiteral<'db>, Option<Specialization<'db>>)> {
        match class {
            ClassType::NonGeneric(literal) => Ok((literal, None)),
            ClassType::Generic(alias) => {
                let fields = self.endpoint.field_request_context();
                let origin = self
                    .endpoint
                    .read_field(
                        alias.field_requests(fields).origin(),
                        &salsa::execution_probe::BorrowOrCopy,
                    )
                    .await;
                let specialization = self
                    .endpoint
                    .read_field(
                        alias.field_requests(fields).specialization(),
                        &salsa::execution_probe::BorrowOrCopy,
                    )
                    .await;
                Ok((ClassLiteral::Static(origin), Some(specialization)))
            }
        }
    }

    async fn known_class(
        &self,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<KnownClass>> {
        Ok(self
            .endpoint
            .read_field(
                class
                    .field_requests(self.endpoint.field_request_context())
                    .known(),
                &salsa::execution_probe::BorrowOrCopy,
            )
            .await)
    }
    async fn checkpoint(&self, _work: InstanceWork) -> RunResult<()> {
        self.endpoint
            .local_call(|| self.endpoint.admit_work(1))
            .await;
        Ok(())
    }
    async fn is_typed_dict(
        &self,
        _db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        instance_storage::static_is_typed_dict_with(class, self).await
    }
    async fn is_protocol(
        &self,
        db: &'db dyn Db,
        class: StaticClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.work(0).await?;
        match self
            .local(|| Ok(class.is_protocol_without_inference(self.db())))
            .await?
        {
            Some(value) => Ok(value),
            None => {
                let bases = self.sources.explicit_bases(self.endpoint, class).await?;
                self.work(bases.len()).await?;
                Ok(StaticClassLiteral::protocol_explicit_bases(bases))
            }
        }
    }
    async fn inherits_from_explicit_any(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> Result<bool, Self::Error> {
        self.work(0).await?;
        match class {
            ClassLiteral::Static(class) => {
                Ok(
                    instance_flags::queued_instance_flags_with(class, InstanceFlagFacts, self)
                        .await?
                        .contains(ClassInstanceFlags::INHERITS_FROM_EXPLICIT_ANY),
                )
            }
            _ => self.refuse("dynamic_instance_flags").await,
        }
    }
    async fn tuple(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _specialization: Option<Specialization<'db>>,
    ) -> Result<TupleType<'db>, Self::Error> {
        self.refuse("tuple").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> FunctionDescriptorEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn union(
        &self,
        union: UnionType<'db>,
        _instance: Option<Type<'db>>,
        _owner: Option<Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("union").await
    }
    async fn alias(
        &self,
        alias: crate::types::TypeAliasType<'db>,
        _instance: Option<Type<'db>>,
        _owner: Option<Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("alias").await
    }
    async fn bind(
        &self,
        _ty: Type<'db>,
        _env: &ProgramEnvironment<'db>,
        _instance: Option<Type<'db>>,
        _owner: Option<Type<'db>>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("bind").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> TypeDispatchEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        self.work(0).await?;
        match ty {
            Type::TypeAlias(_) | Type::Recursive(_) | Type::RecursiveVar(_) => {
                self.refuse("alias_resolution").await
            }
            _ => Ok(ty),
        }
    }
    async fn newtype_union(
        &self,
        _newtype: NewType<'db>,
    ) -> Result<Option<UnionType<'db>>, Self::Error> {
        self.refuse("newtype_union").await
    }
    async fn collect_properties(
        &self,
        _ty: Type<'db>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Self::Error> {
        self.refuse("collect_properties").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> DescriptorEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn slot_value(
        &self,
        _db: &'db dyn Db,
        descriptor: SlotDescriptorType<'db>,
    ) -> RunResult<Type<'db>> {
        self.local(|| Ok(descriptor.value_type(self.db()))).await
    }

    async fn union_parts(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _union: UnionType<'db>,
    ) -> RunResult<(UnionBuilder<'db>, &'db [Type<'db>])> {
        self.refuse("descriptor_union_parts").await
    }

    async fn intersection_parts(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<(IntersectionBuilder<'db>, &'db FxOrderSet<Type<'db>>)> {
        self.refuse("descriptor_intersection_parts").await
    }

    async fn next_descriptor(
        &self,
        requests: &mut impl Iterator<Item = DescriptorRequest<'db>>,
    ) -> RunResult<Option<DescriptorRequest<'db>>> {
        self.local(|| Ok(requests.next())).await
    }

    async fn call_context(
        &self,
        _db: &'db dyn Db,
        _request: DescriptorRequest<'db>,
        _callable: Type<'db>,
    ) -> RunResult<DescriptorGetCallContext<'db>> {
        self.refuse("descriptor_call_context").await
    }

    async fn function_like(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        crate::types::callable::function_descriptor_with(
            request.ty,
            env,
            request.instance,
            Some(request.owner),
            self,
        )
        .await
    }
    async fn protocol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Self::Error> {
        self.queries
            .descriptor_get(
                self.endpoint,
                self.local(|| Ok(env.program(self.db()))).await?,
                request,
            )
            .await
    }
    async fn declare_descriptors(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _requests: impl Iterator<Item = DescriptorRequest<'db>> + Clone,
    ) -> Result<(), Self::Error> {
        self.refuse("declare_descriptors").await
    }
    async fn descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Self::Error> {
        self.work(0).await?;
        crate::types::descriptor::evaluate_entry_with_effects(self.db(), env, request, self).await
    }
    async fn union_like(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<UnionType<'db>>, Self::Error> {
        crate::types::union_like_with(ty, self).await
    }
    async fn class_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorMemberRequest<'db>,
    ) -> Result<Place<'db>, Self::Error> {
        Ok(self
            .queries
            .class_lookup(
                self.endpoint,
                self.local(|| Ok(env.program(self.db()))).await?,
                request.ty,
                "__get__",
                request.policy,
            )
            .await?
            .place)
    }
    async fn data_descriptor(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        self.refuse("data_descriptor").await
    }
    async fn none_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("none_type").await
    }
    async fn invoke(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _request: DescriptorInvocationRequest<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Self::Error> {
        self.refuse("invoke").await
    }
    async fn bindings_origin(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: &Bindings<'db>,
        _arguments: &[Type<'db>; 3],
    ) -> Result<DescriptorOrigin<'db>, Self::Error> {
        self.refuse("bindings_origin").await
    }
    async fn bindings_return_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("bindings_return_type").await
    }
    async fn merge_origins(
        &self,
        _db: &'db dyn Db,
        _left: DescriptorOrigin<'db>,
        _right: DescriptorOrigin<'db>,
    ) -> Result<DescriptorOrigin<'db>, Self::Error> {
        self.refuse("merge_origins").await
    }
    async fn union_add(
        &self,
        _builder: UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error> {
        self.refuse("union_add").await
    }
    async fn union_build(&self, _builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        self.refuse("union_build").await
    }
    async fn union_pair(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("union_pair").await
    }
    async fn intersection_add(
        &self,
        _builder: IntersectionBuilder<'db>,
        _ty: Type<'db>,
    ) -> Result<IntersectionBuilder<'db>, Self::Error> {
        self.refuse("intersection_add").await
    }
    async fn intersection_build(
        &self,
        _builder: IntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error> {
        self.refuse("intersection_build").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> LookupDescriptorEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn fallback_parts(&self, result: MemberLookupResult<'db>) -> RunResult<LookupParts<'db>> {
        self.local(|| Ok(LookupFacts.parts(salsa::FieldReads::new(self.db()), result)))
            .await
    }
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn class_attribute(
        &self,
        key: MemberLookupKey<'db>,
        _receiver: Type<'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        let (program, ty, name, policy) = self
            .local(|| {
                Ok((
                    key.program(self.db()),
                    key.ty(self.db()),
                    key.name(self.db()),
                    key.policy(self.db()),
                ))
            })
            .await?;
        self.work(name.len()).await?;
        if name == "__dict__" || matches!(ty, Type::TypeVar(_)) {
            return self.refuse("receiver_class_attribute").await;
        }
        self.queries
            .class_lookup(self.endpoint, program, ty, name, policy)
            .await
    }
    async fn owner(&self, receiver: Type<'db>) -> Result<Type<'db>, Self::Error> {
        self.meta(receiver).await
    }
    async fn descriptor(
        &self,
        request: descriptor::DescriptorRequest<'db>,
    ) -> Result<descriptor::DescriptorResult<'db>, Self::Error> {
        self.work(0).await?;
        crate::types::descriptor::evaluate_entry_with_effects(self.db(), &self.env, request, self)
            .await
    }
    async fn properties(
        &self,
        ty: Type<'db>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Self::Error> {
        crate::types::property_metadata_with(ty, self).await
    }
    async fn union(&self, _first: Type<'db>, _second: Type<'db>) -> Result<Type<'db>, Self::Error> {
        self.refuse("union").await
    }
    async fn merge_properties(
        &self,
        _first: Option<PropertyDeprecations<'db>>,
        _second: Option<PropertyDeprecations<'db>>,
    ) -> Result<Option<PropertyDeprecations<'db>>, Self::Error> {
        self.refuse("merge_properties").await
    }
    async fn merge_origins(
        &self,
        _first: DescriptorOrigin<'db>,
        _second: DescriptorOrigin<'db>,
    ) -> Result<DescriptorOrigin<'db>, Self::Error> {
        self.refuse("merge_origins").await
    }
    async fn result(
        &self,
        parts: LookupParts<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        self.work(0).await?;
        if parts.error.is_some()
            || parts.properties.is_some()
            || parts.descriptor != DescriptorOrigin::default()
        {
            return self.refuse("member_metadata_interning").await;
        }
        self.local(|| {
            Ok(crate::types::member_lookup_result_with_origin(
                self.db(),
                parts.member,
                parts.error,
                parts.properties,
                parts.descriptor,
            ))
        })
        .await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> MemberEntryEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<(Type<'db>, &'db Name, MemberLookupPolicy)> {
        self.local(|| {
            let fields = salsa::FieldReads::new(self.db());
            Ok((
                LookupFacts.key_type(fields, key),
                LookupFacts.key_name(fields, key),
                LookupFacts.key_policy(fields, key),
            ))
        })
        .await
    }
    async fn suppress_typed_dict_classvar(
        &self,
        ty: Type<'db>,
        result: MemberLookupResult<'db>,
    ) -> RunResult<bool> {
        self.local(|| {
            Ok(LookupFacts.suppress_typed_dict_classvar(
                salsa::FieldReads::new(self.db()),
                ty,
                result,
            ))
        })
        .await
    }
    async fn checkpoint(&self) -> Result<(), Self::Error> {
        self.work(0).await
    }
    async fn meta_type(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        self.meta(ty).await
    }
    async fn nominal_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassType<'db>, Self::Error> {
        instance::nominal_class_with(instance, NominalClassFacts, self).await
    }
    async fn namespace(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        namespace::namespace_lookup_with(
            ty,
            NamespaceLookupRequest {
                class,
                name,
                policy,
            },
            self,
        )
        .await
    }
    async fn enum_member(
        &self,
        ty: Type<'db>,
        _name: &Name,
    ) -> Result<Option<MemberLookupResult<'db>>, Self::Error> {
        self.work(0).await?;
        let Type::NominalInstance(instance) = ty else {
            return self.refuse("non_nominal_enum_lookup").await;
        };
        let class = instance::nominal_class_with(instance, NominalClassFacts, self).await?;
        self.work(0).await?;
        if self
            .sources
            .enum_class(
                self.endpoint,
                self.local(|| Ok(self.fields.class_literal(class))).await?,
            )
            .await?
            .is_some()
        {
            return self.refuse("enum_member_construction").await;
        }
        Ok(None)
    }
    async fn instance_storage(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.work(0).await?;
        let Type::NominalInstance(instance) = ty else {
            return self.refuse("non_nominal_instance_storage").await;
        };
        let class = instance::nominal_class_with(instance, NominalClassFacts, self).await?;
        instance_storage::class_instance_member_with(&self.env, class, name, self).await
    }
    async fn invoke_descriptor(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        crate::types::invoke_lookup_descriptor_with(
            key,
            receiver,
            fallback,
            InstanceFallbackShadowsNonDataDescriptor::No,
            LookupFacts,
            self,
        )
        .await
    }
    async fn fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        self.fallback(ty, name, result, policy).await
    }
    async fn bind_self(
        &self,
        result: MemberLookupResult<'db>,
        receiver: Type<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        self.bind_result(result, receiver).await
    }
    async fn promote(
        &self,
        result: MemberLookupResult<'db>,
    ) -> Result<MemberLookupResult<'db>, Self::Error> {
        self.work(0).await?;
        let parts = self
            .local(|| Ok(LookupFacts.parts(salsa::FieldReads::new(self.db()), result)))
            .await?;
        if matches!(
            parts.member.place,
            Place::Defined(crate::place::DefinedPlace {
                origin: crate::place::TypeOrigin::Inferred,
                ..
            })
        ) && !parts.member.qualifiers.contains(TypeQualifiers::FINAL)
        {
            self.refuse("inferred_literal_promotion").await
        } else {
            Ok(result)
        }
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> DunderEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;
    async fn admit(&self, _work: DunderWork) -> RunResult<()> {
        Ok(self
            .endpoint
            .local_call(|| {
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted,
                ))
            })
            .await)
    }
    async fn read<T>(&self, _read: DunderRead, _operation: impl FnOnce() -> T) -> RunResult<T> {
        Ok(self
            .endpoint
            .local_call(|| {
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted,
                ))
            })
            .await)
    }
    async fn finite_alternatives(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _intersection: IntersectionType<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error> {
        self.refuse("finite_alternatives").await
    }
    async fn lookup(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DunderCallRequest<'_, 'db>,
    ) -> Result<PlaceAndQualifiers<'db>, Self::Error> {
        self.work(0).await?;
        match request.lookup {
            DunderLookup::Implicit(policy) => {
                let result = self
                    .queries
                    .member(
                        self.endpoint,
                        self.local(|| Ok(env.program(self.db()))).await?,
                        request.receiver,
                        request.name,
                        policy | MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                    )
                    .await?;
                self.local(|| {
                    self.admit_local_work(0)?;
                    Ok(result
                        .unwrap_or_else(|error| error.fallback_member(self.db()))
                        .member(self.db()))
                })
                .await
            }
            DunderLookup::OnClass => self.refuse("dunder_on_class").await,
        }
    }
    async fn bindings(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _callable: Type<'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        self.refuse("bindings").await
    }
    async fn match_parameters(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: Bindings<'db>,
        _arguments: &CallArguments<'_, 'db>,
    ) -> Result<Bindings<'db>, Self::Error> {
        self.refuse("match_parameters").await
    }
    async fn check_types(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: Bindings<'db>,
        _arguments: &CallArguments<'_, 'db>,
        _tcx: TypeContext<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Self::Error> {
        self.refuse("check_types").await
    }
    async fn call(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _request: DunderCallRequest<'_, 'db>,
        _arguments: &CallArguments<'_, 'db>,
    ) -> Result<DunderCallResult<'db>, Self::Error> {
        self.refuse("call").await
    }
    async fn union_add(
        &self,
        _builder: UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error> {
        self.refuse("union_add").await
    }
    async fn union_build(&self, _builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error> {
        self.refuse("union_build").await
    }
    async fn merge_intersection(
        &self,
        _receiver: Type<'db>,
        _bindings: Vec<Bindings<'db>>,
    ) -> Result<Bindings<'db>, Self::Error> {
        self.refuse("merge_intersection").await
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> MemberEffects<'call, 'run, 'db, Q, S>
{
    async fn meta(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.work(0).await?;
        match ty {
            Type::NominalInstance(instance) => {
                let class = instance::nominal_class_with(instance, NominalClassFacts, self).await?;
                subclass_of::subclass_from_with(
                    SubclassOfInner::Class(class),
                    SubclassConstructionFacts,
                    self,
                )
                .await
            }
            _ => self.refuse("non_nominal_metatype").await,
        }
    }
    async fn class_mro(
        &self,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.work(name.len()).await?;
        let Some((literal, specialization)) = self
            .local(|| Ok(self.fields.static_class_literal(class)))
            .await?
        else {
            return self.refuse("dynamic_class_mro").await;
        };
        let context = self.sources.context(self.endpoint, literal).await?;
        if context.is_some() || specialization.is_some() {
            return self.refuse("generic_class_mro").await;
        }
        let cursor = self.sources.mro_start(self.endpoint, class).await?;
        let result = mro_members::mro_class_member_with(
            MroClassMemberRequest {
                name,
                policy,
                inherited_generic_context: context,
                is_self_object: self.local(|| Ok(self.fields.is_object(class))).await?,
            },
            cursor,
            self,
        )
        .await?;
        let class::ClassMemberResult::Done(result) = result else {
            return self.refuse("typed_dict_class_mro").await;
        };
        let member = mro_members::finalize_class_member_with(result, self).await?;
        self.work(name.len()).await?;
        if name.starts_with("__") && name.ends_with("__") && !member.place.is_undefined() {
            return self.refuse("dunder_callable_conversion").await;
        }
        Ok(member)
    }
    async fn find_mro(
        &self,
        mut ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.work(name.len()).await?;
        if let Type::SubclassOf(subclass) = ty {
            match subclass.subclass_of() {
                SubclassOfInner::Class(class) => ty = Type::from(class),
                _ => return self.refuse("non_class_subclass_mro").await,
            }
        }
        if let Type::NominalInstance(instance) = ty
            && self
                .local(|| Ok(instance.has_known_class(self.db(), KnownClass::Type)))
                .await?
        {
            if policy.mro_no_object_fallback() {
                return Ok(Some(Place::Undefined.into()));
            }
            ty = self
                .sources
                .known_class(self.endpoint, self.program(), KnownClass::Object)
                .await?;
        }
        self.work(0).await?;
        let Some(class) = self.local(|| Ok(ty.to_class_type(self.db()))).await? else {
            return self.refuse("non_class_mro").await;
        };
        let Some((literal, _)) = self
            .local(|| Ok(self.fields.static_class_literal(class)))
            .await?
        else {
            return self.refuse("dynamic_class_mro").await;
        };
        if instance_storage::static_is_typed_dict_with(literal, self).await? {
            return self.refuse("typed_dict_class_mro").await;
        }
        self.work(0).await?;
        if let Some(member) = self
            .local(|| {
                Ok(crate::types::native_class_mro_attribute(
                    self.fields.static_known(literal),
                    name,
                ))
            })
            .await?
        {
            return Ok(Some(member));
        }
        let member = self.class_mro(class, name, policy).await?;
        self.work(0).await?;
        if crate::types::property_wrapper_kind(name).is_some()
            && matches!(member.place.raw_type(), Some(Type::FunctionLiteral(_)))
        {
            return self.refuse("property_wrapper_descriptor").await;
        }
        Ok(Some(member))
    }
    async fn bind_result(
        &self,
        result: MemberLookupResult<'db>,
        receiver: Type<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.work(0).await?;
        let parts = self
            .local(|| Ok(LookupFacts.parts(salsa::FieldReads::new(self.db()), result)))
            .await?;
        let Some(ty) = parts.member.place.raw_type() else {
            return Ok(result);
        };
        let bound = self
            .local(|| {
                ty.try_bind_self_typevars(
                    self.db(),
                    &self.env,
                    receiver,
                    &mut MemberSelfSearch {
                        endpoint: self.endpoint,
                    },
                    |_, _| {
                        Err(RunError::Refused(
                            salsa::attempt_probe::Incomplete::Interrupted,
                        ))
                    },
                )
            })
            .await?;
        self.work(0).await?;
        if bound == ty {
            Ok(result)
        } else {
            self.refuse("self_binding_metadata").await
        }
    }
    async fn fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.work(0).await?;
        let Type::NominalInstance(instance) = ty else {
            return self.refuse("non_nominal_getattr").await;
        };
        let class = instance::nominal_class_with(instance, NominalClassFacts, self).await?;
        self.work(0).await?;
        let Some((class, _)) = self
            .local(|| Ok(self.fields.static_class_literal(class)))
            .await?
        else {
            return self.refuse("dynamic_getattribute").await;
        };
        let flags =
            instance_flags::queued_instance_flags_with(class, InstanceFlagFacts, self).await?;
        self.work(0).await?;
        if self
            .local(|| {
                Ok(LookupFacts.getattribute_flags_affect_lookup(
                    salsa::FieldReads::new(self.db()),
                    flags,
                    result,
                ))
            })
            .await?
        {
            return self.refuse("custom_getattribute").await;
        }
        let parts = self
            .local(|| Ok(LookupFacts.parts(salsa::FieldReads::new(self.db()), result)))
            .await?;
        match crate::types::member_fallback_decision(parts.member.place) {
            crate::types::MemberFallbackDecision::Defined => Ok(result),
            crate::types::MemberFallbackDecision::PossiblyUndefined => {
                self.refuse("possibly_defined_getattr_merge").await
            }
            crate::types::MemberFallbackDecision::Missing => {
                if policy.no_getattr_lookup() {
                    return Ok(Place::Undefined.into());
                }
                let name_type = self.queries.name_literal(self.endpoint, name).await?;
                let request = DunderCallRequest::implicit(
                    ty,
                    "__getattr__",
                    TypeContext::default(),
                    MemberLookupPolicy::default(),
                );
                let result = request
                    .evaluate_with(
                        self.db(),
                        &self.env,
                        &CallArguments::positional([name_type]),
                        self,
                    )
                    .await?;
                match result {
                    Err(
                        CallDunderError::MethodNotAvailable
                        | CallDunderError::PossiblyUnbound { .. },
                    ) => Ok(Place::Undefined.into()),
                    _ => self.refuse("getattr_call_result").await,
                }
            }
        }
    }
}
struct MemberSelfSearch<'call, 'run: 'call, 'db: 'run> {
    endpoint: &'call TaskEndpoint<'run, 'db>,
}
impl crate::types::visitor::SearchControl for MemberSelfSearch<'_, '_, '_> {
    type Error = RunError;
    fn admit(&mut self, work: crate::types::visitor::SearchWork) -> RunResult<()> {
        match work {
            SearchWork::Semantic(_) => Err(RunError::Refused(
                salsa::attempt_probe::Incomplete::Interrupted,
            )),
            SearchWork::PendingFrame { held } => {
                self.endpoint
                    .admit_work(held.checked_add(1).ok_or(RunError::Refused(
                        salsa::attempt_probe::Incomplete::Allowance,
                    ))?)
            }
            _ => self.endpoint.admit_work(1),
        }
    }
}

pub(in crate::types) struct ClassMemberProvider<'run, Q, S> {
    pub(in crate::types) queries: Q,
    pub(in crate::types) sources: &'run S,
}
impl<'run, 'db: 'run, C, Q: MemberDependencies<'run, 'db>, S: MemberSourcesAccess<'run, 'db>>
    salsa::execution_probe::CallableRouteProvider<'run, 'db, C> for ClassMemberProvider<'run, Q, S>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = PlaceAndQualifiers<'a>,
        >,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        quote_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        key: C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        let program = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(key.program(db))
            })
            .await;
        let effects = MemberEffects {
            endpoint: &endpoint,
            queries: &self.queries,
            sources: self.sources,
            env: ProgramEnvironment::from_program(program),
            fields: crate::types::mro::field_reads::MroFieldReads::new(db),
        };
        let (ty, name, policy) = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok((key.ty(db), key.name(db), key.policy(db)))
            })
            .await;
        let Type::NominalInstance(instance) = ty else {
            return effects.refuse("class_member_dispatch").await;
        };
        crate::types::nominal_class_member_with(ty, instance, name, policy, &effects).await
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        _key: C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(Place::bound(Type::divergent(id)).into())
            })
            .await)
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call PlaceAndQualifiers<'db>,
        _value: PlaceAndQualifiers<'db>,
        _key: C::Input<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted,
                ))
            })
            .await)
    }
}

impl<
    'call,
    'run: 'call,
    'db: 'run,
    Q: MemberDependencies<'run, 'db>,
    S: MemberSourcesAccess<'run, 'db>,
> GeneralMemberEffects<'db> for MemberEffects<'call, 'run, 'db, Q, S>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        self.work(name.len()).await
    }

    async fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<(Type<'db>, &'db Name, MemberLookupPolicy)> {
        self.local(|| {
            self.admit_local_work(3)?;
            self.endpoint.check_completion()?;
            Ok((
                key.ty(self.db()),
                key.name(self.db()),
                key.policy(self.db()),
            ))
        })
        .await
    }

    async fn predicate(
        &self,
        predicate: GeneralMemberPredicate<'db>,
        _name: &str,
    ) -> RunResult<bool> {
        match predicate {
            GeneralMemberPredicate::FunctionLike(Type::FunctionLiteral(_)) => {
                self.refuse(GeneralMemberOperation::FunctionLike.name())
                    .await
            }
            GeneralMemberPredicate::FunctionLike(ty) => {
                self.local(|| {
                    self.admit_local_work(1)?;
                    self.endpoint.check_completion()?;
                    Ok(match ty {
                        Type::KnownInstance(KnownInstanceType::MethodWrapper(_)) => true,
                        Type::Callable(callable) => callable.is_method_like(self.db()),
                        _ => false,
                    })
                })
                .await
            }
            GeneralMemberPredicate::ConstraintSetClass(class) => {
                self.local(|| {
                    self.admit_local_work(1)?;
                    self.endpoint.check_completion()?;
                    Ok(class.is_known(self.db(), KnownClass::ConstraintSet))
                })
                .await
            }
            GeneralMemberPredicate::FunctionTypeClass(class) => {
                self.local(|| {
                    self.admit_local_work(1)?;
                    self.endpoint.check_completion()?;
                    Ok(class.is_known(self.db(), KnownClass::FunctionType))
                })
                .await
            }
            GeneralMemberPredicate::CallableFunctionOrStaticmethod(callable) => {
                self.local(|| {
                    self.admit_local_work(2)?;
                    self.endpoint.check_completion()?;
                    Ok(callable.is_function_like(self.db())
                        || callable.is_staticmethod_like(self.db()))
                })
                .await
            }
            GeneralMemberPredicate::CallableStaticOrClassmethod(callable) => {
                self.local(|| {
                    self.admit_local_work(2)?;
                    self.endpoint.check_completion()?;
                    Ok(callable.is_staticmethod_like(self.db())
                        || callable.is_classmethod_like(self.db()))
                })
                .await
            }
            _ => self.refuse(predicate.operation().name()).await,
        }
    }

    async fn wrapper_descriptor(
        &self,
        _ty: Type<'db>,
        _name: &str,
        _policy: MemberLookupPolicy,
    ) -> RunResult<Option<Type<'db>>> {
        self.refuse(GeneralMemberOperation::WrapperDescriptor.name())
            .await
    }

    async fn callable_runtime_class(
        &self,
        callable: crate::types::CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        self.local(|| {
            self.admit_local_work(1)?;
            self.endpoint.check_completion()?;
            Ok(callable.runtime_class(self.db()))
        })
        .await
    }

    async fn nominal_enum_member(
        &self,
        instance: NominalInstanceType<'db>,
        _name: &str,
    ) -> RunResult<
        Option<(
            ClassLiteral<'db>,
            &'db crate::types::enums::EnumMetadata<'db>,
        )>,
    > {
        let class = instance::nominal_class_with(instance, NominalClassFacts, self).await?;
        let literal = self
            .local(|| {
                self.admit_local_work(1)?;
                self.endpoint.check_completion()?;
                Ok(self.fields.class_literal(class))
            })
            .await?;
        if self
            .sources
            .enum_metadata(self.endpoint, literal)
            .await?
            .is_some()
        {
            return self
                .refuse(GeneralMemberOperation::NominalEnumMember.name())
                .await;
        }
        Ok(None)
    }

    async fn execute(
        &self,
        branch: GeneralMemberBranch<'db>,
        key: MemberLookupKey<'db>,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        if receiver.is_some() {
            return self
                .refuse(GeneralMemberOperation::ExplicitReceiver.name())
                .await;
        }
        match branch {
            GeneralMemberBranch::Bound(ty) => GeneralMemberEffects::bound(self, ty).await,
            GeneralMemberBranch::Undefined => {
                self.local(|| {
                    self.admit_local_work(1)?;
                    self.endpoint.check_completion()?;
                    Ok(Place::Undefined.into())
                })
                .await
            }
            GeneralMemberBranch::VersionInfo => {
                let ty = self
                    .local(|| {
                        self.admit_local_work(3)?;
                        self.endpoint.check_completion()?;
                        let version = self.env.python_version(self.db());
                        let segment = if key.name(self.db()) == "major" {
                            version.major
                        } else {
                            version.minor
                        };
                        Ok(Type::int_literal(segment.into()))
                    })
                    .await?;
                GeneralMemberEffects::bound(self, ty).await
            }
            GeneralMemberBranch::BoolReal(value) => {
                GeneralMemberEffects::bound(self, Type::int_literal(i64::from(value))).await
            }
            GeneralMemberBranch::Instance | GeneralMemberBranch::Restricted => {
                let (ty, _, _) = GeneralMemberEffects::key_parts(self, key).await?;
                if !matches!(ty, Type::NominalInstance(_)) {
                    return self.refuse(branch.operation().name()).await;
                }
                if matches!(branch, GeneralMemberBranch::Restricted) {
                    crate::types::restricted_member_entry_with(key, ty, LookupFacts, self).await
                } else {
                    crate::types::instance_member_entry_with(key, ty, LookupFacts, self).await
                }
            }
            _ => self.refuse(branch.operation().name()).await,
        }
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        if receiver.is_some() {
            return self
                .refuse(GeneralMemberOperation::ExplicitReceiver.name())
                .await;
        }
        self.queries
            .member_name(self.endpoint, self.program(), ty, name, policy)
            .await
    }

    async fn fallback(
        &self,
        _ty: Type<'db>,
        _name: GeneralMemberName<'_>,
        _policy: MemberLookupPolicy,
        _receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.refuse(GeneralMemberOperation::Lookup.name()).await
    }

    async fn dunder_class(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        let meta = self.meta(ty).await?;
        self.local(|| {
            self.admit_local_work(1)?;
            self.endpoint.check_completion()?;
            Ok(Place::bound(meta).into())
        })
        .await
    }

    async fn bound(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.local(|| {
            self.admit_local_work(1)?;
            self.endpoint.check_completion()?;
            Ok(Place::bound(ty).into())
        })
        .await
    }
}

pub(in crate::types) struct InstanceMemberProvider<'run, Q, S> {
    pub(in crate::types) queries: Q,
    pub(in crate::types) sources: &'run S,
}
impl<'run, 'db: 'run, C, Q: MemberDependencies<'run, 'db>, S: MemberSourcesAccess<'run, 'db>>
    salsa::execution_probe::CallableRouteProvider<'run, 'db, C>
    for InstanceMemberProvider<'run, Q, S>
where
    C: for<'a> Configuration<
            DbView = dyn Db,
            Input<'a> = MemberLookupKey<'a>,
            Output<'a> = MemberLookupResult<'a>,
        >,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        quote_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        key: C::Input<'db>,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        let program = endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(key.program(db))
            })
            .await;
        let effects = MemberEffects {
            endpoint: &endpoint,
            queries: &self.queries,
            sources: self.sources,
            env: ProgramEnvironment::from_program(program),
            fields: crate::types::mro::field_reads::MroFieldReads::new(db),
        };
        crate::types::member_lookup::general::member_lookup_dispatch_with(
            key,
            None,
            GeneralMemberFacts,
            &effects,
        )
        .await
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        id: salsa::Id,
        _key: C::Input<'db>,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(Place::bound(Type::divergent(id)).into())
            })
            .await)
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call MemberLookupResult<'db>,
        _value: MemberLookupResult<'db>,
        _key: C::Input<'db>,
    ) -> RunResult<MemberLookupResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted,
                ))
            })
            .await)
    }
}

pub(in crate::types) struct DescriptorGetProvider<'run, Q, S> {
    pub(in crate::types) queries: Q,
    pub(in crate::types) sources: &'run S,
}
impl<'run, 'db: 'run, C, Q: MemberDependencies<'run, 'db>, S: MemberSourcesAccess<'run, 'db>>
    salsa::execution_probe::CallableRouteProvider<'run, 'db, C>
    for DescriptorGetProvider<'run, Q, S>
where
    C: DescriptorConfiguration,
{
    async fn native_value<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        operation: NativeValueOperation<'call, 'db, C>,
    ) -> RunResult<NativeValueQuote>
    where
        'run: 'call,
    {
        quote_native_value(endpoint, operation).await
    }

    async fn body<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        db: &'db dyn Db,
        (program, ty, instance, owner): C::Input<'db>,
    ) -> RunResult<DescriptorResult<'db>>
    where
        'run: 'call,
    {
        let effects = MemberEffects {
            endpoint: &endpoint,
            queries: &self.queries,
            sources: self.sources,
            env: ProgramEnvironment::from_program(program),
            fields: crate::types::mro::field_reads::MroFieldReads::new(db),
        };
        effects.work(0).await?;
        crate::types::descriptor::evaluate_with_effects(
            db,
            &effects.env,
            DescriptorRequest {
                ty,
                instance,
                owner,
            },
            &effects,
        )
        .await
    }
    async fn initial<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _id: salsa::Id,
        _key: C::Input<'db>,
    ) -> RunResult<DescriptorResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Ok(Ok(None))
            })
            .await)
    }
    async fn recover<'call>(
        &'call self,
        endpoint: TaskEndpoint<'run, 'db>,
        _db: &'db dyn Db,
        _cycle: &'call salsa::Cycle<'call>,
        _last: &'call DescriptorResult<'db>,
        _value: DescriptorResult<'db>,
        _key: C::Input<'db>,
    ) -> RunResult<DescriptorResult<'db>>
    where
        'run: 'call,
    {
        Ok(endpoint
            .local_call(|| {
                endpoint.admit_work(1)?;
                Err(RunError::Refused(
                    salsa::attempt_probe::Incomplete::Interrupted,
                ))
            })
            .await)
    }
}
