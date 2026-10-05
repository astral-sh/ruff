//! Constructor members resolved through admitted children and the invocation's callable guard.
//!
//! Initializer resolution delegates eager `Self` binding to the member provider, which can
//! refuse `MemberLookup(MemberSelfBinding)` when mapping is required. Callable expansion,
//! constructor-stage assembly and argument checking belong to their respective consumers.

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::place::{Place, PlaceAndQualifiers};
use crate::types::constructor::member_resolution::{
    ConstructorDescriptorEffects, ConstructorMemberEffects, ObjectInitializer, initializer_with,
    metaclass_call_with, new_method_with, resolve_initializer_descriptor_with,
};
use crate::types::constructor::{ConstructorMember, ConstructorMembers, InitializerBinding};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::descriptor::DescriptorRequest;
use crate::types::{
    BoundMethodType, ClassType, DescriptorGetResult, MemberEntryEffects, MemberLookupPolicy, Type,
};

impl<'access, 'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'access, 'run, 'db, A> {
    /// Resolves a custom metaclass `__call__` with this constructor invocation's guard.
    pub(in crate::types::infer) async fn constructor_metaclass_call(
        &self,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<ConstructorMember<'db>> {
        let effects = self.constructor_member_effects(guard).await?;
        let member = self
            .allocate_future(|| metaclass_call_with(members, &effects))
            .await?
            .await?;
        ConstructorDescriptorEffects::local(&effects, || member).await
    }

    /// Resolves `__new__` using its canonical lookup and the supplied descriptor guard.
    pub(in crate::types::infer) async fn constructor_new_method(
        &self,
        members: ConstructorMembers<'db>,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<ConstructorMember<'db>> {
        let effects = self.constructor_member_effects(guard).await?;
        let member = self
            .allocate_future(|| new_method_with(members, &effects))
            .await?
            .await?;
        ConstructorDescriptorEffects::local(&effects, || member).await
    }

    /// Resolves `__init__`, applying the requested `object` fallback and native receiver binding.
    pub(in crate::types::infer) async fn constructor_initializer(
        &self,
        members: ConstructorMembers<'db>,
        object: ObjectInitializer,
        guard: &CallableRecursionGuard<'db>,
    ) -> RunResult<ConstructorMember<'db>> {
        let effects = self.constructor_member_effects(guard).await?;
        let member = self
            .allocate_future(|| initializer_with(members, object, &effects))
            .await?
            .await?;
        ConstructorDescriptorEffects::local(&effects, || member).await
    }

    /// Admits the fixed adapter and environment while borrowing the original invocation guard.
    async fn constructor_member_effects<'effects, 'guard>(
        &'effects self,
        guard: &'guard CallableRecursionGuard<'db>,
    ) -> RunResult<ControlledConstructorMembers<'effects, 'access, 'run, 'db, 'guard, A>> {
        let size = size_of::<ControlledConstructorMembers<'_, '_, '_, '_, '_, A>>();
        self.local(4, size, || ControlledConstructorMembers {
            source: self,
            guard,
            env: ProgramEnvironment::from_program(self.program),
        })
        .await
    }
}

/// Borrows source access and the caller's guard; members and results contain only copyable handles.
struct ControlledConstructorMembers<'effects, 'access, 'run, 'db: 'run, 'guard, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    guard: &'guard CallableRecursionGuard<'db>,
    env: ProgramEnvironment<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorDescriptorEffects<'db>
    for ControlledConstructorMembers<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        // Fund the fixed inputs, branch comparisons and copy-only retirement before each shared
        // member decision. Namespace scans, queries and descriptor binding charge their own work.
        let size = size_of::<(
            ConstructorMembers<'db>,
            Place<'db>,
            DescriptorRequest<'db>,
            ObjectInitializer,
            &Self,
        )>();
        self.source.local(8, size, || ()).await
    }

    async fn local<T: Copy>(&self, operation: impl FnOnce() -> T) -> RunResult<T> {
        // Sixteen scalar operations cover the largest fixed constructor record update, including
        // option selection and copies of its place metadata. No closure traverses a collection.
        self.source.local(16, size_of::<T>(), operation).await
    }

    async fn descriptor(
        &self,
        request: DescriptorRequest<'db>,
    ) -> RunResult<Option<DescriptorGetResult<'db>>> {
        let descriptor = self
            .source
            .allocate_future(|| self.source.guarded_member_descriptor(request, self.guard))
            .await?
            .await?;
        self.local(|| descriptor.unwrap_or_else(|error| Some(error.fallback())))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorMemberEffects<'db>
    for ControlledConstructorMembers<'_, '_, 'run, 'db, '_, A>
{
    async fn member(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<ConstructorMember<'db>> {
        let member = self
            .source
            .allocate_future(|| {
                self.source
                    .guarded_member(ty, name, policy, receiver, self.guard)
            })
            .await?
            .await?;
        let parts = self.source.member_lookup_parts(member).await?;
        self.local(|| ConstructorMember {
            place: parts.member.place,
            origin: parts.descriptor,
        })
        .await
    }

    async fn new_member(&self, ty: Type<'db>) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        let member = self
            .source
            .allocate_future(|| {
                self.source
                    .access
                    .constructor_new_member(self.source.program, ty)
            })
            .await?
            .await?;
        self.local(|| member).await
    }

    async fn namespace(
        &self,
        ty: Type<'db>,
        class: ClassType<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Place<'db>> {
        let member = self
            .source
            .allocate_future(|| MemberEntryEffects::namespace(self.source, ty, class, name, policy))
            .await?
            .await?;
        self.local(|| member.place).await
    }

    async fn function_like(&self, request: DescriptorRequest<'db>) -> RunResult<Option<Type<'db>>> {
        let callable = self
            .source
            .allocate_future(|| {
                crate::types::callable::function_descriptor_with(
                    request.ty,
                    &self.env,
                    request.instance,
                    Some(request.owner),
                    self.source,
                )
            })
            .await?
            .await?;
        self.local(|| callable).await
    }

    async fn bound_function(&self, method: BoundMethodType<'db>) -> RunResult<Type<'db>> {
        self.source
            .field(
                method
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .func(),
            )
            .await
    }

    async fn bind_initializer(
        &self,
        members: ConstructorMembers<'db>,
        initializer: Type<'db>,
    ) -> RunResult<InitializerBinding<'db>> {
        let binding = self
            .source
            .allocate_future(|| resolve_initializer_descriptor_with(members, initializer, self))
            .await?
            .await?;
        let callable = self
            .source
            .allocate_future(|| {
                self.source
                    .bind_member_self_type(binding.callable, members.instance)
            })
            .await?
            .await?;
        self.local(|| InitializerBinding {
            callable,
            ..binding
        })
        .await
    }
}
