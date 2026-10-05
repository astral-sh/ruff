use std::marker::PhantomData;
use std::rc::Rc;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::{FixedFieldCopy, SourceAccess, SourceEffects};
use crate::types::Type;
use crate::types::cyclic::{
    CycleDetectorLookup, CycleDetectorVisit, CycleGuardControl, HasIdentity, RelationGuardError,
    RelationGuardWork,
};
use crate::types::local_transfer::generated_field_quote;
use crate::types::typevar::default::evaluation::{
    LazyTypeVarDefaultEffects, TypeVarDefaultEffects, lazy_typevar_default_with,
    typevar_default_with,
};
use crate::types::typevar::{TypeVarDefaultEvaluation, TypeVarDefaultVisitor, TypeVarInstance};
use crate::{Db, ProgramEnvironment};

mod cost;
mod self_reference;

#[derive(Clone)]
enum TypeVarDefaultVisitorHandle<'run, 'db> {
    Owned(Rc<TypeVarDefaultVisitor<'db>>, PhantomData<&'run ()>),
    #[cfg(test)]
    Borrowed(&'run TypeVarDefaultVisitor<'db>),
}

impl<'db> TypeVarDefaultVisitorHandle<'_, 'db> {
    fn visitor(&self) -> &TypeVarDefaultVisitor<'db> {
        match self {
            Self::Owned(visitor, _) => visitor,
            #[cfg(test)]
            Self::Borrowed(visitor) => visitor,
        }
    }
}

struct TypeVarDefaultGuard<'endpoint, 'run, 'db: 'run> {
    endpoint: &'endpoint TaskEndpoint<'run, 'db>,
    storage: cost::GuardStorage,
    result_payload_bytes: usize,
}

impl<'db> CycleGuardControl<'db, TypeVarInstance<'db>> for TypeVarDefaultGuard<'_, '_, 'db> {
    type Error = RunError;

    fn admit(&mut self, work: RelationGuardWork) -> RunResult<()> {
        let preparation = match work {
            RelationGuardWork::Relocate { .. } => match self.storage {
                cost::GuardStorage::Active => cost::ACTIVE_RELOCATION_PREPARATION,
                cost::GuardStorage::Cache { .. } => cost::CACHE_RELOCATION_PREPARATION,
            },
            RelationGuardWork::CacheKeyScan { capacity: Some(_) } => cost::SPILLED_CACHE_SCAN_PREPARATION,
            RelationGuardWork::CacheKeyScan { capacity: None } => cost::INLINE_CACHE_SCAN_PREPARATION,
            RelationGuardWork::KeyCheck | RelationGuardWork::Candidate
            | RelationGuardWork::Identity | RelationGuardWork::ActivePush => const { cost::fixed_guard_preparation() },
            RelationGuardWork::CacheAccess { .. } => cost::CACHE_ACCESS_PREPARATION,
            RelationGuardWork::ExactScan { .. } | RelationGuardWork::CandidateScan { .. } => cost::SCAN_PREPARATION,
            RelationGuardWork::Resource { .. } => cost::RESOURCE_PREPARATION,
            RelationGuardWork::Finish => cost::FINISH_PREPARATION,
        };
        cost::admit(self.endpoint, preparation)?;
        if let RelationGuardWork::CacheKeyScan { capacity } = work {
            self.storage = cost::GuardStorage::Cache { capacity };
        }
        cost::admit(self.endpoint, cost::guard(self.storage, work, self.result_payload_bytes))
    }

    fn key_has_fixed_cost(_key: &TypeVarInstance<'db>) -> bool {
        true
    }

    fn candidate(
        &mut self,
        db: &'db dyn Db,
        item: &TypeVarInstance<'db>,
        active: &TypeVarInstance<'db>,
    ) -> RunResult<bool> {
        Ok(item.may_share_identity(db, active))
    }

    fn identity(
        &mut self,
        db: &'db dyn Db,
        item: &TypeVarInstance<'db>,
    ) -> RunResult<TypeVarInstance<'db>> {
        Ok(item.to_identity(db))
    }
}

