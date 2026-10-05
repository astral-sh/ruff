//! Class-object namespace children run under the caller's existing source endpoint.

use std::future::Future;
use std::pin::Pin;

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::{Definedness, PlaceAndQualifiers};
use crate::types::class::KnownClassInstanceEffects;
use crate::types::class::namespace::NamespaceLookupEffects;
use crate::types::instance::{NominalClassFacts, nominal_class_with};
use crate::types::member_lookup::class_object::{
    ClassObjectEffects, ClassObjectFacts, ClassObjectWork,
    class_object_instance_approximation_with, class_object_instance_member_with,
    class_object_member_with,
};
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::member_lookup::mro_dispatch::SubclassMroEffects;
use crate::types::relation::source::RelationSourceEffects;
use crate::types::{
    ClassType, MemberEntryEffects, MemberLookupPolicy, NominalInstanceType, ProtocolInstanceType,
    SubclassOfInner, SubclassOfType, Type,
};

#[cfg(test)]
use crate::types::infer::source_runtime::tests::class_object_member::{
    Stage, observe_after, observe_before,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Resolves one named class-object member, including metaclass instance storage.
    pub(in crate::types::infer) async fn class_object_member_value(
        &self,
        ty: Type<'db>,
        name: &Name,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let member = self
            .class_object_child(|| {
                class_object_member_with(ty, name.as_str(), policy, ClassObjectFacts, self)
            })
            .await?;
        #[cfg(test)]
        observe_before(Stage::Transfer);
        self.class_object_local(3, 0, || {
            #[cfg(test)]
            observe_after(Stage::Transfer);
            member
        })
        .await
    }

    /// Admits fixed callback, result and quotation representations before executing a local step.
    ///
    /// `work` counts scalar operations; `extra_bytes` covers caller-specific slots. Class-object
    /// adapters borrow names and source access or copy handles, so retirement has no payload
    /// traversal. The sixteen additional units cover six checked additions, two contract checks
    /// and eight fixed callback/result transfers and retirement steps.
    pub(super) async fn class_object_local<T, M: FnOnce() -> T>(
        &self,
        work: usize,
        extra_bytes: usize,
        make: M,
    ) -> RunResult<T> {
        let bytes = Self::checked(
            extra_bytes
                .checked_add(size_of::<M>())
                .and_then(|bytes| bytes.checked_add(size_of::<Option<M>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<T>()))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<(usize, usize)>>())),
        )?;
        self.local(Self::checked(work.checked_add(16))?, bytes, make)
            .await
    }

    /// Admits a child factory, boxes its future through the existing allocator, and awaits its result.
    ///
    /// The allocator charges the future itself. Its factory remains outside refusal callbacks;
    /// the caller retains all borrowed inputs while the source endpoint drains queued children.
    pub(super) async fn class_object_child<T, F, M>(&self, make: M) -> RunResult<T>
    where
        F: Future<Output = RunResult<T>>,
        M: FnOnce() -> F,
    {
        let bytes = Self::checked(
            size_of::<M>()
                .checked_add(size_of::<Option<M>>())
                .and_then(|bytes| bytes.checked_add(size_of::<Pin<Box<F>>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<Pin<Box<F>>>>()))
                .and_then(|bytes| bytes.checked_add(size_of::<T>()))
                .and_then(|bytes| bytes.checked_add(size_of::<RunResult<T>>())),
        )?;
        self.class_object_local(8, bytes, || ()).await?;
        self.allocate_future(make).await?.await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassObjectEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, _work: ClassObjectWork) -> RunResult<()> {
        // Each decision uses a subset of these copyable slots. Charging the full set at every
        // checkpoint covers intermediate Options, dispatch arguments and publication without
        // charging the database objects reached through their handles. The sixteen work units
        // allow four variant tests, two field extractions, six handle/Option transfers and four
        // fixed dispatch/retirement steps; own-declaration inspection uses the most variant tests.
        let bytes = size_of::<(
            Type<'db>,
            Option<Type<'db>>,
            Option<ClassType<'db>>,
            Option<PlaceAndQualifiers<'db>>,
            Option<Definedness>,
            SubclassOfInner<'db>,
            MemberLookupPolicy,
            &str,
            ClassObjectWork,
        )>();
        self.class_object_local(16, bytes, || ()).await
    }

    async fn find_in_mro(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.class_object_child(|| self.source_find_name_in_mro(ty, name, policy))
            .await
    }

    async fn to_class_type(&self, ty: Type<'db>) -> RunResult<Option<ClassType<'db>>> {
        self.class_object_child(|| KnownClassInstanceEffects::to_class_type(self, ty))
            .await
    }

    async fn subclass_inner_class(
        &self,
        inner: SubclassOfInner<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.class_object_child(|| RelationSourceEffects::subclass_inner_class(self, inner))
            .await
    }

    async fn protocol_origin(
        &self,
        protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.class_object_child(|| SubclassMroEffects::protocol_origin(self, protocol))
            .await
    }

    async fn own_member(
        &self,
        class: ClassType<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let member = self
            .class_object_child(|| self.source_own_class_member(class, name, None))
            .await?;
        self.class_object_local(2, 0, || member.inner).await
    }

    async fn meta_type(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        self.class_object_child(|| MemberEntryEffects::meta_type(self, ty))
            .await
    }

    async fn instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.class_object_child(|| class_object_instance_approximation_with(ty, self))
            .await
    }

    async fn instance_member(
        &self,
        ty: Type<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        #[cfg(test)]
        observe_before(Stage::InstanceStorage);
        let member = self
            .class_object_child(|| {
                class_object_instance_member_with(ty, name, ClassObjectFacts, self)
            })
            .await?;
        #[cfg(test)]
        observe_after(Stage::InstanceStorage);
        self.class_object_local(2, 0, || member).await
    }

    async fn fall_back_to(
        &self,
        member: PlaceAndQualifiers<'db>,
        fallback: PlaceAndQualifiers<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.class_object_child(|| NamespaceLookupEffects::fall_back_to(self, member, fallback))
            .await
    }

    async fn class_instance_approximation(&self, ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.class_object_child(|| NamespaceLookupEffects::instance_approximation(self, ty))
            .await
    }

    async fn subclass_instance(&self, subclass: SubclassOfType<'db>) -> RunResult<Type<'db>> {
        self.class_object_child(|| self.subclass_instance_value(subclass))
            .await
    }

    async fn other_instance_approximation(&self, _ty: Type<'db>) -> RunResult<Option<Type<'db>>> {
        self.class_object_child(|| {
            self.unavailable(SourceOperation::MemberLookup(
                GeneralMemberOperation::InstanceApproximation,
            ))
        })
        .await
    }

    async fn nominal_instance_member(
        &self,
        instance: NominalInstanceType<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let class = self
            .class_object_child(|| nominal_class_with(instance, NominalClassFacts, self))
            .await?;
        self.class_object_child(|| self.class_instance_storage(class, name))
            .await
    }

    async fn other_instance_member(
        &self,
        _ty: Type<'db>,
        _name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.class_object_child(|| {
            self.unavailable(SourceOperation::MemberLookup(
                GeneralMemberOperation::InstanceStorage,
            ))
        })
        .await
    }
}
