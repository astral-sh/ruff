//! Computes callable argument deferral using the caller's `CallableRecursionGuard` for
//! callable conversion and the existing source execution endpoint.

use std::slice;

use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::types::call::bind::typevartuple::{
    CallableInspection, TypeVarTupleCallableEffects, TypeVarTupleCallableFacts,
    inspect_callables_with, inspect_signature_with, should_defer_typevartuple_callable_with,
};
use crate::types::callable::UpcastPolicy;
use crate::types::cyclic::CallableRecursionGuard;
use crate::types::signatures::{CallableSignature, Signature};
use crate::types::visitor::runtime::{RuntimeTypeSearchWith, RuntimeTypeWalk};
use crate::types::visitor::{TypeSearchMode, TypeWalkFacts, search_type_with};
use crate::types::{CallableType, CallableTypes, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn defer_typevartuple_callable(
        &self,
        env: &ProgramEnvironment<'db>,
        declared: Type<'db>,
        expected: Type<'db>,
        argument: Type<'db>,
        guard: Option<&CallableRecursionGuard<'db>>,
    ) -> RunResult<bool> {
        self.environment_program(env).await?;
        self.allocate_future(|| async {
            let effects = SourceTypeVarTupleCallableEffects {
                source: self,
                env,
                guard,
            };
            should_defer_typevartuple_callable_with(declared, expected, argument, &effects).await
        })
        .await?
        .await
    }
}

struct SourceTypeVarTupleCallableEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
    env: &'effects ProgramEnvironment<'db>,
    guard: Option<&'effects CallableRecursionGuard<'db>>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> TypeVarTupleCallableEffects<'db>
    for SourceTypeVarTupleCallableEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn upcast(&self, ty: Type<'db>) -> RunResult<Option<CallableTypes<'db>>> {
        self.source
            .allocate_future(|| {
                self.source
                    .callables_with_policy(self.env, ty, UpcastPolicy::default(), self.guard)
            })
            .await?
            .await
    }

    async fn signatures(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<&'db CallableSignature<'db>> {
        self.source
            .field(
                callable
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .signatures(),
            )
            .await
    }

    async fn cursor<'a, T>(&self, values: &'a [T]) -> RunResult<slice::Iter<'a, T>> {
        self.source
            .local(1, size_of::<slice::Iter<'a, T>>(), || values.iter())
            .await
    }

    async fn next<'a, T>(&self, cursor: &mut slice::Iter<'a, T>) -> RunResult<Option<&'a T>> {
        self.source.local(1, 0, || cursor.next()).await
    }

    async fn contains_typevartuple(&self, ty: Type<'db>) -> RunResult<bool> {
        self.source
            .allocate_future(|| async {
                let mut walk = RuntimeTypeWalk {
                    db: self.source.db(),
                    endpoint: self.source.access.endpoint(),
                    query: TypeVarTuplePredicate {
                        source: self.source,
                    },
                    unavailable: self.source,
                };
                search_type_with(
                    ty,
                    TypeSearchMode::SkipLazyAttributes,
                    TypeWalkFacts,
                    &mut walk,
                )
                .await
            })
            .await?
            .await
    }

    async fn inspect(
        &self,
        callables: &CallableTypes<'db>,
        inspection: CallableInspection,
    ) -> RunResult<bool> {
        self.source
            .allocate_future(|| {
                inspect_callables_with(callables, inspection, TypeVarTupleCallableFacts, self)
            })
            .await?
            .await
    }

    async fn inspect_signature(
        &self,
        signature: &Signature<'db>,
        inspection: CallableInspection,
    ) -> RunResult<bool> {
        self.source
            .allocate_future(|| {
                inspect_signature_with(signature, inspection, TypeVarTupleCallableFacts, self)
            })
            .await?
            .await
    }
}

struct TypeVarTuplePredicate<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RuntimeTypeSearchWith<'run, 'db>
    for TypeVarTuplePredicate<'_, '_, 'run, 'db, A>
{
    async fn predicate(
        &self,
        _endpoint: &TaskEndpoint<'run, 'db>,
        ty: Type<'db>,
    ) -> RunResult<bool> {
        let variable = self.source.local(1, 0, || ty.as_typevar()).await?;
        let Some(variable) = variable else {
            return Ok(false);
        };
        let fields = self.source.access.endpoint().field_request_context();
        let identity = self.source.field(variable.identity_request(fields)).await?;
        let kind = self
            .source
            .field(identity.identity.field_requests(fields).kind())
            .await?;
        self.source.local(1, 0, || kind.is_typevartuple()).await
    }
}