fn guard_result<T>(result: Result<T, RelationGuardError<RunError>>) -> RunResult<T> {
    match result {
        Ok(result) => Ok(result),
        Err(RelationGuardError::Refused(error)) => Err(error),
        Err(RelationGuardError::CapacityExhausted) => Err(RunError::Contract(
            "TypeVar default visitor capacity overflow",
        )),
        Err(RelationGuardError::Changed) => Err(RunError::Contract(
            "TypeVar default visitor changed during admission",
        )),
        Err(RelationGuardError::UnsupportedKey) => Err(RunError::Contract(
            "TypeVar default visitor requires fixed-cost instance keys",
        )),
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn typevar_default(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.type_parameter_future(|| self.typevar_default_with_handle(variable, env, None))
            .await?.await
    }

    /// Evaluates an already-read default, creating a visitor only if it is lazy.
    pub(super) async fn typevar_default_from_stored(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        stored: TypeVarDefaultEvaluation<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let effects = self.local_with_fixed_transfers(3, 0, || TypeVarDefaultSourceEffects {
            source: self,
            visitor: None,
        }).await?;
        self.boxed_future_with_fixed_transfers(
            Ok((1, size_of::<Option<TypeVarDefaultEvaluation<'db>>>())),
            || typevar_default_with(variable, env, Some(stored), &effects),
        ).await?.await
    }

    #[cfg(test)]
    pub(in crate::types::infer) async fn typevar_default_with_visitor(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        visitor: &'run TypeVarDefaultVisitor<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let visitor = self
            .local_with_fixed_transfers(
                4,
                0,
                || TypeVarDefaultVisitorHandle::Borrowed(visitor),
            )
            .await?;
        self.boxed_future_with_fixed_transfers(
            Ok((1, size_of::<Option<&TypeVarDefaultVisitorHandle<'run, 'db>>>())),
            || self.typevar_default_with_handle(variable, env, Some(&visitor)),
        ).await?.await
    }

    /// Returns the checked default, entering a visitor only when the stored default is lazy.
    async fn typevar_default_with_handle(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        visitor: Option<&TypeVarDefaultVisitorHandle<'run, 'db>>,
    ) -> RunResult<Option<Type<'db>>> {
        let effects = self.local_with_fixed_transfers(3, 0, || TypeVarDefaultSourceEffects {
            source: self,
            visitor,
        }).await?;
        let stored = effects.stored_default(variable).await?;
        self.type_parameter_future(|| typevar_default_with(variable, env, stored, &effects))
            .await?.await
    }

    /// Rejects self-referential lazy defaults and caches completed results in the visitor.
    async fn lazy_typevar_default_with_handle(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        visitor: &TypeVarDefaultVisitorHandle<'run, 'db>,
    ) -> RunResult<Option<Type<'db>>> {
        let endpoint = self.access.endpoint();
        let lookup = self
            .local_with_fixed_transfers(32, TypeVarDefaultVisitor::admitted_transient_bytes()
                + 2 * size_of::<TypeVarDefaultGuard<'_, 'run, 'db>>(), || {
                visitor.visitor().lookup_visit_admitted(
                    self.db(),
                    variable,
                    &mut TypeVarDefaultGuard {
                        endpoint,
                        storage: cost::GuardStorage::Active,
                        result_payload_bytes: 0,
                    },
                )
            })
            .await?;
        let mut scope = match guard_result(lookup)? {
            CycleDetectorLookup::Cached(cached) => return Ok(cached.into_result()),
            CycleDetectorLookup::Visit(CycleDetectorVisit::Ready(result)) => return Ok(result),
            CycleDetectorLookup::Visit(CycleDetectorVisit::Cycle(_)) => return Ok(None),
            CycleDetectorLookup::Visit(CycleDetectorVisit::Pending(scope)) => scope,
        };
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::lazy_defaults::observe_pending(
            self.db(),
            variable,
        );
        let effects = self.local_with_fixed_transfers(3, 0, || LazyTypeVarDefaultSourceEffects {
            source: self,
            visitor,
        }).await?;
        let result = self.type_parameter_future(|| lazy_typevar_default_with(variable, env, &effects))
            .await?.await?;
        let result_payload_bytes = self.local_with_fixed_transfers(16, 0, || {
            result.map(Type::inline_payload_bytes).unwrap_or(0)
        }).await?;
        let prepared = self
            .local_quoted_with_fixed_transfers(const {
                cost::add(Ok((32, TypeVarDefaultVisitor::admitted_transient_bytes()
                    + 2 * size_of::<TypeVarDefaultGuard<'_, 'run, 'db>>())), cost::finish_commit())
            }, || {
                scope.prepare_finish_admitted(
                    &result,
                    &mut TypeVarDefaultGuard {
                        endpoint,
                        storage: cost::GuardStorage::Cache { capacity: None },
                        result_payload_bytes,
                    },
                )
            })
            .await?;
        let prepared = guard_result(prepared)?;
        #[cfg(test)]
        crate::types::infer::source_runtime::tests::lazy_defaults::observe_prepared(
            self.db(),
            variable,
        );
        self.local_with_fixed_transfers(8, 0, || {
            scope.commit_prepared_admitted(prepared, result, |left, right| left == right)
        })
        .await?
        .map_err(|_| RunError::Contract("TypeVar default visitor changed before completion"))
    }
}

/// Dispatches defaults while carrying an existing visitor through recursive requests.
struct TypeVarDefaultSourceEffects<'effects, 'access, 'run, 'db: 'run, 'visitor, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    visitor: Option<&'visitor TypeVarDefaultVisitorHandle<'run, 'db>>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    TypeVarDefaultSourceEffects<'_, '_, 'run, 'db, '_, A>
{
    async fn stored_default(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<TypeVarDefaultEvaluation<'db>>> {
        let endpoint = self.source.access.endpoint();
        let quote = generated_field_quote(
                |variable: TypeVarInstance<'db>, context| variable.field_requests(context),
                |variable: TypeVarInstance<'db>, context| variable.default_request(context),
            );
        let read = self.source.boxed_future_with_fixed_transfers(quote, || {
            endpoint.read_field(variable.default_request(endpoint.field_request_context()), &FixedFieldCopy)
        }).await?;
        Ok(read.await)
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVarDefaultEffects<'db>
    for TypeVarDefaultSourceEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.local_with_fixed_transfers(16, 0, || ()).await
    }

    async fn checked_lazy_default(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        // Fund the field read, optional-reference copy, branch and extraction before
        // selecting an existing visitor or constructing its owner.
        let visitor = self.source.local_with_fixed_transfers(4, 0, || self.visitor).await?;
        if let Some(visitor) = visitor {
            return self.source.type_parameter_future(|| {
                self.source.lazy_typevar_default_with_handle(variable, env, visitor)
            }).await?.await;
        }

        let quote = self.source.local_quoted_with_fixed_transfers(
            const { cost::owner_preparation() }, cost::visitor,
        ).await?;
        let visitor = self.source
            .local_quoted_with_fixed_transfers(quote, || {
                TypeVarDefaultVisitorHandle::Owned(
                    Rc::new(TypeVarDefaultVisitor::new(None)),
                    PhantomData,
                )
            })
            .await?;
        self.source.type_parameter_future(|| {
            self.source.lazy_typevar_default_with_handle(variable, env, &visitor)
        }).await?.await
    }
}

/// Supplies controlled lazy evaluation and self-reference validation, borrowing the visitor
/// handle kept alive while the default and its descendants run.
struct LazyTypeVarDefaultSourceEffects<'effects, 'access, 'run, 'db: 'run, 'visitor, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    visitor: &'visitor TypeVarDefaultVisitorHandle<'run, 'db>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> LazyTypeVarDefaultEffects<'db>
    for LazyTypeVarDefaultSourceEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn checkpoint(&self) -> RunResult<()> {
        self.source.local_with_fixed_transfers(16, 0, || ()).await
    }

    async fn lazy_default_unchecked(
        &self,
        variable: TypeVarInstance<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.source.type_parameter_future(|| self.source.access.lazy_typevar_default(variable))
            .await?.await
    }

    async fn type_is_self_referential(
        &self,
        variable: TypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
        default: Type<'db>,
    ) -> RunResult<bool> {
        self.source.type_parameter_future(|| {
            self.source.typevar_default_is_self_referential(variable, env, default, self.visitor)
        }).await?.await
    }
}
