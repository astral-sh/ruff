//! Admitted constructor preparation using retained bindings and shared signature decisions.

use salsa::execution_probe::{RunError, RunResult};

use super::callable_guard::GuardedPreparationEffects;
use super::class_selection::FixedFieldCopy;
use super::{SourceAccess, SourceEffects};
use crate::types::call::Bindings;
use crate::types::call::bind::ConstructorCallableKind;
use crate::types::call::bind::constructor_context_walk::{
    apply_class_context_with, set_constructor_instance_with,
};
use crate::types::call::bind::constructor_preparation::{
    self, ConstructorBindingStorageEffects, ConstructorReturnEffects,
    ReceiverBindingEffects, attach_downstream_with, bake_bindings_receivers_with,
    bind_initializer_self_with, bind_new_with, constructor_return_with, wrap_constructor_with,
};
use crate::types::call::bind::ownership::{
    CloneBindingsEffects, pristine_bindings_retirement_work,
};
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::generics::GenericContext;
use crate::types::signatures::Signature;
use crate::types::signatures::constructor_preparation::ConstructorSignatureEffects;
use crate::types::typevar::{TypeVarIdentity, TypeVarKind};
use crate::types::{BoundMethodType, BoundTypeVarInstance, Type};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorReturnEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.local(16, 3 * size_of::<Type<'db>>(), || ()).await
    }

    async fn resolve_alias(&self, ty: Type<'db>) -> RunResult<Type<'db>> {
        NominalSelectionEffects::resolve_alias(self, ty).await
    }

    async fn is_self(&self, ty: BoundTypeVarInstance<'db>) -> RunResult<bool> {
        let identity = ConstructorReturnEffects::identity(self, ty).await?;
        let kind = self
            .field_with_profile(
                identity
                    .field_requests(self.access.endpoint().field_request_context())
                    .kind(),
                &FixedFieldCopy,
            )
            .await?;
        self.local(1, size_of::<bool>(), || {
            matches!(kind, TypeVarKind::TypingSelf)
        })
        .await
    }

    async fn identity(&self, ty: BoundTypeVarInstance<'db>) -> RunResult<TypeVarIdentity<'db>> {
        let typevar = self
            .field_with_profile(
                ty.field_requests(self.access.endpoint().field_request_context())
                    .typevar(),
                &FixedFieldCopy,
            )
            .await?;
        self.field_with_profile(
            typevar
                .field_requests(self.access.endpoint().field_request_context())
                .identity(),
            &FixedFieldCopy,
        )
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ReceiverBindingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    #[cfg(test)]
    fn constructor_wrap_before(&self) {
        crate::types::infer::source_runtime::tests::constructor_preparation::observe_before(
            crate::types::infer::source_runtime::tests::constructor_preparation::Stage::ConstructorWrap,
        );
    }

    #[cfg(test)]
    fn constructor_wrap_after(&self) {
        crate::types::infer::source_runtime::tests::constructor_preparation::observe_after(
            crate::types::infer::source_runtime::tests::constructor_preparation::Stage::ConstructorWrap,
        );
    }

    async fn typing_self_type(
        &self,
        _db: &'db dyn Db,
        method: BoundMethodType<'db>,
    ) -> RunResult<Type<'db>> {
        self.constructor_typing_self_type(method).await
    }

    async fn normalized_return(
        &self,
        _db: &'db dyn Db,
        signature: &Signature<'db>,
        constructor: Option<(ConstructorCallableKind, Type<'db>)>,
    ) -> RunResult<Type<'db>> {
        match constructor {
            Some((kind, instance)) => {
                self.allocate_future(|| constructor_return_with(signature, kind, instance, self))
                    .await?
                    .await
            }
            None => {
                self.local(1, size_of::<Type<'db>>(), || signature.return_ty)
                    .await
            }
        }
    }

    async fn retire_downstream(
        &self,
        downstream: &mut Option<Box<Bindings<'db>>>,
    ) -> RunResult<()> {
        ConstructorSignatureEffects::local(
            self,
            Some(2),
            Some(size_of::<Option<Box<Bindings<'db>>>>()),
            || *downstream = None,
        )
        .await
    }

    async fn constructor_descendants(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        instance_type: Type<'db>,
    ) -> RunResult<()> {
        self.allocate_future(|| set_constructor_instance_with(db, bindings, instance_type, self))
            .await?
            .await
    }

    async fn merge_generic_context(
        &self,
        _db: &'db dyn Db,
        existing: Option<GenericContext<'db>>,
        incoming: GenericContext<'db>,
    ) -> RunResult<GenericContext<'db>> {
        match self
            .local_with_fixed_transfers(2, 0, || existing)
            .await?
        {
            None => {
                self.local_with_fixed_transfers(1, 0, || incoming)
                    .await
            }
            Some(existing) => {
                self.boxed_future_with_fixed_transfers(Ok((0, 0)), || {
                    self.merge_return_context(existing, incoming)
                })
                .await?
                .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CloneBindingsEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        operation: impl FnOnce() -> T,
    ) -> RunResult<T> {
        ConstructorSignatureEffects::local(self, work, bytes, operation).await
    }

    async fn signature_clone(&self, signature: &Signature<'db>) -> RunResult<Signature<'db>> {
        let retirement = ConstructorSignatureEffects::signature_retirement(self, signature).await?;
        let bytes = self
            .local(1, size_of::<Option<usize>>(), || {
                signature.clone_requested_bytes()
            })
            .await?;
        ConstructorSignatureEffects::local(
            self,
            retirement.checked_add(8),
            bytes.and_then(|bytes| bytes.checked_add(size_of::<Signature<'db>>())),
            || signature.clone(),
        )
        .await
    }

    #[cfg(test)]
    fn clone_before(&self) {
        crate::types::infer::source_runtime::tests::constructor_preparation::observe_before(
            crate::types::infer::source_runtime::tests::constructor_preparation::Stage::DownstreamClone,
        );
    }

    #[cfg(test)]
    fn clone_after(&self) {
        crate::types::infer::source_runtime::tests::constructor_preparation::observe_after(
            crate::types::infer::source_runtime::tests::constructor_preparation::Stage::DownstreamClone,
        );
    }

    #[cfg(test)]
    fn downstream_install_before(&self) {
        crate::types::infer::source_runtime::tests::constructor_preparation::observe_before(
            crate::types::infer::source_runtime::tests::constructor_preparation::Stage::DownstreamInstall,
        );
    }

    #[cfg(test)]
    fn downstream_install_after(&self) {
        crate::types::infer::source_runtime::tests::constructor_preparation::observe_after(
            crate::types::infer::source_runtime::tests::constructor_preparation::Stage::DownstreamInstall,
        );
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Bakes bound receivers while retaining every original overload until its replacement is admitted.
    pub(super) async fn bake_constructor_receivers(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> RunResult<()> {
        self.allocate_future(|| bake_bindings_receivers_with(self.db(), env, bindings, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorBindingStorageEffects<'db>
    for GuardedPreparationEffects<'_, '_, '_, 'run, 'db, A>
{
    async fn bind_new(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
        self_type: Type<'db>,
        instance_type: Type<'db>,
    ) -> RunResult<()> {
        self.source
            .allocate_future(|| {
                bind_new_with(db, env, bindings, self_type, instance_type, self.source)
            })
            .await?
            .await
    }

    async fn wrap_constructor(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        instance_type: Type<'db>,
        kind: ConstructorCallableKind,
    ) -> RunResult<()> {
        self.source
            .allocate_future(|| {
                wrap_constructor_with(db, bindings, instance_type, kind, self.source)
            })
            .await?
            .await
    }

    async fn mark_unbound(
        &self,
        bindings: &mut Bindings<'db>,
        kind: ConstructorCallableKind,
    ) -> RunResult<()> {
        self.source
            .local(2, size_of::<bool>(), || {
                constructor_preparation::mark_unbound(bindings, kind)
            })
            .await
    }

    async fn bind_initializer_self(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        bindings: &mut Bindings<'db>,
    ) -> RunResult<()> {
        self.source
            .allocate_future(|| bind_initializer_self_with(db, env, bindings, self.source))
            .await?
            .await
    }

    async fn attach_downstream(
        &self,
        bindings: &mut Bindings<'db>,
        downstream: &Bindings<'db>,
    ) -> RunResult<()> {
        self.source
            .allocate_future(|| attach_downstream_with(bindings, downstream, self.source))
            .await?
            .await
    }

    async fn apply_class_context(
        &self,
        db: &'db dyn Db,
        bindings: &mut Bindings<'db>,
        context: Option<GenericContext<'db>>,
    ) -> RunResult<()> {
        self.source
            .allocate_future(|| apply_class_context_with(db, bindings, context, self.source))
            .await?
            .await
    }

    async fn fallback(
        &self,
        receiver: Type<'db>,
        context: Option<GenericContext<'db>>,
        return_type: Type<'db>,
    ) -> RunResult<Bindings<'db>> {
        let (parameter_work, parameter_bytes) =
            crate::types::signatures::constructor_preparation::gradual_parameters_quote().ok_or(
                RunError::Contract("constructor fallback quotation overflow"),
            )?;
        let work = SourceEffects::<A>::checked(
            parameter_work
                .checked_add(32)
                .and_then(|work| work.checked_add(pristine_bindings_retirement_work(1)?)),
        )?;
        let bytes = SourceEffects::<A>::checked(
            parameter_bytes.checked_add(constructor_preparation::fallback_representation_bytes()),
        )?;
        ConstructorSignatureEffects::local(self.source, Some(work), Some(bytes), || {
            constructor_preparation::fallback(receiver, context, return_type)
        })
        .await
    }

    async fn transfer(&self, _bindings: &Bindings<'db>) -> RunResult<()> {
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::constructor_preparation::observe_before(
            crate::types::infer::source_runtime::tests::constructor_preparation::Stage::Transfer,
        );
        self.source.local(2, size_of::<Bindings<'db>>() + size_of::<RunResult<Bindings<'db>>>(), || {
            #[cfg(test)]
            crate::types::infer::source_runtime::tests::constructor_preparation::observe_after(
                crate::types::infer::source_runtime::tests::constructor_preparation::Stage::Transfer,
            );
        }).await
    }
}
