//! Callable binding preparation retains the invocation's guard through admitted entry effects.
//!
//! The exact entry recipe records dependencies and distinguishes active keys. Successful active-key
//! insertions install removal receipts in a scope retained by preparation until its children finish.
//! Recorded dependencies remain guard/cache state after the visit ends.

use std::convert::Infallible;
use std::ops::ControlFlow;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};
use ty_python_core::definition::Definition;

#[cfg(test)]
use crate::types::infer::source_runtime::tests::callable_guard as guard_tests;

use super::local_transfer::boxed_future_with_fixed_transfers_at;
use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::call::Bindings;
use crate::types::call::preparation::bound_method::bound_method_bindings_with;
use crate::types::call::preparation::known_class::KnownClassBindingEffects;
use crate::types::call::preparation::{
    BindingPreparationDependency, BindingPreparationEffects, BindingPreparationFacts,
    bindings_body_with, bindings_with,
};
use crate::types::class::identity::{ClassIdentityEffects, class_identity_specialization_with};
use crate::types::constructor::bindings::constructor_bindings_with;
use crate::types::cyclic::entry::{
    CallableDefinitionEffects, CallableEntryDecision, CallableEntryFacts,
    CallableGuardEntryEffects, CallableGuardOperation, ConstructorGuardEffects,
    ExactCallableEntryDecision, RecursiveIdentityEffects, callable_definition_with,
    callable_enter_exact_in_place_with,
    constructor_guard_cache_with, recursive_identity_with,
};
use crate::types::cyclic::guard_storage::{
    CallableGuardStorageControl, CallableGuardStorageError, CallableGuardStorageWork,
};
use crate::types::cyclic::{
    CallableExpansion, CallableRecursionGuard, CallableVisitScope, DefinitionUse,
    DescriptorDispatchScope, RecursiveDefinition, TypeIdentity,
};
use crate::types::function::{FunctionLiteral, FunctionType};
use crate::types::instance::{NominalClassFacts, nominal_class_with};
use crate::types::newtype::NewType;
use crate::types::signatures::source::parameters_storage_quote;
use crate::types::{
    ClassLiteral, ClassType, DescriptorDispatches, DescriptorOrigin, GenericContext,
    NominalInstanceType, ProtocolInstanceType, Signature, Specialization, StaticClassLiteral, Type,
};
use crate::{Db, ProgramEnvironment};

