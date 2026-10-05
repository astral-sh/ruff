use salsa::execution_probe::{RunError, RunResult, TaskEndpoint};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::types::callable::CallableTypeKind;
use crate::types::instance::{NominalClassFacts, nominal_is_definition_generic_with};
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::member_lookup::self_binding::{
    MemberSelfBindingEffects, contains_self_with, supports_self_binding_with,
};
use crate::types::visitor::runtime::{RuntimeTypeSearchWith, RuntimeTypeWalk};
use crate::types::visitor::{TypeSearchMode, TypeWalkFacts, search_type_with};
use crate::types::{CallableType, NominalInstanceType, Type, TypeVarKind};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Searches for `Self` through [`contains_self_with`], including alias arguments and
    /// excluding nominal instances whose class definition is not generic.
    pub(super) async fn contains_self_source(&self, ty: Type<'db>) -> RunResult<bool> {
        self.allocate_future(|| async {
            let mut effects = self
                .local(1, size_of::<SourceMemberSelfBindingEffects<'_, '_, 'run, 'db, A>>(), || SourceMemberSelfBindingEffects { source: self })
                .await?;
            contains_self_with(ty, &mut effects).await
        })
        .await?
        .await
    }

    /// Return `ty` unchanged when eager `Self` binding is unnecessary; otherwise report the
    /// operation as unavailable.
    pub(super) async fn bind_member_self_type(
        &self,
        ty: Type<'db>,
        _receiver: Type<'db>,
    ) -> RunResult<Type<'db>> {
        let supports_binding = self
            .allocate_future(|| async {
                let mut effects = SourceMemberSelfBindingEffects { source: self };
                supports_self_binding_with(ty, &mut effects).await
            })
            .await?
            .await?;
        if !supports_binding {
            return Ok(ty);
        }
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::MemberSelfBinding,
        ))
        .await
    }
}

struct SourceMemberSelfBindingEffects<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MemberSelfBindingEffects<'db>
    for SourceMemberSelfBindingEffects<'_, '_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&mut self) -> RunResult<()> {
        self.source.work(1).await
    }

    async fn callable_is_function_like(&mut self, callable: CallableType<'db>) -> RunResult<bool> {
        let kind = self
            .source
            .field(
                callable
                    .field_requests(self.source.access.endpoint().field_request_context())
                    .kind(),
            )
            .await?;
        self.source
            .local(1, 0, || matches!(kind, CallableTypeKind::FunctionLike))
            .await
    }

    async fn nominal_is_definition_generic(
        &mut self,
        instance: NominalInstanceType<'db>,
    ) -> RunResult<bool> {
        nominal_is_definition_generic_with(instance, NominalClassFacts, self.source).await
    }

    async fn search_self(&mut self, ty: Type<'db>) -> RunResult<bool> {
        self.source
            .allocate_future(|| async {
                let mut walk = RuntimeTypeWalk {
                    db: self.source.db(),
                    endpoint: self.source.access.endpoint(),
                    query: MemberSelfPredicate {
                        source: self.source,
                    },
                    unavailable: self.source,
                };
                search_type_with(
                    ty,
                    TypeSearchMode::IncludeAliasArguments,
                    TypeWalkFacts,
                    &mut walk,
                )
                .await
            })
            .await?
            .await
    }

    async fn contains_self(&mut self, ty: Type<'db>) -> RunResult<bool> {
        contains_self_with(ty, self).await
    }
}

struct MemberSelfPredicate<'effects, 'access, 'run, 'db: 'run, A> {
    source: &'effects SourceEffects<'access, 'run, 'db, A>,
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> RuntimeTypeSearchWith<'run, 'db>
    for MemberSelfPredicate<'_, '_, 'run, 'db, A>
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
        self.source
            .local(1, 0, || matches!(kind, TypeVarKind::TypingSelf))
            .await
    }
}
