use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::Place;
use crate::types::call::{Bindings, CallArguments, CallError};
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::descriptor::effects::{DescriptorEffects, DescriptorOperation, sealed};
use crate::types::descriptor::{
    DescriptorInvocationRequest, DescriptorMemberRequest, DescriptorRequest, DescriptorResult,
    evaluate_entry_with_effects, evaluate_with_effects,
};
use crate::types::set_theoretic::builder::controlled_union::UnionEffects;
use crate::types::set_theoretic::pair_intersection::PairIntersectionEffects;
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::{
    DescriptorGetCallContext, DescriptorOrigin, IntersectionBuilder, IntersectionType, KnownClass,
    NewType, PropertyDeprecations, SlotDescriptorType, Type, TypeDispatchEffects, UnionBuilder,
    UnionType, union_like_with,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn descriptor_protocol(
        &self,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        let env = self
            .local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
                ProgramEnvironment::from_program(self.program)
            })
            .await?;
        self.allocate_future(|| evaluate_with_effects(self.db(), &env, request, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DescriptorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
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
        self.field(
            descriptor
                .field_requests(self.access.endpoint().field_request_context())
                .value_type(),
        )
        .await
    }

    async fn union_parts(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        union: UnionType<'db>,
    ) -> RunResult<(UnionBuilder<'db>, &'db [Type<'db>])> {
        self.environment_program(env).await?;
        let mut builder = PairUnionEffects::new_union(self, env).await?;
        let recursion = self.union_recursion_source(union).await?;
        UnionEffects::merge_recursion(self, &mut builder, recursion).await?;
        Ok((builder, self.union_elements_source(union).await?))
    }

    async fn intersection_parts(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> RunResult<(IntersectionBuilder<'db>, &'db FxOrderSet<Type<'db>>)> {
        let builder = self.new_intersection(env).await?;
        let elements = self
            .field(
                intersection
                    .field_requests(self.access.endpoint().field_request_context())
                    .positive(),
            )
            .await?;
        Ok((builder, elements))
    }

    async fn next_descriptor(
        &self,
        requests: &mut impl Iterator<Item = DescriptorRequest<'db>>,
    ) -> RunResult<Option<DescriptorRequest<'db>>> {
        self.local(size_of::<DescriptorRequest<'db>>() * 2 + 1, 0, || {
            requests.next()
        })
        .await
    }

    async fn call_context(
        &self,
        _db: &'db dyn Db,
        request: DescriptorRequest<'db>,
        callable: Type<'db>,
    ) -> RunResult<DescriptorGetCallContext<'db>> {
        self.allocate_future(|| {
            self.access.intern_descriptor_get_call_context(
                request.ty,
                callable,
                request.instance,
                request.owner,
            )
        })
        .await?
        .await
    }

    async fn function_like(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| {
            crate::types::callable::function_descriptor_with(
                request.ty,
                env,
                request.instance,
                Some(request.owner),
                self,
            )
        })
        .await?
        .await
    }

    async fn protocol(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        self.environment_program(env).await?;
        self.access.descriptor_get(request).await
    }

    async fn declare_descriptors(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _requests: impl Iterator<Item = DescriptorRequest<'db>> + Clone,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::AlternativeRegistration,
        ))
        .await
    }

    async fn descriptor(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> RunResult<DescriptorResult<'db>> {
        self.environment_program(env).await?;
        self.allocate_future(|| evaluate_entry_with_effects(self.db(), env, request, self))
            .await?
            .await
    }

    async fn union_like(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<UnionType<'db>>> {
        union_like_with(ty, self).await
    }

    async fn class_member(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorMemberRequest<'db>,
    ) -> RunResult<Place<'db>> {
        self.environment_program(env).await?;
        let name = Name::new_static("__get__");
        let member = self
            .access
            .class_member_lookup(request.ty, &name, request.policy)
            .await?;
        self.local(1, 0, || member.place).await
    }

    async fn data_descriptor(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let program = self.environment_program(env).await?;
        self.access.data_descriptor(program, ty, false).await
    }

    async fn none_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Type<'db>> {
        self.access
            .known_class_instance(self.program, KnownClass::NoneType)
            .await
    }

    async fn invoke(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorInvocationRequest<'db>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>> {
        let count = request.arguments.len();
        let work = Self::checked(count.checked_mul(4).and_then(|work| work.checked_add(4)))?;
        let bytes = Self::checked(
            CallArguments::capacity_bytes(count)
                .and_then(|payload| payload.checked_add(size_of::<CallArguments<'_, 'db>>())),
        )?;
        // Keep the arguments in this continuation so they outlive child drainage when
        // `synthetic_call` is interrupted. Each positional entry has an empty type map.
        let mut argument_owner: Option<CallArguments<'_, 'db>> =
            self.initialize_value(|| None).await?;
        self.local(work, bytes, || {
            argument_owner = Some(CallArguments::positional(request.arguments));
        })
        .await?;
        let arguments = argument_owner.as_ref().ok_or(RunError::Contract(
            "descriptor arguments were not constructed",
        ))?;
        self.allocate_future(|| self.synthetic_call(env, request.callable, arguments, None))
            .await?
            .await
    }

    async fn bindings_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>; 3],
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.allocate_future(|| bindings.descriptor_origin_with(db, env, arguments, self))
            .await?
            .await
    }

    async fn bindings_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> RunResult<Type<'db>> {
        self.allocate_future(|| bindings.return_type_with(db, env, self)).await?.await
    }

    async fn merge_origins(
        &self,
        db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.allocate_future(|| left.merge_with(db, right, self)).await?.await
    }

    async fn union_add(
        &self,
        mut builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        PairUnionEffects::union_add(self, &mut builder, ty).await?;
        Ok(builder)
    }

    async fn union_build(&self, builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        PairUnionEffects::union_build(self, builder).await
    }

    async fn union_pair(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.access.union_from_two_elements(left, right).await
    }

    async fn intersection_add(
        &self,
        mut builder: IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> RunResult<IntersectionBuilder<'db>> {
        self.intersection_add_positive(&mut builder, ty).await?;
        Ok(builder)
    }

    async fn intersection_build(&self, builder: IntersectionBuilder<'db>) -> RunResult<Type<'db>> {
        PairIntersectionEffects::intersection_build(self, builder).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    crate::types::callable::FunctionDescriptorEffects<'db> for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn union(
        &self,
        _union: UnionType<'db>,
        _instance: Option<Type<'db>>,
        _owner: Option<Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::FunctionBinding,
        ))
        .await
    }

    async fn alias(
        &self,
        _alias: crate::types::TypeAliasType<'db>,
        _instance: Option<Type<'db>>,
        _owner: Option<Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::FunctionBinding,
        ))
        .await
    }

    async fn bind(
        &self,
        ty: Type<'db>,
        env: &ProgramEnvironment<'db>,
        instance: Option<Type<'db>>,
        owner: Option<Type<'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        self.allocate_future(|| {
            crate::types::callable::function_descriptor::bind_function_descriptor_with(
                ty, env, instance, owner, self,
            )
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeDispatchEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(1).await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        NominalSelectionEffects::resolve_alias(self, ty).await
    }

    async fn newtype_union(&self, _newtype: NewType<'db>) -> RunResult<Option<UnionType<'db>>> {
        self.unavailable(SourceOperation::Descriptor(
            DescriptorOperation::NewTypeUnion,
        ))
        .await
    }

    async fn collect_properties(
        &self,
        ty: Type<'db>,
    ) -> RunResult<Option<PropertyDeprecations<'db>>> {
        self.allocate_future(|| {
            crate::types::property_deprecations::collect_with(self.db(), ty, self)
        })
        .await?
        .await
    }
}
