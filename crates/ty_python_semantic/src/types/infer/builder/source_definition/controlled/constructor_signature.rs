//! Admitted receiver-signature storage and the controlled semantic children it requires.

use std::borrow::Cow;

use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::class_selection::NominalSelectionEffects;
use crate::types::constraints::OwnedConstraintSet;
use crate::types::constraints::source::SourceStructuralResult;
use crate::types::generics::GenericContext;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::signatures::constructor_preparation::{
    ConstructorSignatureEffects, ConstructorSignatureOperation,
};
use crate::types::signatures::{Parameters, Signature};
use crate::types::mapping::OwnedTypeMapping;
use crate::types::relation::source::assignability_condition;
use crate::types::relation::source::resources::RelationResourceAccess;
use crate::types::typevar::TypeVarConstraints;
use crate::types::typevar::bounds::typevar_bounds_with;
use crate::types::{
    BindingContext, BoundTypeVarIdentity, BoundTypeVarInstance, Type, TypeVarBoundOrConstraints,
    TypeVarKind,
};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorSignatureEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        requested_bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = (|| {
            let work = Self::checked(work)?;
            let bytes = Self::checked(requested_bytes)?;
            let bytes = Self::checked(
                bytes
                    .checked_add(size_of_val(&action))
                    .and_then(|bytes| bytes.checked_add(size_of::<T>())),
            )?;
            Ok((work, bytes))
        })();
        self.local_quoted(quote, action).await
    }

    async fn signature_retirement(&self, signature: &Signature<'db>) -> RunResult<usize> {
        let work = ConstructorSignatureEffects::local(self, Some(64), Some(0), || {
            signature.retirement_work()
        })
        .await?;
        Self::checked(work)
    }

    async fn contains_self(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        self.contains_self_source(ty).await
    }

    async fn synthetic_receiver(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::ConstructorSignature(
            ConstructorSignatureOperation::SyntheticReceiver,
        ))
        .await
    }

    async fn bind_self_type(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        self_type: Type<'db>,
        context: Option<BindingContext<'db>>,
    ) -> RunResult<Type<'db>> {
        let binding = self.prepare_self_binding(env, self_type, context).await?;
        let mapping = self.local_with_fixed_transfers(1, 0, || OwnedTypeMapping::BindSelf(binding)).await?;
        self.apply_mapping(ty, self.program, mapping).await
    }

    async fn bind_self_signature_types(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        parameters: &mut Parameters<'db>,
        return_type: &mut Type<'db>,
        self_type: Type<'db>,
        context: Option<BindingContext<'db>>,
    ) -> RunResult<()> {
        let binding = self.prepare_self_binding(env, self_type, context).await?;
        let mapping = self.local_with_fixed_transfers(1, 0, || OwnedTypeMapping::BindSelf(binding)).await?;
        let mapped_parameters = self.apply_parameter_mapping(parameters, self.program, mapping).await?;
        self.local_with_fixed_transfers(2, 0, || *parameters = mapped_parameters).await?;
        let mapped_return = self.apply_mapping(*return_type, self.program, mapping).await?;
        self.local_with_fixed_transfers(2, 0, || *return_type = mapped_return).await
    }

    async fn resolve_alias(&self, _db: &'db dyn Db, ty: Type<'db>) -> RunResult<Type<'db>> {
        NominalSelectionEffects::resolve_alias(self, ty).await
    }

    async fn receiver_violates_domain(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _receiver: Type<'db>,
        _typevar: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ConstructorSignature(
            ConstructorSignatureOperation::ReceiverDomain,
        ))
        .await
    }

    async fn receiver_constraint(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        receiver: Type<'db>,
        annotation: Type<'db>,
    ) -> RunResult<Cow<'db, OwnedConstraintSet<'db>>> {
        self.owned_receiver_constraints(env, receiver, annotation).await
    }

    async fn clone_constraints(
        &self,
        constraints: &OwnedConstraintSet<'db>,
    ) -> RunResult<OwnedConstraintSet<'db>> {
        let retirement = ConstructorSignatureEffects::local(self, Some(48), Some(0), || {
            constraints.retirement_work()
        })
        .await?;
        ConstructorSignatureEffects::local(
            self,
            retirement.and_then(|work| work.checked_add(3)),
            Some(0),
            || constraints.clone(),
        )
        .await
    }

    async fn intersect_constraints(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: &OwnedConstraintSet<'db>,
        second: &OwnedConstraintSet<'db>,
    ) -> RunResult<Option<OwnedConstraintSet<'db>>> {
        self.receiver_constraint_child(|| self.environment_program(env)).await?;
        let merged = self.receiver_constraint_child(|| {
            self.access.resources().intersect_owned_terminals(self.db(), first, second, self)
        }).await?;
        match merged {
            SourceStructuralResult::Complete(value) => {
                self.local_with_fixed_transfers(3, 0, || {
                    (!value.is_trivially_always_satisfied()).then_some(value)
                }).await
            }
            SourceStructuralResult::Unsupported => {
                self.unavailable(SourceOperation::ConstructorSignature(
                    ConstructorSignatureOperation::ReceiverConstraintMerge,
                ))
                .await
            }
        }
    }

    async fn remove_self(
        &self,
        _db: &'db dyn Db,
        _context: GenericContext<'db>,
        _binding_context: Option<BindingContext<'db>>,
    ) -> RunResult<GenericContext<'db>> {
        self.unavailable(SourceOperation::ConstructorSignature(
            ConstructorSignatureOperation::GenericContextSelfRemoval,
        ))
        .await
    }

    async fn is_self(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let fields = self.access.endpoint().field_request_context();
        let variable = self
            .field(variable.field_requests(fields).typevar())
            .await?;
        let identity = self
            .field(variable.field_requests(fields).identity())
            .await?;
        let kind = self.field(identity.field_requests(fields).kind()).await?;
        ConstructorSignatureEffects::local(self, Some(1), Some(0), || match kind {
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

    async fn upper_bound(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let bounds =
            ConstructorSignatureEffects::bound_or_constraints(self, db, env, variable).await?;
        ConstructorSignatureEffects::local(self, Some(1), Some(0), || match bounds {
            Some(TypeVarBoundOrConstraints::UpperBound(bound)) => Some(bound),
            Some(TypeVarBoundOrConstraints::Constraints(_)) | None => None,
        })
        .await
    }

    async fn is_assignable(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        source: Type<'db>,
        target: Type<'db>,
    ) -> RunResult<bool> {
        self.boxed_future_with_fixed_transfers(Ok((0, 0)), || {
            assignability_condition(db, env, source, target, self)
        })
        .await?
        .await
    }

    async fn variables(
        &self,
        _db: &'db dyn Db,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        self.field(context.variables_request(self.access.endpoint().field_request_context()))
            .await
    }

    async fn identity(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<BoundTypeVarIdentity<'db>> {
        self.field(variable.identity_request(self.access.endpoint().field_request_context()))
            .await
    }

    async fn bound_or_constraints(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarBoundOrConstraints<'db>>> {
        let fields = self.access.endpoint().field_request_context();
        let variable = self
            .field(variable.field_requests(fields).typevar())
            .await?;
        self.allocate_future(|| typevar_bounds_with(variable, env, self))
            .await?
            .await
    }

    async fn constraint_elements(
        &self,
        _db: &'db dyn Db,
        constraints: TypeVarConstraints<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let elements = self
            .field(
                constraints
                    .field_requests(self.access.endpoint().field_request_context())
                    .elements(),
            )
            .await?;
        ConstructorSignatureEffects::local(self, Some(1), Some(0), || &**elements).await
    }

    async fn default_type(
        &self,
        _db: &'db dyn Db,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.access.bound_typevar_default(variable).await
    }

    async fn specialize_unused_self(
        &self,
        _db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        signature: &Signature<'db>,
        variable: BoundTypeVarInstance<'db>,
        self_type: Type<'db>,
    ) -> RunResult<Signature<'db>> {
        let program = self
            .boxed_future_with_fixed_transfers(Ok((0, 0)), || self.environment_program(env))
            .await?
            .await?;
        let mapping = self
            .local_with_fixed_transfers(3, 0, || OwnedTypeMapping::Single {
                variable,
                replacement: self_type,
            })
            .await?;
        self.boxed_future_with_fixed_transfers(Ok((0, 0)), || {
            self.apply_signature_mapping(signature, program, mapping)
        })
        .await?
        .await
    }
}
