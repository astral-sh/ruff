use std::rc::Rc;

use salsa::execution_probe::{ExecutionWork, RunError, RunResult, TaskEndpoint};

use super::{TypeVarDefaultVisitorHandle, cost};
use crate::analysis::TypeVarDefaultOperation;
use crate::types::constraints::control::{GrowthPlan, TddError};
use crate::types::cyclic::TypeIdentity;
use crate::types::generics::context_construction::ContextVariables;
use crate::types::infer::builder::source_definition::controlled::storage::{
    sequence_merge,
};
use crate::types::infer::builder::source_definition::controlled::{
    FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::local_transfer::generated_field_quote;
use crate::types::local_transfer::collections::smallvec_with_capacity_quote;
use crate::types::typevar::default::self_reference::{
    SelfReferenceEffects, SelfReferenceFacts, SelfReferenceState, alias_is_self_referential_with,
    recursive_is_self_referential_with, self_reference_predicate_with,
    type_is_self_referential_with, variable_is_self_referential_with,
};
use crate::types::typevar::{TypeVarIdentity, TypeVarInstance};
use crate::types::visitor::runtime::{RuntimeTypeSearchWith, RuntimeTypeWalk};
use crate::types::visitor::{SmallSetControl, TypeSearchMode, TypeWalkEffects, TypeWalkFacts, search_type_with};
use crate::types::{
    BoundTypeVarInstance, GenericContext, RecursiveType, Specialization, Type, TypeAliasType,
};
use crate::{Program, ProgramEnvironment};

/// Admits the shared set's key callbacks and backing payload before mutation.
/// `remember_variable` must first admit `cost::variable_set`, which pays native operations and
/// full-key Hash/Eq bodies.
struct VariableSetControl<'a, 'run, 'db> {
    endpoint: &'a TaskEndpoint<'run, 'db>,
}

impl<'db> SmallSetControl<TypeVarInstance<'db>> for VariableSetControl<'_, '_, 'db> {
    type Error = RunError;

    fn access(&mut self, _variable: TypeVarInstance<'db>) -> Result<(), TddError<RunError>> {
        cost::admit(self.endpoint, const { cost::variable_key_access() }).map_err(TddError::Refused)
    }

    fn grow(&mut self, plan: GrowthPlan) -> Result<(), TddError<RunError>> {
        cost::admit(self.endpoint, cost::variable_growth(plan)).map_err(TddError::Refused)
    }
}

struct ValidationState<'run, 'db> {
    shared: SelfReferenceState<'db>,
    visitor: TypeVarDefaultVisitorHandle<'run, 'db>,
    #[cfg(test)]
    db: &'db dyn crate::Db,
    #[cfg(test)]
    variable: TypeVarInstance<'db>,
}

#[cfg(test)]
impl Drop for ValidationState<'_, '_> {
    fn drop(&mut self) {
        crate::types::infer::source_runtime::tests::default_self_reference::validation_dropped(
            self.db,
            self.variable,
            self.visitor.visitor(),
        );
    }
}

// Queued descendants own these handles, while their pending default scopes borrow the same visitor.
struct SelfReferenceSource<'run, 'db, A> {
    access: A,
    program: Program<'db>,
    env: ProgramEnvironment<'db>,
    visitor: TypeVarDefaultVisitorHandle<'run, 'db>,
    #[cfg(test)]
    variable: TypeVarInstance<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SelfReferenceSource<'run, 'db, A> {
    fn source(&self) -> SourceEffects<'_, 'run, 'db, A> {
        SourceEffects::new(&self.access, self.program)
    }

    fn clone_with_visitor(&self, visitor: &TypeVarDefaultVisitorHandle<'run, 'db>) -> Self {
        Self {
            access: A::clone(&self.access),
            program: self.program,
            env: self.env.clone(),
            visitor: visitor.clone(),
            #[cfg(test)]
            variable: self.variable,
        }
    }

