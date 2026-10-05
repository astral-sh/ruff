//! Descriptor dependencies admitted by the queued evaluator.

use std::future::{Future, ready};

use super::effects::{self, DescriptorEffects};
use super::{
    DescriptorInvocationRequest, DescriptorMemberRequest, DescriptorRequest, DescriptorResult,
};
use crate::place::Place;
use crate::types::call::{Bindings, CallError};
use crate::types::callable::scheduled_probe::{Boundary, Router};
use crate::types::{
    DescriptorGetCallContext, DescriptorOrigin, IntersectionBuilder, IntersectionType,
    KnownInstanceType, SlotDescriptorType, Type, UnionBuilder, UnionType,
};
use crate::{Db, FxOrderSet, ProgramEnvironment};

#[derive(Clone, Copy, Debug, Eq, PartialEq, salsa::SalsaValue)]
pub(crate) enum DescriptorEffect {
    FunctionLikeBinding,
    UnionLikeResolution,
    ClassMember,
    DataDescriptor,
    NoneType,
    BindingsOrigin,
    BindingsReturnType,
    MergeOrigins,
    UnionAdd,
    UnionBuild,
    UnionPair,
    IntersectionAdd,
    IntersectionBuild,
}

pub(in crate::types) struct QueuedDescriptorEffects<'eval, 'db, 'c> {
    pub(in crate::types) router: &'eval Router<'db, 'c>,
    pub(in crate::types) parent: DescriptorRequest<'db>,
}

impl effects::sealed::Sealed for QueuedDescriptorEffects<'_, '_, '_> {}

impl<'db> DescriptorEffects<'db> for QueuedDescriptorEffects<'_, 'db, '_> {
    type Error = Boundary;

    async fn checkpoint(&self) -> Result<(), Boundary> {
        Ok(())
    }

    async fn slot_value(
        &self,
        db: &'db dyn Db,
        descriptor: SlotDescriptorType<'db>,
    ) -> Result<Type<'db>, Boundary> {
        Ok(descriptor.value_type(db))
    }

    async fn union_parts(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        union: UnionType<'db>,
    ) -> Result<(UnionBuilder<'db>, &'db [Type<'db>]), Boundary> {
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
    ) -> Result<(IntersectionBuilder<'db>, &'db FxOrderSet<Type<'db>>), Boundary> {
        Ok((IntersectionBuilder::new(db, env), intersection.positive(db)))
    }

    async fn next_descriptor(
        &self,
        requests: &mut impl Iterator<Item = DescriptorRequest<'db>>,
    ) -> Result<Option<DescriptorRequest<'db>>, Boundary> {
        Ok(requests.next())
    }

    async fn call_context(
        &self,
        db: &'db dyn Db,
        request: DescriptorRequest<'db>,
        callable: Type<'db>,
    ) -> Result<DescriptorGetCallContext<'db>, Boundary> {
        Ok(DescriptorGetCallContext::new(
            db,
            request.ty,
            callable,
            request.instance,
            request.owner,
        ))
    }

    fn declare_descriptors(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        requests: impl Iterator<Item = DescriptorRequest<'db>> + Clone,
    ) -> impl Future<Output = Result<(), Boundary>> {
        for request in requests {
            self.router.declare_descriptor(request);
        }
        ready(Ok(()))
    }

    async fn descriptor(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Boundary> {
        self.router.descriptor_demand(self.parent, request).await
    }

    fn function_like(
        &self,
        db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> impl Future<Output = Result<Option<Type<'db>>, Boundary>> {
        ready(match request.ty {
            Type::Callable(callable) if !callable.is_method_like(db) => Ok(None),
            Type::Union(_)
            | Type::TypeAlias(_)
            | Type::FunctionLiteral(_)
            | Type::Callable(_)
            | Type::KnownInstance(KnownInstanceType::MethodWrapper(_)) => Err(
                Boundary::DescriptorEffect(DescriptorEffect::FunctionLikeBinding),
            ),
            _ => Ok(None),
        })
    }

    async fn protocol(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        request: DescriptorRequest<'db>,
    ) -> Result<DescriptorResult<'db>, Boundary> {
        super::evaluate_with_effects(db, env, request, self).await
    }

    fn union_like(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> impl Future<Output = Result<Option<UnionType<'db>>, Boundary>> {
        ready(match ty {
            Type::Union(union) => Ok(Some(union)),
            Type::TypeAlias(_)
            | Type::Recursive(_)
            | Type::RecursiveVar(_)
            | Type::NewTypeInstance(_) => Err(Boundary::DescriptorEffect(
                DescriptorEffect::UnionLikeResolution,
            )),
            _ => Ok(None),
        })
    }

    fn class_member(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _request: DescriptorMemberRequest<'db>,
    ) -> impl Future<Output = Result<Place<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(
            DescriptorEffect::ClassMember,
        )))
    }

    fn data_descriptor(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<bool, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(
            DescriptorEffect::DataDescriptor,
        )))
    }

    fn none_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(DescriptorEffect::NoneType)))
    }

    async fn invoke(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DescriptorInvocationRequest<'db>,
    ) -> Result<Result<Bindings<'db>, CallError<'db>>, Boundary> {
        self.router
            .descriptor_invocation_demand(self.parent, request)
            .await
    }

    fn bindings_origin(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: &Bindings<'db>,
        _arguments: &[Type<'db>; 3],
    ) -> impl Future<Output = Result<DescriptorOrigin<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(
            DescriptorEffect::BindingsOrigin,
        )))
    }

    fn bindings_return_type(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: &Bindings<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(
            DescriptorEffect::BindingsReturnType,
        )))
    }

    fn merge_origins(
        &self,
        _db: &'db dyn Db,
        left: DescriptorOrigin<'db>,
        right: DescriptorOrigin<'db>,
    ) -> impl Future<Output = Result<DescriptorOrigin<'db>, Boundary>> {
        ready(match (left.dispatches, right.dispatches) {
            (Some(left), Some(right)) if left != right => {
                Err(Boundary::DescriptorEffect(DescriptorEffect::MergeOrigins))
            }
            _ => Ok(DescriptorOrigin {
                dispatches: left.dispatches.or(right.dispatches),
                incomplete: left.incomplete || right.incomplete,
                return_contains_recursive_recovery: left.return_contains_recursive_recovery
                    || right.return_contains_recursive_recovery,
            }),
        })
    }

    fn union_add(
        &self,
        _builder: UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<UnionBuilder<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(DescriptorEffect::UnionAdd)))
    }

    fn union_build(
        &self,
        _builder: UnionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(
            DescriptorEffect::UnionBuild,
        )))
    }

    fn union_pair(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _left: Type<'db>,
        _right: Type<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(DescriptorEffect::UnionPair)))
    }

    fn intersection_add(
        &self,
        _builder: IntersectionBuilder<'db>,
        _ty: Type<'db>,
    ) -> impl Future<Output = Result<IntersectionBuilder<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(
            DescriptorEffect::IntersectionAdd,
        )))
    }

    fn intersection_build(
        &self,
        _builder: IntersectionBuilder<'db>,
    ) -> impl Future<Output = Result<Type<'db>, Boundary>> {
        ready(Err(Boundary::DescriptorEffect(
            DescriptorEffect::IntersectionBuild,
        )))
    }
}
