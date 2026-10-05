use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::place::PlaceAndQualifiers;
use crate::types::call::dunder::{
    DunderCallRequest, DunderCallResult, DunderEffects, DunderLookup, DunderRead, DunderWork,
    sealed,
};
use crate::types::call::{Bindings, CallArguments, CallDunderError, CallError};
use crate::types::member_lookup::finalization::{GetattrCallKind, GetattrCallResult};
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::{IntersectionType, MemberLookupPolicy, Type, TypeContext, UnionBuilder};
use crate::{Db, ProgramEnvironment};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn call_member_fallback_dunder(
        &self,
        ty: Type<'db>,
        name: Type<'db>,
        kind: GetattrCallKind,
    ) -> RunResult<GetattrCallResult<'db>> {
        let (method, policy) = self
            .local(3, 0, || match kind {
                GetattrCallKind::GetAttr => ("__getattr__", MemberLookupPolicy::default()),
                GetattrCallKind::GetAttribute => (
                    "__getattribute__",
                    MemberLookupPolicy::MRO_NO_OBJECT_FALLBACK
                        | MemberLookupPolicy::META_CLASS_NO_TYPE_FALLBACK,
                ),
            })
            .await?;
        let bytes = Self::checked(CallArguments::capacity_bytes(1))?;
        let arguments = self
            .local(8, bytes, || CallArguments::positional([name]))
            .await?;
        let env = self
            .local(size_of::<ProgramEnvironment<'db>>() * 2 + 1, 0, || {
                ProgramEnvironment::from_program(self.program)
            })
            .await?;
        let effects = MemberFallbackDunderEffects { source: self };
        let request = DunderCallRequest::implicit(ty, method, TypeContext::default(), policy);
        let result = self
            .allocate_future(|| request.evaluate_with(self.db(), &env, &arguments, &effects))
            .await?
            .await?;
        self.work(1).await?;
        match result {
            Err(CallDunderError::MethodNotAvailable) => Ok(GetattrCallResult::Missing),
            Err(CallDunderError::PossiblyUnbound { .. }) => Ok(GetattrCallResult::PossiblyUnbound),
            Ok(_) | Err(CallDunderError::CallError(..)) => {
                effects
                    .unavailable(GeneralMemberOperation::GetattrFallback)
                    .await
            }
        }
    }
}

struct MemberFallbackDunderEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>>
    MemberFallbackDunderEffects<'_, '_, 'run, 'db, A>
{
    async fn unavailable<T>(&self, operation: GeneralMemberOperation) -> RunResult<T> {
        self.source
            .unavailable(SourceOperation::MemberLookup(operation))
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> sealed::Sealed
    for MemberFallbackDunderEffects<'_, '_, 'run, 'db, A>
{
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> DunderEffects<'db>
    for MemberFallbackDunderEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn admit(&self, work: DunderWork) -> RunResult<()> {
        let operation = match work {
            DunderWork::IntersectionStorage { .. } => GeneralMemberOperation::Intersection,
            DunderWork::MissingUnionElement => GeneralMemberOperation::Union,
            DunderWork::PossiblyUnboundStorage => GeneralMemberOperation::GetattrFallback,
        };
        self.unavailable(operation).await
    }

    async fn read<T>(&self, read: DunderRead, _operation: impl FnOnce() -> T) -> RunResult<T> {
        let operation = match read {
            DunderRead::IntersectionElements => GeneralMemberOperation::Intersection,
            DunderRead::UnionElements => GeneralMemberOperation::Union,
        };
        self.unavailable(operation).await
    }

    async fn finite_alternatives(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _intersection: IntersectionType<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(GeneralMemberOperation::Intersection).await
    }

    async fn lookup(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        request: DunderCallRequest<'_, 'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let work = SourceEffects::<A>::checked(
            request
                .name
                .len()
                .checked_mul(2)
                .and_then(|len| len.checked_add(2)),
        )?;
        let bytes =
            SourceEffects::<A>::checked(request.name.len().checked_add(3 * size_of::<usize>()))?;
        let (name, policy) = self
            .source
            .local(work, bytes, || {
                let policy = match request.lookup {
                    DunderLookup::Implicit(policy) => {
                        policy | MemberLookupPolicy::NO_INSTANCE_FALLBACK
                    }
                    DunderLookup::OnClass => MemberLookupPolicy::default(),
                };
                (Name::new(request.name), policy)
            })
            .await?;
        let result = self
            .source
            .access
            .member_lookup(request.receiver, &name, policy)
            .await?;
        let parts = self.source.member_lookup_parts(result).await?;
        Ok(parts.member)
    }

    async fn bindings(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _callable: Type<'db>,
    ) -> RunResult<Bindings<'db>> {
        self.unavailable(GeneralMemberOperation::GetattrFallback)
            .await
    }

    async fn match_parameters(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: Bindings<'db>,
        _arguments: &CallArguments<'_, 'db>,
    ) -> RunResult<Bindings<'db>> {
        self.unavailable(GeneralMemberOperation::GetattrFallback)
            .await
    }

    async fn check_types(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _bindings: Bindings<'db>,
        _arguments: &CallArguments<'_, 'db>,
        _tcx: TypeContext<'db>,
    ) -> RunResult<Result<Bindings<'db>, CallError<'db>>> {
        self.unavailable(GeneralMemberOperation::GetattrFallback)
            .await
    }

    async fn call(
        &self,
        _db: &'db dyn Db,
        _env: &ProgramEnvironment<'db>,
        _request: DunderCallRequest<'_, 'db>,
        _arguments: &CallArguments<'_, 'db>,
    ) -> RunResult<DunderCallResult<'db>> {
        self.unavailable(GeneralMemberOperation::GetattrFallback)
            .await
    }

    async fn union_add(
        &self,
        _builder: UnionBuilder<'db>,
        _ty: Type<'db>,
    ) -> RunResult<UnionBuilder<'db>> {
        self.unavailable(GeneralMemberOperation::Union).await
    }

    async fn union_build(&self, _builder: UnionBuilder<'db>) -> RunResult<Type<'db>> {
        self.unavailable(GeneralMemberOperation::Union).await
    }

    async fn merge_intersection(
        &self,
        _receiver: Type<'db>,
        _bindings: Vec<Bindings<'db>>,
    ) -> RunResult<Bindings<'db>> {
        self.unavailable(GeneralMemberOperation::Intersection).await
    }
}
