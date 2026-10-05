//! Semantic dependencies of descriptor evaluation.
//!
//! A queued provider must implement each operation or return its own explicit incomplete outcome.
//! Only the sealed legacy provider below may call the synchronous checker implementations.

use std::convert::Infallible;
use std::future::{Future, ready};

use super::{
    DescriptorInvocationRequest, DescriptorMemberRequest, DescriptorRequest, DescriptorResult,
};
use crate::place::Place;
use crate::types::call::{Bindings, CallArguments, CallError};
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::{
    DescriptorGetCallContext, DescriptorOrigin, IntersectionBuilder, IntersectionType,
    SlotDescriptorType, Type, UnionBuilder, UnionType,
};
use crate::{Db, FxOrderSet, Program, ProgramEnvironment};

#[cfg(any(test, feature = "experimental-analysis"))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DescriptorOperation {
    GuardedEvaluation,
    FunctionBinding,
    AlternativeRegistration,
    ClassMember,
    DataDescriptor,
    CallContext,
    Invocation,
    BindingsOrigin,
    BindingsReturnType,
    MergeOrigins,
    NewTypeUnion,
    PropertyMetadata,
}

pub(in crate::types) mod sealed {
    pub(in crate::types) trait Sealed {}
}

pub(in crate::types) trait DescriptorEffects<'db>: sealed::Sealed {
    type Error;

    async fn checkpoint(&self) -> Result<(), Self::Error>;

    async fn slot_value(
        &self,
        db: &'db dyn Db,
        descriptor: SlotDescriptorType<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn union_parts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        union: UnionType<'db>,
    ) -> Result<(UnionBuilder<'db>, &'db [Type<'db>]), Self::Error>;

    async fn intersection_parts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<(IntersectionBuilder<'db>, &'db FxOrderSet<Type<'db>>), Self::Error>;

    async fn next_descriptor(
        &self,
        requests: &mut impl Iterator<Item = DescriptorRequest<'db>>,
    ) -> Result<Option<DescriptorRequest<'db>>, Self::Error>;

    async fn call_context(
        &self,
        db: &'db dyn Db,
        request: DescriptorRequest<'db>,
        callable: Type<'db>,
    ) -> Result<DescriptorGetCallContext<'db>, Self::Error>;

    async fn function_like(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<Option<Type<'db>>, Self::Error>;

    /// Evaluates only the protocol body, after native descriptor access has been ruled out.
    /// The legacy provider applies its existing query cache at this boundary.
    async fn protocol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Self::Error>;

    /// Registers every independent alternative before any is awaited. A graph can retain their
    /// separate plans and grounded results even if another alternative remains unresolved.
    async fn declare_descriptors(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        requests: impl Iterator<Item = DescriptorRequest<'db>> + Clone,
    ) -> Result<(), Self::Error>;

    async fn descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Self::Error>;

    async fn union_like(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<Option<UnionType<'db>>, Self::Error>;

    async fn class_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorMemberRequest<'db>,
    ) -> Result<Place<'db>, Self::Error>;

    async fn data_descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error>;

    async fn none_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    /// Completes matching, inference, argument checking, overload selection and native evaluation.
    /// An unfinished stage must remain an effect failure, not a recovered `CallError` or bindings.
    async fn invoke(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorInvocationRequest<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Self::Error>;

    async fn bindings_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>; 3],
    ) -> Result<DescriptorOrigin<'db>, Self::Error>;

    async fn bindings_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn merge_origins(
        &self,
        db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> Result<DescriptorOrigin<'db>, Self::Error>;

    async fn union_add(
        &self,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<UnionBuilder<'db>, Self::Error>;

    async fn union_build(&self, builder: UnionBuilder<'db>) -> Result<Type<'db>, Self::Error>;

    async fn union_pair(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> Result<Type<'db>, Self::Error>;

    async fn intersection_add(
        &self,
        builder: IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> Result<IntersectionBuilder<'db>, Self::Error>;

    async fn intersection_build(
        &self,
        builder: IntersectionBuilder<'db>,
    ) -> Result<Type<'db>, Self::Error>;
}

pub(in crate::types) fn descriptor_get_ingredient(
    db: &dyn Db,
) -> &salsa::plumbing::function::IngredientImpl<TryCallDunderGetInnerConfiguration> {
    try_call_dunder_get_inner::fn_ingredient_(db, db.zalsa())
}

#[salsa::tracked(configuration = (pub(in crate::types) TryCallDunderGetInnerConfiguration), attempt = ReturnOnly, returns(copy), cycle_initial=|_, _, _, _, _, _| Ok(None), heap_size=ruff_memory_usage::heap_size)]
fn try_call_dunder_get_inner<'db>(
    db: &'db dyn Db,
    program: Program<'db>,
    ty: Type<'db>,
    instance: Option<Type<'db>>,
    owner: Type<'db>,
) -> DescriptorResult<'db> {
    super::evaluate(db, program, ty, instance, owner, None)
}

pub(super) struct LegacyInlineEffects<'guard, 'db> {
    pub(super) recursion_guard: Option<&'guard CallableRecursionGuard<'db>>,
}

impl sealed::Sealed for LegacyInlineEffects<'_, '_> {}

impl<'db> DescriptorEffects<'db> for LegacyInlineEffects<'_, 'db> {
    type Error = Infallible;

    async fn checkpoint(&self) -> Result<(), Infallible> {
        Ok(())
    }

    async fn slot_value(
        &self,
        db: &'db dyn Db,
        descriptor: SlotDescriptorType<'db>,
    ) -> Result<Type<'db>, Infallible> {
        Ok(descriptor.value_type(db))
    }

    async fn union_parts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        union: UnionType<'db>,
    ) -> Result<(UnionBuilder<'db>, &'db [Type<'db>]), Infallible> {
        Ok((
            UnionBuilder::new(db, env).or_recursively_defined(union.recursively_defined(db)),
            union.elements(db),
        ))
    }

    async fn intersection_parts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        intersection: IntersectionType<'db>,
    ) -> Result<(IntersectionBuilder<'db>, &'db FxOrderSet<Type<'db>>), Infallible> {
        Ok((IntersectionBuilder::new(db, env), intersection.positive(db)))
    }

    async fn next_descriptor(
        &self,
        requests: &mut impl Iterator<Item = DescriptorRequest<'db>>,
    ) -> Result<Option<DescriptorRequest<'db>>, Infallible> {
        Ok(requests.next())
    }

    async fn call_context(
        &self,
        db: &'db dyn Db,
        request: DescriptorRequest<'db>,
        callable: Type<'db>,
    ) -> Result<DescriptorGetCallContext<'db>, Infallible> {
        Ok(DescriptorGetCallContext::new(
            db,
            request.ty,
            callable,
            request.instance,
            request.owner,
        ))
    }

    fn function_like(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Self::Error>> {
        ready(Ok(request.ty.function_like_dunder_get(
            db,
            env,
            request.instance,
            Some(request.owner),
        )))
    }

    fn protocol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> impl Future<Output = Result<DescriptorResult<'db>, Self::Error>> {
        ready(Ok(if self.recursion_guard.is_some() {
            super::evaluate(
                db,
                env.program(db),
                request.ty,
                request.instance,
                request.owner,
                self.recursion_guard,
            )
        } else {
            try_call_dunder_get_inner(
                db,
                env.program(db),
                request.ty,
                request.instance,
                request.owner,
            )
        }))
    }

    fn declare_descriptors(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _requests: impl Iterator<Item = DescriptorRequest<'db>> + Clone,
    ) -> impl Future<Output = Result<(), Self::Error>> {
        ready(Ok(()))
    }

    fn descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> impl Future<Output = Result<DescriptorResult<'db>, Self::Error>> {
        ready(Ok(request.ty.try_call_dunder_get_with_recursion_guard(
            db,
            env,
            request.instance,
            request.owner,
            self.recursion_guard,
        )))
    }

    fn union_like(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<UnionType<'db>>, Self::Error>> {
        ready(Ok(ty.as_union_like(db)))
    }

    fn class_member(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorMemberRequest<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Self::Error>> {
        ready(Ok(request
            .ty
            .class_member_with_policy(db, env, "__get__", request.policy)
            .place))
    }

    fn data_descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Self::Error>> {
        ready(Ok(ty.is_data_descriptor(db, env)))
    }

    fn none_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(Type::none(db, env)))
    }

    fn invoke(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorInvocationRequest<'db>,
    ) -> impl Future<Output = Result<Result<Bindings<'db>, CallError<'db>>, Self::Error>> {
        #[cfg(test)]
        let observation =
            crate::types::constructor::expansion_probe::descriptor_observation::descriptor(
                request.callable,
                request.arguments,
            );
        let result = request.callable.try_call_with_recursion_guard(
            db,
            env,
            &CallArguments::positional(request.arguments),
            self.recursion_guard,
        );
        #[cfg(test)]
        crate::types::constructor::expansion_probe::descriptor_observation::event(
            "DescriptorInvocationResult",
            (
                result.is_ok(),
                crate::types::constructor::expansion_probe::stopped(db),
            ),
        );
        #[cfg(test)]
        drop(observation);
        ready(Ok(result))
    }

    fn bindings_origin(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
        arguments: &[Type<'db>; 3],
    ) -> impl Future<Output = Result<DescriptorOrigin<'db>, Self::Error>> {
        #[cfg(test)]
        if crate::types::constructor::expansion_probe::descriptor_observation::enabled() {
            crate::types::constructor::expansion_probe::descriptor_observation::event(
                "DescriptorBindingsOrigin",
                (
                    arguments.map(
                        crate::types::constructor::expansion_probe::descriptor_observation::key,
                    ),
                    crate::types::constructor::expansion_probe::stopped(db),
                ),
            );
        }
        ready(Ok(bindings.descriptor_origin(db, env, arguments)))
    }

    fn bindings_return_type(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &Bindings<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        #[cfg(test)]
        crate::types::constructor::expansion_probe::descriptor_observation::event(
            "DescriptorBindingsReturnType",
            crate::types::constructor::expansion_probe::stopped(db),
        );
        ready(Ok(bindings.return_type(db, env)))
    }

    fn merge_origins(
        &self,
        db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> impl Future<Output = Result<DescriptorOrigin<'db>, Self::Error>> {
        ready(Ok(left.merge(db, right)))
    }

    fn union_add(
        &self,
        builder: UnionBuilder<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<UnionBuilder<'db>, Self::Error>> {
        ready(Ok(builder.add(ty)))
    }

    fn union_build(
        &self,
        builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(builder.build()))
    }

    fn union_pair(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        left: Type<'db>,
        right: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(UnionType::from_two_elements(db, env, left, right)))
    }

    fn intersection_add(
        &self,
        mut builder: IntersectionBuilder<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<IntersectionBuilder<'db>, Self::Error>> {
        builder.add_positive_in_place(ty);
        ready(Ok(builder))
    }

    fn intersection_build(
        &self,
        builder: IntersectionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Self::Error>> {
        ready(Ok(builder.build()))
    }
}

#[cfg(test)]
mod tests;