/// Quotes `(logical work, requested bytes)` for [`callable_enter_exact_in_place_with`] with
/// `RunError` effects. [`boxed_future_with_fixed_transfers_at`] separately funds the future,
/// factory, box, and final decision result;
/// guard effects fund table work and removal. Twelve operations cover key construction, three
/// effect calls, three result checks, two branches, negation, and decision/result construction.
/// Nine more fund three ControlFlow/residual conversions per result check. Carrier counts cover
/// four keys, three effect/scope borrow pairs, returned/consumed inner results and ControlFlow
/// values, one error residual, and four Boolean values. Compile-time evaluation adds no runtime
/// quotation arithmetic; changing the recipe requires revisiting this bound.
pub(super) const EXACT_CALLABLE_ENTRY_QUOTE: (usize, usize) = (
    21,
    4 * size_of::<(CallableExpansion, Type<'static>)>()
        + 6 * size_of::<&()>()
        + 2 * size_of::<RunResult<()>>()
        + 4 * size_of::<RunResult<bool>>()
        + 2 * size_of::<ControlFlow<RunResult<Infallible>, ()>>()
        + 4 * size_of::<ControlFlow<RunResult<Infallible>, bool>>()
        + size_of::<RunResult<Infallible>>()
        + 4 * size_of::<bool>(),
);

struct CallableGuardAdmission<'endpoint, 'run, 'db: 'run> {
    endpoint: &'endpoint TaskEndpoint<'run, 'db>,
}

impl CallableGuardStorageControl for CallableGuardAdmission<'_, '_, '_> {
    type Error = RunError;

    fn admit(&self, work: CallableGuardStorageWork) -> RunResult<()> {
        let quote = work.quote().ok_or(RunError::Contract(
            "callable guard storage quotation overflow",
        ))?;
        self.endpoint.admit_work(quote.work_units)?;
        if quote.requested_payload_bytes != 0 {
            self.endpoint.admit(ExecutionWork::Resource {
                requested_bytes: quote.requested_payload_bytes,
            })?;
        }
        self.endpoint.check_completion()
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    // Only passive results leave the callback. Guard, scope, and snapshot owners stay in the
    // enclosing continuation when the runtime drains children after rejected completion.
    async fn callable_storage<T: Copy>(
        &self,
        mut action: impl FnMut(
            &CallableGuardAdmission<'_, 'run, 'db>,
        ) -> Result<T, CallableGuardStorageError<RunError>>,
    ) -> RunResult<T> {
        let endpoint = self.access.endpoint();
        loop {
            let result = endpoint
                .local_call(|| {
                    // Fund the scalar table snapshot and checked quotation before inspecting it.
                    endpoint.admit_work(64)?;
                    endpoint.check_completion()?;
                    match action(&CallableGuardAdmission { endpoint }) {
                        Ok(result) => Ok(Ok(Some(result))),
                        Err(CallableGuardStorageError::Refused(error)) => Err(error),
                        Err(CallableGuardStorageError::UntrackedGuard) => {
                            Ok(Err(CallableGuardOperation::StorageOrigin))
                        }
                        Err(CallableGuardStorageError::StalePreparation) => Ok(Ok(None)),
                        Err(CallableGuardStorageError::CapacityExhausted) => Err(
                            RunError::Contract("callable guard storage quotation overflow"),
                        ),
                        Err(CallableGuardStorageError::ScopeAlreadyEntered) => {
                            Err(RunError::Contract("callable guard scope already entered"))
                        }
                        Err(CallableGuardStorageError::DifferentGuard) => Err(RunError::Contract(
                            "callable guard scope belongs to another guard",
                        )),
                    }
                })
                .await;
            match result {
                Ok(Some(result)) => return Ok(result),
                // Re-admit against the new snapshot; the previous charge remains consumed.
                Ok(None) => continue,
                Err(operation) => {
                    return self
                        .unavailable(SourceOperation::CallableGuard(operation))
                        .await;
                }
            }
        }
    }

    pub(super) async fn admitted_callable_guard(&self) -> RunResult<CallableRecursionGuard<'db>> {
        let mut owner = None;
        self.callable_storage(|control| {
            owner = Some(CallableRecursionGuard::new_admitted(control)?);
            Ok(())
        })
        .await?;
        owner.ok_or(RunError::Contract("callable guard was not constructed"))
    }

    pub(super) async fn admitted_constructor_guard(
        &self,
        receiver: Type<'db>,
    ) -> RunResult<CallableRecursionGuard<'db>> {
        let use_shared_cache =
            constructor_guard_cache_with(receiver, CallableEntryFacts, self).await?;
        let mut owner = None;
        self.callable_storage(|control| {
            owner = Some(CallableRecursionGuard::new_admitted_with_constructor_cache(
                control,
                use_shared_cache,
            )?);
            Ok(())
        })
        .await?;
        owner.ok_or(RunError::Contract("callable guard was not constructed"))
    }

    async fn callable_cycle_bindings(&self, ty: Type<'db>) -> RunResult<Bindings<'db>> {
        let quote = parameters_storage_quote(2).ok_or(RunError::Contract(
            "callable cycle signature quotation overflow",
        ))?;
        let work = Self::checked(quote.work.checked_add(1))?;
        let bytes = Self::checked(quote.bytes.checked_add(size_of::<Signature<'db>>() * 2))?;
        let mut signature = None;
        self.local(work, bytes, || {
            signature = Some(Signature::unknown());
        })
        .await?;
        let signature = signature.ok_or(RunError::Contract(
            "callable cycle signature was not constructed",
        ))?;
        KnownClassBindingEffects::single_binding(self, ty, signature).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ConstructorGuardEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn constructor_probe_active(&self) -> RunResult<bool> {
        self.local(1, 0, || false).await
    }

    async fn constructor_definition(
        &self,
        receiver: Type<'db>,
    ) -> RunResult<Option<DefinitionUse<'db>>> {
        callable_definition_with(
            receiver,
            CallableExpansion::Upcast,
            CallableEntryFacts,
            self,
        )
        .await
    }

    async fn constructor_unbounded_specialization(
        &self,
        _target: RecursiveDefinition<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::CallableGuard(
            CallableGuardOperation::ConstructorSpecializationProof,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassIdentityEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn generic_context(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.access.class_generic_context(class).await
    }

    async fn identity_alias(
        &self,
        class: StaticClassLiteral<'db>,
        context: GenericContext<'db>,
    ) -> RunResult<ClassType<'db>> {
        let specialization = self.identity_specialization(context).await?;
        let alias = self
            .type_parameter_future(|| self.access.intern_generic_alias(class, specialization))
            .await?
            .await?;
        self.local_with_fixed_transfers(1, 0, || ClassType::Generic(alias)).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CallableDefinitionEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn identity_class(&self, class: ClassLiteral<'db>) -> RunResult<ClassType<'db>> {
        match class {
            ClassLiteral::Static(class) => class_identity_specialization_with(class, self).await,
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => {
                self.local(1, 0, || ClassType::NonGeneric(class)).await
            }
        }
    }

    async fn instance_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<ClassType<'db>> {
        nominal_class_with(instance, NominalClassFacts, self).await
    }

    async fn protocol_class(
        &self,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.unavailable(SourceOperation::CallableGuard(
            CallableGuardOperation::ProtocolOrigin,
        ))
        .await
    }

    async fn static_class(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.static_class_identity(class).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RecursiveIdentityEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn function_literal(
        &self,
        function: FunctionType<'db>,
    ) -> RunResult<FunctionLiteral<'db>> {
        self.field(
            function
                .field_requests(self.access.endpoint().field_request_context())
                .literal(),
        )
        .await
    }

    async fn newtype_definition(&self, newtype: NewType<'db>) -> RunResult<Definition<'db>> {
        self.field(
            newtype
                .field_requests(self.access.endpoint().field_request_context())
                .definition(),
        )
        .await
    }

    async fn recursive_definition(
        &self,
        _ty: Type<'db>,
    ) -> RunResult<Option<RecursiveDefinition<'db>>> {
        self.unavailable(SourceOperation::CallableGuard(
            CallableGuardOperation::RecursiveIdentity,
        ))
        .await
    }

    async fn unbounded_specialization(&self, _target: RecursiveDefinition<'db>) -> RunResult<bool> {
        self.unavailable(SourceOperation::CallableGuard(
            CallableGuardOperation::RecursiveIdentity,
        ))
        .await
    }

    async fn definition(&self, _target: RecursiveDefinition<'db>) -> RunResult<Definition<'db>> {
        self.unavailable(SourceOperation::CallableGuard(
            CallableGuardOperation::RecursiveIdentity,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CallableGuardEntryEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn constructor_probe(
        &self,
        _scope: &mut CallableVisitScope<'_, 'db>,
        _key: (CallableExpansion, Type<'db>),
    ) -> RunResult<Option<CallableEntryDecision>> {
        // Controlled execution reports its own interruption as an outer error.
        self.local(1, 0, || None).await
    }

    async fn record_dependency(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> RunResult<()> {
        self.callable_storage(|control| {
            scope
                .guard()
                .prepare_dependency_insert(key, control)?
                .try_commit()
                .map_err(|_| CallableGuardStorageError::StalePreparation)?;
            #[cfg(test)]
            guard_tests::committed(
                self.db(),
                self.access.endpoint(),
                scope.guard(),
                guard_tests::Stage::Dependency,
                Some(key),
            )
            .map_err(CallableGuardStorageError::Refused)?;
            Ok(())
        })
        .await
    }

    async fn contains_exact(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> RunResult<bool> {
        self.callable_storage(|control| scope.guard().contains_exact_with(key, control))
            .await
    }

    async fn callable_definition(
        &self,
        ty: Type<'db>,
        mode: CallableExpansion,
    ) -> RunResult<Option<DefinitionUse<'db>>> {
        callable_definition_with(ty, mode, CallableEntryFacts, self).await
    }

    async fn has_previous_definition(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
    ) -> RunResult<bool> {
        let mut snapshot = None;
        self.callable_storage(|control| {
            snapshot = Some(scope.guard().active_definition_uses_with(control)?);
            Ok(())
        })
        .await?;
        let snapshot = snapshot.ok_or(RunError::Contract(
            "callable guard definition snapshot was not constructed",
        ))?;
        let work = Self::checked(snapshot.len().checked_add(1))?;
        let bytes = Self::checked(
            snapshot
                .len()
                .checked_mul(size_of::<RecursiveDefinition<'db>>() * 2),
        )?;
        self.local(work, bytes, || {
            snapshot
                .iter()
                .any(|(previous, _)| previous.target() == reference.target())
        })
        .await
    }

    async fn repeated_definition_growth(
        &self,
        _scope: &CallableVisitScope<'_, 'db>,
        _reference: DefinitionUse<'db>,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::CallableGuard(
            CallableGuardOperation::RepeatedDefinitionProof,
        ))
        .await
    }

    async fn dispatch(
        &self,
        scope: &CallableVisitScope<'_, 'db>,
    ) -> RunResult<DescriptorOrigin<'db>> {
        self.local(1, size_of::<DescriptorOrigin<'db>>() * 2, || {
            scope.guard().descriptor_origin()
        })
        .await
    }

    async fn retain_anchor(
        &self,
        _scope: &mut CallableVisitScope<'_, 'db>,
        _reference: DefinitionUse<'db>,
        _dispatches: DescriptorDispatches<'db>,
    ) -> RunResult<()> {
        self.unavailable(SourceOperation::CallableGuard(
            CallableGuardOperation::DescriptorDeclarations,
        ))
        .await
    }

    async fn insert_definition(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        reference: DefinitionUse<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> RunResult<()> {
        self.callable_storage(|control| {
            scope
                .prepare_definition_insert((reference, origin), control)?
                .try_commit()
                .map_err(|_| CallableGuardStorageError::StalePreparation)?;
            #[cfg(test)]
            guard_tests::committed(
                self.db(),
                self.access.endpoint(),
                scope.guard(),
                guard_tests::Stage::Definition,
                None,
            )
            .map_err(CallableGuardStorageError::Refused)?;
            Ok(())
        })
        .await
    }

    async fn recursive_identity(&self, ty: Type<'db>) -> RunResult<Option<TypeIdentity<'db>>> {
        recursive_identity_with(ty, self).await
    }

    async fn insert_identity(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, TypeIdentity<'db>),
    ) -> RunResult<bool> {
        self.callable_storage(|control| {
            scope
                .prepare_identity_insert(key, control)?
                .try_commit()
                .map_err(|_| CallableGuardStorageError::StalePreparation)
        })
        .await
    }

    async fn record_growth(&self, scope: &CallableVisitScope<'_, 'db>) -> RunResult<()> {
        self.local(2, 0, || scope.guard().record_growth_recovery())
            .await
    }

    async fn insert_exact(
        &self,
        scope: &mut CallableVisitScope<'_, 'db>,
        key: (CallableExpansion, Type<'db>),
    ) -> RunResult<bool> {
        self.callable_storage(|control| {
            let inserted = scope
                .prepare_exact_insert(key, control)?
                .try_commit()
                .map_err(|_| CallableGuardStorageError::StalePreparation)?;
            #[cfg(test)]
            guard_tests::committed(
                self.db(),
                self.access.endpoint(),
                scope.guard(),
                guard_tests::Stage::Exact,
                Some(key),
            )
            .map_err(CallableGuardStorageError::Refused)?;
            Ok(inserted)
        })
        .await
    }
}

pub(super) struct GuardedPreparationEffects<'effects, 'access, 'guard, 'run, 'db: 'run, A> {
    pub(super) source: &'effects SourceEffects<'access, 'run, 'db, A>,
    pub(super) guard: &'guard CallableRecursionGuard<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    GuardedPreparationEffects<'_, '_, '_, 'run, 'db, A>
{
    /// Expands a resolved descriptor callable under the dispatch state that selected it.
    /// The scope restores the caller's previous dispatch state after expansion and origin
    /// attachment, including refusals.
    pub(super) async fn bindings_from_descriptor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        origin: DescriptorOrigin<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .local(
                1,
                size_of::<Option<DescriptorDispatchScope<'_, 'db>>>(),
                || (),
            )
            .await?;
        let mut scope = None;
        self.source
            .local(2, size_of::<DescriptorDispatchScope<'_, 'db>>() * 2, || {
                scope = Some(self.guard.begin_dependency_scope());
            })
            .await?;
        let mut scope = scope.ok_or(RunError::Contract(
            "descriptor dependency scope was not constructed",
        ))?;
        self.source
            .callable_storage(|control| {
                self.guard
                    .prepare_dependency_replace(&mut scope, origin, control)?
                    .try_commit()
                    .map_err(|_| CallableGuardStorageError::StalePreparation)
            })
            .await?;
        let mut bindings = self
            .forward(db, env, ty, origin.return_contains_recursive_recovery)
            .await?;
        self.source
            .allocate_future(|| bindings.add_descriptor_origin_with(db, origin, self.source))
            .await?
            .await?;
        self.source
            .local(1, size_of::<Bindings<'db>>(), || ())
            .await?;
        Ok(bindings)
    }

    async fn guarded_bindings(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        let mut scope = None;
        self.source
            .local(1, size_of::<CallableVisitScope<'_, 'db>>() * 2, || {
                scope = Some(self.guard.begin_scope());
            })
            .await?;
        let mut scope = scope.ok_or(RunError::Contract(
            "callable guard scope was not constructed",
        ))?;
        let decision = boxed_future_with_fixed_transfers_at(
            self.source.access.endpoint(),
            Ok(EXACT_CALLABLE_ENTRY_QUOTE),
            || {
                callable_enter_exact_in_place_with(
                    (CallableExpansion::Bindings, ty),
                    &mut scope,
                    self.source,
                )
            },
        )
        .await?
        .await?;
        match decision {
            ExactCallableEntryDecision::ExactCycle => {
                self.source.callable_cycle_bindings(ty).await
            }
            ExactCallableEntryDecision::Entered => self.body(db, env, ty, unknown_is_recovery).await,
        }
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> BindingPreparationEffects<'db>
    for GuardedPreparationEffects<'_, '_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn guarded(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .allocate_future(|| self.guarded_bindings(db, env, ty, unknown_is_recovery))
            .await?
            .await
    }

    async fn body(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .allocate_future(|| {
                bindings_body_with(
                    db,
                    env,
                    ty,
                    unknown_is_recovery,
                    BindingPreparationFacts,
                    self,
                )
            })
            .await?
            .await
    }

    async fn forward(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .allocate_future(|| {
                bindings_with(
                    db,
                    env,
                    ty,
                    unknown_is_recovery,
                    BindingPreparationFacts,
                    self,
                )
            })
            .await?
            .await
    }

    async fn function(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        function: FunctionType<'db>,
    ) -> RunResult<Bindings<'db>> {
        BindingPreparationEffects::function(self.source, db, env, function).await
    }

    async fn known_class(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassLiteral<'db>,
    ) -> RunResult<Option<Bindings<'db>>> {
        BindingPreparationEffects::known_class(self.source, db, env, ty, class).await
    }

    async fn constructor(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        class: ClassType<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.source
            .allocate_future(|| constructor_bindings_with(db, env, ty, class, self.guard, self))
            .await?
            .await
    }

    async fn dependency(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        dependency: BindingPreparationDependency<'db>,
        unknown_is_recovery: bool,
    ) -> RunResult<Bindings<'db>> {
        match dependency {
            BindingPreparationDependency::Callable(callable) => {
                self.source.stored_callable_bindings(ty, callable).await
            }
            BindingPreparationDependency::BoundMethod(method) => {
                self.source
                    .allocate_future(|| {
                        bound_method_bindings_with(db, env, ty, method, unknown_is_recovery, self)
                    })
                    .await?
                    .await
            }
            BindingPreparationDependency::WrapperDescriptor(_) => {
                BindingPreparationEffects::dependency(
                    self.source,
                    db,
                    env,
                    ty,
                    dependency,
                    unknown_is_recovery,
                )
                .await
            }
            _ => self.source.unavailable(SourceOperation::CallBindings).await,
        }
    }
}