    /// Quotes cloning the retained source and visitor, including their nonfinal releases.
    fn clone_quote() -> RunResult<(usize, usize)> {
        cost::add(A::retained_clone_quote(), const { cost::add(
            cost::rc_clone::<crate::types::typevar::TypeVarDefaultVisitor<'db>>(),
            cost::events(32, &[
                size_of::<A>(), size_of::<ProgramEnvironment<'db>>(), size_of::<Program<'db>>(),
                size_of::<TypeVarDefaultVisitorHandle<'run, 'db>>(), size_of::<usize>(),
            ]),
        ) })
    }

    async fn unavailable<T>(&self, operation: TypeVarDefaultOperation) -> RunResult<T> {
        self.source()
            .unavailable(SourceOperation::TypeVarDefault(operation))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn typevar_default_is_self_referential(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
        visitor: &TypeVarDefaultVisitorHandle<'run, 'db>,
    ) -> RunResult<bool> {
        let quote = self.local_quoted_with_fixed_transfers(
            const { cost::preparation() }, SelfReferenceSource::<'run, 'db, A>::clone_quote,
        ).await?;
        let effects = self
            .local_quoted_with_fixed_transfers(quote, || SelfReferenceSource {
                access: A::clone(self.access),
                program: self.program,
                env: env.clone(),
                visitor: visitor.clone(),
                #[cfg(test)]
                variable,
            })
            .await?;
        self.type_parameter_future(|| type_is_self_referential_with(variable, default, &effects))
            .await?.await
    }
}

struct SelfReferencePredicate<'effects, 'run, 'db, A> {
    effects: &'effects SelfReferenceSource<'run, 'db, A>,
    state: &'effects Rc<ValidationState<'run, 'db>>,
    target: TypeVarIdentity<'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RuntimeTypeSearchWith<'run, 'db>
    for SelfReferencePredicate<'_, 'run, 'db, A>
{
    async fn predicate(
        &self,
        _endpoint: &TaskEndpoint<'run, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let source = self.effects.source();
        source.type_parameter_future(|| self_reference_predicate_with(ty, self.target, self.state, self.effects))
            .await?.await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SelfReferenceEffects<'db>
    for SelfReferenceSource<'run, 'db, A>
{
    type Error = RunError;
    type State = Rc<ValidationState<'run, 'db>>;

    async fn new_state(&self) -> RunResult<Self::State> {
        let source = self.source();
        let quote = source.local_quoted_with_fixed_transfers(const { cost::owner_preparation() }, || {
            cost::add(smallvec_with_capacity_quote::<TypeIdentity<'db>, 1>(1), const {
                cost::add(cost::add(cost::rc_owner::<ValidationState<'run, 'db>>(), cost::empty_variable_set()), cost::add(
                    cost::rc_clone::<crate::types::typevar::TypeVarDefaultVisitor<'db>>(),
                    cost::events(64, &[
                        size_of::<SelfReferenceState<'db>>(), size_of::<ValidationState<'run, 'db>>(),
                        size_of::<usize>(), size_of::<TypeVarDefaultVisitorHandle<'run, 'db>>(),
                    ]),
                ))
            })
        }).await?;
        let state = self
            .source()
            .local_quoted_with_fixed_transfers(quote, || {
                Rc::new(ValidationState {
                    shared: SelfReferenceState::new(),
                    visitor: self.visitor.clone(),
                    #[cfg(test)]
                    db: self.access.db(),
                    #[cfg(test)]
                    variable: self.variable,
                })
            })
            .await?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::default_self_reference::validation_created(
            self.access.db(),
            self.variable,
            state.visitor.visitor(),
        );
        Ok(state)
    }

    async fn identity(&self, variable: TypeVarInstance<'db>) -> RunResult<TypeVarIdentity<'db>> {
        let source = self.source();
        let endpoint = self.access.endpoint();
        let quote = generated_field_quote(
                |variable: TypeVarInstance<'db>, context| variable.field_requests(context),
                |variable: TypeVarInstance<'db>, context| variable.field_requests(context).identity(),
            );
        let read = source.boxed_future_with_fixed_transfers(quote, || {
            endpoint.read_field(variable.field_requests(endpoint.field_request_context()).identity(), &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }

    async fn bound_typevar(
        &self,
        variable: BoundTypeVarInstance<'db>,
    ) -> RunResult<TypeVarInstance<'db>> {
        let source = self.source();
        let endpoint = self.access.endpoint();
        let quote = generated_field_quote(
                |variable: BoundTypeVarInstance<'db>, context| variable.field_requests(context),
                |variable: BoundTypeVarInstance<'db>, context| variable.field_requests(context).typevar(),
            );
        let read = source.boxed_future_with_fixed_transfers(quote, || {
            endpoint.read_field(variable.field_requests(endpoint.field_request_context()).typevar(), &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }

    /// Records a complete variable instance, returning true when it was not already remembered.
    async fn remember_variable(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<bool> {
        let source = self.source();
        let endpoint = self.access.endpoint();
        source
            .local_quoted_with_fixed_transfers(const { cost::add(cost::variable_metadata(), cost::variable_borrow()) }, || {
                let mut seen = state.shared.seen_typevars.borrow_mut();
                let layout = seen.layout();
                let preparation = if layout.len < layout.capacity {
                    const { cost::variable_set_preparation() }
                } else {
                    const { cost::add(cost::variable_set_preparation(), cost::variable_growth_preparation()) }
                };
                cost::admit(endpoint, preparation)?;
                cost::admit(endpoint, cost::variable_set(layout))?;
                seen.insert_with(variable, &mut VariableSetControl { endpoint }).map_err(|error| match error {
                    TddError::Refused(error) => error,
                    TddError::CapacityExhausted => RunError::Contract("self-reference small-set capacity overflow"),
                })
            })
            .await?
    }

    async fn checked_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let source = self.source();
        let quote = source.local_quoted_with_fixed_transfers(const { cost::preparation() }, Self::clone_quote).await?;
        let child = source
            .local_quoted_with_fixed_transfers(quote, || self.clone_with_visitor(&self.visitor))
            .await?;
        let endpoint = self.access.endpoint();
        let child = source.type_parameter_future(move || async move {
                #[cfg(test)]
                crate::types::infer::source_runtime::tests::default_self_reference::observe_default_child(
                    child.access.db(), variable, child.visitor.visitor(),
                );
                let source = child.source();
                source.boxed_future_with_fixed_transfers(
                    Ok((1, size_of::<Option<&TypeVarDefaultVisitorHandle<'run, 'db>>>())),
                    || source.typevar_default_with_handle(variable, &child.env, Some(&child.visitor)),
                ).await?.await
        }).await?;
        Ok(endpoint.child_call(|| async {
            let demand = endpoint.demand(move || child)?;
            #[cfg(test)]
            crate::types::infer::source_runtime::tests::default_self_reference::observe_default_queued(
                self.access.db(), variable, self.visitor.visitor(),
            );
            demand.await
        }).await)
    }

    async fn search(
        &self,
        state: &Self::State,
        ty: Type<'db>,
        target: TypeVarIdentity<'db>,
    ) -> RunResult<bool> {
        let source = self.source();
        let quote = source.local_quoted_with_fixed_transfers(const { cost::preparation() }, || {
            cost::add(Self::clone_quote(), const { cost::rc_clone::<ValidationState<'run, 'db>>() })
        }).await?;
        let (child, state) = source
            .local_quoted_with_fixed_transfers(quote, || {
                (self.clone_with_visitor(&state.visitor), Rc::clone(state))
            })
            .await?;
        let endpoint = self.access.endpoint();
        let child = source.type_parameter_future(move || async move {
                #[cfg(test)]
                crate::types::infer::source_runtime::tests::default_self_reference::observe_search_child(
                    child.access.db(), ty, child.visitor.visitor(),
                );
                let source = child.source();
                let mut walk = source.local_with_fixed_transfers(12, 0, || RuntimeTypeWalk {
                    db: child.access.db(),
                    endpoint: child.access.endpoint(),
                    query: SelfReferencePredicate { effects: &child, state: &state, target },
                    unavailable: &source,
                }).await?;
                source.boxed_future_with_fixed_transfers(Ok((0, 0)), || search_type_with(ty, TypeSearchMode::SkipLazyAttributes, TypeWalkFacts, &mut walk)).await?.await
        }).await?;
        Ok(endpoint.child_call(|| async {
            let demand = endpoint.demand(move || child)?;
            #[cfg(test)]
            crate::types::infer::source_runtime::tests::default_self_reference::observe_search_queued(
                self.access.db(), ty, self.visitor.visitor(),
            );
            demand.await
        }).await)
    }

    async fn variable_reference(
        &self,
        state: &Self::State,
        variable: TypeVarInstance<'db>,
        target: TypeVarIdentity<'db>,
    ) -> RunResult<bool> {
        let source = self.source();
        source.type_parameter_future(|| variable_is_self_referential_with(variable, target, state, SelfReferenceFacts, self)).await?.await
    }

    async fn alias_reference(
        &self,
        state: &Self::State,
        alias: TypeAliasType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> RunResult<bool> {
        let source = self.source();
        source.type_parameter_future(|| alias_is_self_referential_with(alias, target, state, self)).await?.await
    }

    async fn recursive_reference(
        &self,
        state: &Self::State,
        recursive: RecursiveType<'db>,
        target: TypeVarIdentity<'db>,
    ) -> RunResult<bool> {
        let source = self.source();
        source.type_parameter_future(|| recursive_is_self_referential_with(recursive, target, state, self)).await?.await
    }

    async fn alias_specialization(
        &self,
        alias: TypeAliasType<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        let source = self.source();
        let mut walk = source.local_with_fixed_transfers(8, 0, || RuntimeTypeWalk {
            db: self.access.db(),
            endpoint: self.access.endpoint(),
            query: (),
            unavailable: &source,
        }).await?;
        source.type_parameter_future(|| walk.alias_arguments(alias)).await?.await
    }

    async fn specialization_types(
        &self,
        specialization: Specialization<'db>,
    ) -> RunResult<&'db [Type<'db>]> {
        let source = self.source();
        let mut walk = source.local_with_fixed_transfers(8, 0, || RuntimeTypeWalk {
            db: self.access.db(),
            endpoint: self.access.endpoint(),
            query: (),
            unavailable: &source,
        }).await?;
        source.type_parameter_future(|| walk.specialization_types(specialization)).await?.await
    }

    async fn next_type(
        &self,
        types: &[Type<'db>],
        index: &mut usize,
    ) -> RunResult<Option<Type<'db>>> {
        self.source()
            .local_quoted_with_fixed_transfers(const { cost::type_cursor() }, || {
                let result = types.get(*index).copied();
                if result.is_some() {
                    *index = index
                        .checked_add(1)
                        .ok_or(RunError::Contract("self-reference type cursor overflow"))?;
                }
                Ok(result)
            })
            .await?
    }

    async fn alias_generic_context(
        &self,
        _alias: TypeAliasType<'db>,
    ) -> RunResult<Option<GenericContext<'db>>> {
        self.unavailable(TypeVarDefaultOperation::AliasGenericContext)
            .await
    }

    async fn generic_variables(
        &self,
        context: GenericContext<'db>,
    ) -> RunResult<&'db ContextVariables<'db>> {
        self.source()
            .field(context.variables_request(self.access.endpoint().field_request_context()))
            .await
    }

    async fn next_variable(
        &self,
        variables: &ContextVariables<'db>,
        index: &mut usize,
    ) -> RunResult<Option<BoundTypeVarInstance<'db>>> {
        self.source()
            .local(size_of::<BoundTypeVarInstance<'db>>() * 2 + 4, 0, || {
                let result = GenericContext::variable_at_in(variables, *index);
                if result.is_some() {
                    *index = index.checked_add(1).ok_or(RunError::Contract(
                        "self-reference variable cursor overflow",
                    ))?;
                }
                Ok(result)
            })
            .await?
    }

    async fn alias_identity(&self, _alias: TypeAliasType<'db>) -> RunResult<TypeIdentity<'db>> {
        self.unavailable(TypeVarDefaultOperation::AliasIdentity)
            .await
    }

    async fn recursive_identity(
        &self,
        _recursive: RecursiveType<'db>,
    ) -> RunResult<TypeIdentity<'db>> {
        self.unavailable(TypeVarDefaultOperation::RecursiveIdentity)
            .await
    }

    async fn remember_type(
        &self,
        state: &Self::State,
        identity: TypeIdentity<'db>,
    ) -> RunResult<bool> {
        let source = self.source();
        let endpoint = self.access.endpoint();
        source
            .local(1, 0, || {
                let mut seen = state.shared.seen_types.borrow_mut();
                let scan = seen
                    .len()
                    .checked_mul(size_of::<TypeIdentity<'db>>() + 1)
                    .and_then(|work| work.checked_add(1))
                    .ok_or(RunError::Contract(
                        "self-reference identity quotation overflow",
                    ))?;
                endpoint.admit_work(scan)?;
                endpoint.check_completion()?;
                let payload = |identity: TypeIdentity<'db>| match identity {
                    TypeIdentity::Other(ty) => ty.inline_payload_bytes(),
                    _ => 0,
                };
                let stored_payload = seen
                    .iter()
                    .try_fold(0usize, |bytes, identity| {
                        bytes.checked_add(payload(*identity))
                    })
                    .ok_or(RunError::Contract(
                        "self-reference stored identity payload overflow",
                    ))?;
                let equality = payload(identity)
                    .checked_mul(seen.len())
                    .and_then(|work| work.checked_add(stored_payload))
                    .and_then(|work| {
                        work.checked_add(
                            seen.len()
                                .checked_mul(size_of::<TypeIdentity<'db>>() * 2 + 1)?,
                        )
                    })
                    .and_then(|work| work.checked_add(1))
                    .ok_or(RunError::Contract(
                        "self-reference identity equality overflow",
                    ))?;
                endpoint.admit_work(equality)?;
                endpoint.check_completion()?;
                if seen.contains(&identity) {
                    return Ok(false);
                }
                let quote = sequence_merge::<TypeIdentity<'db>>(seen.len(), seen.capacity(), 1)
                    .ok_or(RunError::Contract(
                        "self-reference identity growth overflow",
                    ))?;
                let work = quote
                    .work
                    .checked_mul(size_of::<TypeIdentity<'db>>() * 2 + 1)
                    .and_then(|work| work.checked_add(quote.bytes))
                    .ok_or(RunError::Contract(
                        "self-reference identity disposal overflow",
                    ))?;
                endpoint.admit_work(work)?;
                if quote.bytes != 0 {
                    endpoint.admit(ExecutionWork::Resource {
                        requested_bytes: quote.bytes,
                    })?;
                }
                endpoint.check_completion()?;
                seen.reserve(1);
                seen.push(identity);
                Ok(true)
            })
            .await?
    }

    async fn alias_value(&self, _alias: TypeAliasType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TypeVarDefaultOperation::AliasValue).await
    }

    async fn alias_raw_value(&self, _alias: TypeAliasType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TypeVarDefaultOperation::AliasRawValue)
            .await
    }

    async fn recursive_arguments(
        &self,
        recursive: RecursiveType<'db>,
    ) -> RunResult<Option<Specialization<'db>>> {
        let source = self.source();
        let mut walk = source.local_with_fixed_transfers(8, 0, || RuntimeTypeWalk {
            db: self.access.db(),
            endpoint: self.access.endpoint(),
            query: (),
            unavailable: &source,
        }).await?;
        source.type_parameter_future(|| walk.recursive_arguments(recursive)).await?.await
    }

    async fn recursive_unfold(&self, _recursive: RecursiveType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(TypeVarDefaultOperation::RecursiveUnfold)
            .await
    }
}
