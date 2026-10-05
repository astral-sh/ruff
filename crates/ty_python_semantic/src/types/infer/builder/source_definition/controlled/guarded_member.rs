//! Member lookup retains the caller's callable guard through descriptor selection.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::PlaceAndQualifiers;
use crate::types::descriptor::effects::{DescriptorEffects, DescriptorOperation};
use crate::types::descriptor::{
    DescriptorEntryEffects, DescriptorRequest, DescriptorResult, evaluate_entry_with_effects,
};
use crate::types::enums::EnumMetadata;
use crate::types::member_lookup::general::{
    GeneralMemberBranch, GeneralMemberEffects, GeneralMemberFacts, GeneralMemberName,
    GeneralMemberPredicate, member_lookup_dispatch_with, member_lookup_entry_with,
};
use crate::types::{
    CallableRecursionGuard, CallableType, ClassLiteral, ClassType, DescriptorOrigin,
    InstanceFallbackShadowsNonDataDescriptor, KnownClass, LookupDescriptorEffects, LookupFacts,
    LookupParts, MemberEntryEffects, MemberLookupKey, MemberLookupPolicy, MemberLookupResult,
    NominalInstanceType, PropertyDeprecations, SlotDescriptorType, Type,
    instance_member_entry_with, invoke_lookup_descriptor_with, restricted_member_entry_with,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Looks up `__call__` with the caller's recursion guard and without instance storage.
    pub(in crate::types::infer) async fn guarded_callable_member(
        &self,
        ty: Type<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.allocate_future(|| {
            self.guarded_member(
                ty,
                "__call__",
                MemberLookupPolicy::NO_INSTANCE_FALLBACK,
                None,
                guard,
            )
        })
        .await?
        .await
    }

    /// Looks up a member with the supplied receiver, policy and callable recursion guard.
    pub(super) async fn guarded_member(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        let effects = self
            .local(
                3,
                size_of::<GuardedMemberEffects<'_, '_, '_, '_, '_, A>>(),
                || GuardedMemberEffects {
                    source: self,
                    guard,
                },
            )
            .await?;
        let result = self
            .allocate_future(|| {
                member_lookup_entry_with(
                    ty,
                    GeneralMemberName::Text(name),
                    policy,
                    receiver,
                    GeneralMemberFacts,
                    &effects,
                )
            })
            .await?
            .await?;
        self.local(1, size_of::<MemberLookupResult<'db>>(), || result)
            .await
    }

    /// Selects metaclass descriptors with the class namespace fallback and the supplied guard.
    /// Class attributes shadow non-data metaclass descriptors; data descriptors retain precedence.
    pub(super) async fn guarded_class_object_descriptor(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        let effects = self.local_with_fixed_transfers(3, 0, || GuardedMemberEffects {
            source: self,
            guard,
        }).await?;
        self.type_parameter_future(|| invoke_lookup_descriptor_with(
            key, receiver, fallback, InstanceFallbackShadowsNonDataDescriptor::Yes,
            LookupFacts, &effects,
        )).await?.await
    }

    /// Binds native descriptors while retaining the caller's guard for the protocol boundary.
    pub(super) async fn guarded_member_descriptor(
        &self,
        request: DescriptorRequest<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        let size = size_of::<(
            ProgramEnvironment<'db>,
            GuardedDescriptorEffects<'_, '_, '_, '_, '_, A>,
        )>();
        let (env, effects) = self
            .local(5, size, || {
                (
                    ProgramEnvironment::from_program(self.program),
                    GuardedDescriptorEffects {
                        source: self,
                        guard,
                    },
                )
            })
            .await?;
        let result = self
            .allocate_future(|| evaluate_entry_with_effects(self.db(), &env, request, &effects))
            .await?
            .await?;
        self.local(1, size_of::<DescriptorResult<'db>>(), || result)
            .await
    }
}

/// Borrows the invocation guard without providing an unguarded protocol fallback.
struct GuardedDescriptorEffects<'effects, 'access, 'run, 'db: 'run, 'guard, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    guard: &'guard CallableRecursionGuard<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DescriptorEntryEffects<'db>
    for GuardedDescriptorEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint_entry(&self) -> RunResult<()> {
        let size = size_of::<(DescriptorRequest<'db>, DescriptorResult<'db>)>();
        self.source.local(6, size, || ()).await
    }

    async fn slot_value_entry(
        &self,
        db: &'db dyn Db,
        descriptor: SlotDescriptorType<'db>,
    ) -> RunResult<Type<'db>> {
        DescriptorEffects::slot_value(self.source, db, descriptor).await
    }

    async fn function_like_entry(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        DescriptorEffects::function_like(self.source, db, env, request).await
    }

    async fn protocol_entry(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        self.source
            .local(1, size_of::<&CallableRecursionGuard<'db>>(), || self.guard)
            .await?;
        self.source
            .unavailable(SourceOperation::Descriptor(
                DescriptorOperation::GuardedEvaluation,
            ))
            .await
    }
}

struct GuardedMemberEffects<'effects, 'access, 'run, 'db: 'run, 'guard, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    guard: &'guard CallableRecursionGuard<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GeneralMemberEffects<'db>
    for GuardedMemberEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        GeneralMemberEffects::checkpoint(self.source, name).await
    }

    async fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<(Type<'db>, &'db Name, MemberLookupPolicy)> {
        GeneralMemberEffects::key_parts(self.source, key).await
    }

    async fn predicate(
        &self,
        predicate: GeneralMemberPredicate<'db>,
        name: &str,
    ) -> RunResult<bool> {
        GeneralMemberEffects::predicate(self.source, predicate, name).await
    }

    async fn wrapper_descriptor(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<Type<'db>>> {
        GeneralMemberEffects::wrapper_descriptor(self.source, ty, name, policy).await
    }

    async fn callable_runtime_class(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        GeneralMemberEffects::callable_runtime_class(self.source, callable).await
    }

    async fn nominal_enum_member(
        &self,
        instance: NominalInstanceType<'db>,
        name: &str,
    ) -> RunResult<Option<(ClassLiteral<'db>, &'db EnumMetadata<'db>)>> {
        GeneralMemberEffects::nominal_enum_member(self.source, instance, name).await
    }

    async fn execute(
        &self,
        branch: GeneralMemberBranch<'db>,
        key: MemberLookupKey<'db>,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        match branch {
            GeneralMemberBranch::Bound(_)
            | GeneralMemberBranch::Undefined
            | GeneralMemberBranch::BoolReal(_)
            | GeneralMemberBranch::VersionInfo => {
                GeneralMemberEffects::execute(self.source, branch, key, receiver).await
            }
            GeneralMemberBranch::ClassObject => {
                self.source
                    .type_parameter_future(|| self.source.class_object_entry(key, receiver, Some(self.guard)))
                    .await?
                    .await
            }
            GeneralMemberBranch::Instance | GeneralMemberBranch::Restricted => {

                let (ty, _, _) = GeneralMemberEffects::key_parts(self, key).await?;
                let receiver = receiver.unwrap_or(ty);
                if matches!(branch, GeneralMemberBranch::Instance) {
                    self.source
                        .allocate_future(|| {
                            instance_member_entry_with(key, receiver, LookupFacts, self)
                        })
                        .await?
                        .await
                } else {
                    self.source
                        .allocate_future(|| {
                            restricted_member_entry_with(key, receiver, LookupFacts, self)
                        })
                        .await?
                        .await
                }
            }
            _ => {
                self.source
                    .unavailable(SourceOperation::MemberLookup(branch.operation()))
                    .await
            }
        }
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        let mut name_owner = None;
        match name {
            GeneralMemberName::Shared(name) => {
                self.source
                    .local(size_of::<Name>() * 2 + 1, 0, || {
                        name_owner = Some(name.clone());
                    })
                    .await?;
            }
            GeneralMemberName::Text(text) => {
                let work = SourceEffects::<A>::checked(
                    text.len()
                        .checked_mul(2)
                        .and_then(|len| len.checked_add(size_of::<Name>() * 2 + 1)),
                )?;
                let bytes =
                    SourceEffects::<A>::checked(text.len().checked_add(3 * size_of::<usize>()))?;
                self.source
                    .local(work, bytes, || {
                        name_owner = Some(Name::new(text));
                    })
                    .await?;
            }
        }
        let name = name_owner.ok_or(RunError::Contract(
            "guarded member name was not constructed",
        ))?;
        let key = self
            .source
            .access
            .member_lookup_key(ty, name, policy)
            .await?;
        // The guard is not part of the canonical member-query key, so execute the shared body
        // with this adapter instead of fetching that query's unguarded result.
        self.source
            .allocate_future(|| {
                member_lookup_dispatch_with(key, receiver, GeneralMemberFacts, self)
            })
            .await?
            .await
    }

    async fn fallback(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .allocate_future(|| {
                member_lookup_entry_with(ty, name, policy, receiver, GeneralMemberFacts, self)
            })
            .await?
            .await
    }

    async fn dunder_class(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        GeneralMemberEffects::dunder_class(self.source, ty).await
    }

    async fn bound(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        GeneralMemberEffects::bound(self.source, ty).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberEntryEffects<'db>
    for GuardedMemberEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        MemberEntryEffects::checkpoint(self.source).await
    }

    async fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<(Type<'db>, &'db Name, MemberLookupPolicy)> {
        GeneralMemberEffects::key_parts(self.source, key).await
    }

    async fn suppress_typed_dict_classvar(
        &self,
        ty: Type<'db>,
        result: MemberLookupResult<'db>,
    ) -> RunResult<bool> {
        MemberEntryEffects::suppress_typed_dict_classvar(self.source, ty, result).await
    }

    async fn meta_type(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        MemberEntryEffects::meta_type(self.source, ty).await
    }

    async fn nominal_class(&self, instance: NominalInstanceType<'db>) -> RunResult<ClassType<'db>> {
        MemberEntryEffects::nominal_class(self.source, instance).await
    }

    async fn namespace(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        MemberEntryEffects::namespace(self.source, ty, class, name, policy).await
    }

    async fn enum_member(
        &self,
        ty: Type<'db>,
        name: &Name,
    ) -> RunResult<Option<MemberLookupResult<'db>>> {
        MemberEntryEffects::enum_member(self.source, ty, name).await
    }

    async fn instance_storage(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        MemberEntryEffects::instance_storage(self.source, ty, name).await
    }

    async fn invoke_descriptor(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
        fallback: MemberLookupResult<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        self.source
            .allocate_future(|| {
                invoke_lookup_descriptor_with(
                    key,
                    receiver,
                    fallback,
                    InstanceFallbackShadowsNonDataDescriptor::No,
                    LookupFacts,
                    self,
                )
            })
            .await?
            .await
    }

    async fn fallback(
        &self,
        ty: Type<'db>,
        name: &Name,
        result: MemberLookupResult<'db>,
        policy: MemberLookupPolicy,
    ) -> RunResult<MemberLookupResult<'db>> {
        MemberEntryEffects::fallback(self.source, ty, name, result, policy).await
    }

    async fn bind_self(
        &self,
        result: MemberLookupResult<'db>,
        receiver: Type<'db>,
    ) -> RunResult<MemberLookupResult<'db>> {
        MemberEntryEffects::bind_self(self.source, result, receiver).await
    }

    async fn promote(&self, result: MemberLookupResult<'db>) -> RunResult<MemberLookupResult<'db>> {
        MemberEntryEffects::promote(self.source, result).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LookupDescriptorEffects<'db>
    for GuardedMemberEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        LookupDescriptorEffects::checkpoint(self.source).await
    }

    async fn fallback_parts(&self, result: MemberLookupResult<'db>) -> RunResult<LookupParts<'db>> {
        LookupDescriptorEffects::fallback_parts(self.source, result).await
    }

    async fn class_attribute(
        &self,
        key: MemberLookupKey<'db>,
        receiver: Type<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        // `InlineLookupDescriptor::class_attribute` looks up the namespace without a guard.
        // `InlineLookupDescriptor::descriptor` supplies its guard to descriptor evaluation.
        LookupDescriptorEffects::class_attribute(self.source, key, receiver).await
    }

    async fn owner(&self, receiver: Type<'db>) -> RunResult<Type<'db>> {
        LookupDescriptorEffects::owner(self.source, receiver).await
    }

    async fn descriptor(
        &self,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        self.source
            .guarded_member_descriptor(request, self.guard)
            .await
    }

    async fn properties(&self, ty: Type<'db>) -> RunResult<Option<PropertyDeprecations<'db>>> {
        LookupDescriptorEffects::properties(self.source, ty).await
    }

    async fn union(&self, first: Type<'db>, second: Type<'db>) -> RunResult<Type<'db>> {
        LookupDescriptorEffects::union(self.source, first, second).await
    }

    async fn merge_properties(
        &self,
        first: Option<PropertyDeprecations<'db>>,
        second: Option<PropertyDeprecations<'db>>,
    ) -> RunResult<Option<PropertyDeprecations<'db>>> {
        LookupDescriptorEffects::merge_properties(self.source, first, second).await
    }

    async fn merge_origins(
        &self,
        first: DescriptorOrigin<'db>,
        second: DescriptorOrigin<'db>,
    ) -> RunResult<DescriptorOrigin<'db>> {
        LookupDescriptorEffects::merge_origins(self.source, first, second).await
    }

    async fn result(&self, parts: LookupParts<'db>) -> RunResult<MemberLookupResult<'db>> {
        LookupDescriptorEffects::result(self.source, parts).await
    }
}
