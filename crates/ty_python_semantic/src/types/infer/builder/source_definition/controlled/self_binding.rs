//! Self replacements use canonical variable fields and the shared owner-matching decisions.

use std::future::Future;
use std::pin::Pin;
use std::slice;

use salsa::execution_probe::{FieldRequest, FieldRequestContext, RunError, RunResult};

use super::{SourceAccess, SourceEffects};
use crate::types::class_selection;
use crate::types::mapping::self_binding::{
    SelfBindingEffects, SelfBindingWork, prepare_with, sealed, should_bind_with,
};
use crate::types::typevar::bounds::typevar_bounds_with;
use crate::types::typevar::{BindingContext, TypeVarBoundOrConstraints};
use crate::types::{BoundTypeVarInstance, ClassLiteral, ClassType, SelfBinding, Type, TypeVarKind};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Prepares `ty` as a Self replacement, resolving its nominal class before mapping any type.
    pub(in crate::types) async fn prepare_self_binding(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        binding_context: Option<BindingContext<'db>>,
    ) -> RunResult<SelfBinding<'db>> {
        self.self_binding_child(|| prepare_with(self.db(), env, ty, binding_context, self))
            .await
    }

    /// Returns whether `variable` becomes the binding's replacement, checking its binding context
    /// before requesting the replacement class's MRO.
    pub(in crate::types) async fn should_bind_self_mapping(
        &self,
        env: &ProgramEnvironment<'db>,
        binding: &SelfBinding<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.self_binding_child(|| should_bind_with(self.db(), env, binding, variable, self))
            .await
    }

    /// Constructs and awaits a Self-mapping child after admitting its future and result transfers.
    async fn self_binding_child<T, F: Future<Output = RunResult<T>>>(
        &self,
        make: impl FnOnce() -> F,
    ) -> RunResult<T> {
        let quote = size_of::<RunResult<T>>()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(size_of::<F>().checked_mul(2)?))
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .map(|bytes| (6, bytes))
            .ok_or(RunError::Contract("Self mapping child quotation overflow"));
        let future: Pin<Box<F>> = self
            .local_quoted_with_fixed_transfers(quote, || Box::pin(make()))
            .await?;
        future.await
    }

    /// Reads a canonical field after admitting its context, request, and returned handle.
    async fn self_binding_field<V, R: FieldRequest<'db>>(
        &self,
        make_fields: impl FnOnce(FieldRequestContext<'db>) -> V,
        make_request: impl FnOnce(V) -> R,
    ) -> RunResult<R::Output> {
        let context = self
            .local_with_fixed_transfers(8, 0, || self.access.endpoint().field_request_context())
            .await?;
        let fields = self
            .local_with_fixed_transfers(3, 0, || make_fields(context))
            .await?;
        let request = self
            .local_with_fixed_transfers(8, 0, || make_request(fields))
            .await?;
        let value = self.field(request).await?;
        self.local_with_fixed_transfers(2, 0, || value).await
    }

    /// Returns a class's literal, reading generic origins and preserving non-generic literals.
    async fn self_binding_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<ClassLiteral<'db>> {
        match self.local_with_fixed_transfers(3, 0, || class).await? {
            ClassType::NonGeneric(literal) => {
                self.local_with_fixed_transfers(2, 0, || literal).await
            }
            ClassType::Generic(alias) => {
                let origin = self
                    .self_binding_field(
                        |context| alias.field_requests(context),
                        |fields| fields.origin(),
                    )
                    .await?;
                self.local_with_fixed_transfers(2, 0, || ClassLiteral::Static(origin))
                    .await
            }
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for SourceEffects<'_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SelfBindingEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, work: SelfBindingWork) -> RunResult<()> {
        let (operations, bytes) = match work {
            SelfBindingWork::Prepare => (
                10,
                2 * size_of::<SelfBinding<'db>>() + size_of::<Option<ClassLiteral<'db>>>(),
            ),
            SelfBindingWork::MatchVariable => (
                16,
                size_of::<slice::Iter<'db, ClassLiteral<'db>>>()
                    + 2 * size_of::<Option<ClassLiteral<'db>>>()
                    + 2 * size_of::<Option<BindingContext<'db>>>()
                    + 3 * size_of::<bool>(),
            ),
            SelfBindingWork::MroMember => {
                (4, 2 * size_of::<ClassLiteral<'db>>() + size_of::<bool>())
            }
        };
        self.local_with_fixed_transfers(operations, bytes, || ())
            .await
    }

    async fn is_self(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let variable = self
            .self_binding_field(
                |context| variable.field_requests(context),
                |fields| fields.typevar(),
            )
            .await?;
        let identity = self
            .self_binding_field(
                |context| variable.field_requests(context),
                |fields| fields.identity(),
            )
            .await?;
        let kind = self
            .self_binding_field(
                |context| identity.field_requests(context),
                |fields| fields.kind(),
            )
            .await?;
        self.local_with_fixed_transfers(3, 0, || match kind {
            TypeVarKind::TypingSelf => true,
            TypeVarKind::LegacyTypeVar
            | TypeVarKind::Pep695TypeVar
            | TypeVarKind::LegacyParamSpec
            | TypeVarKind::Pep695ParamSpec
            | TypeVarKind::LegacyTypeVarTuple
            | TypeVarKind::Pep695TypeVarTuple
            | TypeVarKind::Pep613Alias => false,
        })
        .await
    }

    async fn binding_context(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BindingContext<'db>> {
        let identity = self
            .self_binding_field(
                |context| variable.identity_request(context),
                |request| request,
            )
            .await?;
        self.local_with_fixed_transfers(2, 0, || identity.binding_context)
            .await
    }

    async fn nominal_owner(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<Option<ClassLiteral<'db>>> {
        let class = self
            .self_binding_child(|| class_selection::nominal_class_with(ty, self))
            .await?;
        match self.local_with_fixed_transfers(3, 0, || class).await? {
            Some(class) => {
                let literal = self.self_binding_class_literal(class).await?;
                self.local_with_fixed_transfers(2, 0, || Some(literal))
                    .await
            }
            None => self.local_with_fixed_transfers(2, 0, || None).await,
        }
    }

    async fn self_owner(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<ClassLiteral<'db>>> {
        let variable = self
            .self_binding_field(
                |context| variable.field_requests(context),
                |fields| fields.typevar(),
            )
            .await?;
        let bounds = self
            .self_binding_child(|| typevar_bounds_with(variable, env, self))
            .await?;
        match self.local_with_fixed_transfers(3, 0, || bounds).await? {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => {
                SelfBindingEffects::nominal_owner(self, db, env, bound).await
            }
            Some(TypeVarBoundOrConstraints::Constraints(_)) | None => {
                self.local_with_fixed_transfers(2, 0, || None).await
            }
        }
    }

    async fn mro_literals(
        &self,
        _db: &'db dyn Db,
        class: ClassLiteral<'db>,
    ) -> RunResult<&'db [ClassLiteral<'db>]> {
        self.self_binding_child(|| self.access.class_mro_literals(class))
            .await
    }
}
