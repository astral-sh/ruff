//! General member lookup uses the source root's canonical query and module dependencies.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::place::Place;
use crate::types::callable::CallableTypeKind;
use crate::types::enums::EnumMetadata;
use crate::types::instance::tuple_spec::TupleSpecEffects;
use crate::types::mapping::specialization_start::SpecializationStartEffects;
use crate::types::member_lookup::general::{
    GeneralMemberBranch, GeneralMemberEffects, GeneralMemberFacts, GeneralMemberName,
    GeneralMemberOperation, GeneralMemberPredicate, member_lookup_entry_with,
};
use crate::types::{
    CallableType, ClassLiteral, KnownClass, KnownInstanceType, LookupFacts, MemberLookupKey,
    MemberLookupPolicy, MemberLookupResult, NominalInstanceType, Type, instance_member_entry_with,
    restricted_member_entry_with,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> GeneralMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        self.work(Self::checked(name.len().checked_add(128))?).await
    }

    async fn key_parts(
        &self,
        key: MemberLookupKey<'db>,
    ) -> RunResult<(Type<'db>, &'db Name, MemberLookupPolicy)> {
        let fields = key.field_requests(self.access.endpoint().field_request_context());
        let program = self.field(fields.program()).await?;
        self.check_program(program)?;
        let ty = self.field(fields.ty()).await?;
        let name = self.field(fields.name()).await?;
        let policy = self.field(fields.policy()).await?;
        Ok((ty, name, policy))
    }

    async fn predicate(
        &self,
        predicate: GeneralMemberPredicate<'db>,
        _name: &str,
    ) -> RunResult<bool> {
        match predicate {
            GeneralMemberPredicate::FunctionLike(Type::FunctionLiteral(function)) => {
                function.callable_type_kind_with(self.db(), self).await?;
                Ok(true)
            }
            GeneralMemberPredicate::FunctionLike(Type::Callable(callable)) => {
                let fields =
                    callable.field_requests(self.access.endpoint().field_request_context());
                let kind = self.field(fields.kind()).await?;
                self.local(1, 0, || {
                    matches!(
                        kind,
                        CallableTypeKind::FunctionLike
                            | CallableTypeKind::StaticMethodLike
                            | CallableTypeKind::ClassMethodLike
                    )
                })
                .await
            }
            GeneralMemberPredicate::FunctionLike(Type::KnownInstance(
                KnownInstanceType::MethodWrapper(wrapper),
            )) => {
                let fields = wrapper.field_requests(self.access.endpoint().field_request_context());
                self.field(fields.kind()).await?;
                Ok(true)
            }
            GeneralMemberPredicate::FunctionLike(_) => self.local(1, 0, || false).await,
            GeneralMemberPredicate::CallableFunctionOrStaticmethod(callable) => {
                let fields =
                    callable.field_requests(self.access.endpoint().field_request_context());
                let kind = self.field(fields.kind()).await?;
                self.local(1, 0, || {
                    matches!(
                        kind,
                        CallableTypeKind::FunctionLike | CallableTypeKind::StaticMethodLike
                    )
                })
                .await
            }
            GeneralMemberPredicate::CallableStaticOrClassmethod(callable) => {
                let fields =
                    callable.field_requests(self.access.endpoint().field_request_context());
                let kind = self.field(fields.kind()).await?;
                self.local(1, 0, || {
                    matches!(
                        kind,
                        CallableTypeKind::StaticMethodLike | CallableTypeKind::ClassMethodLike
                    )
                })
                .await
            }
            GeneralMemberPredicate::ParamSpec(typevar) => {
                SpecializationStartEffects::typevar_is_paramspec(self, self.db(), typevar).await
            }
            _ => {
                self.unavailable(SourceOperation::MemberLookup(predicate.operation()))
                    .await
            }
        }
    }

    async fn wrapper_descriptor(
        &self,
        _ty: Type<'db>,
        _name: &str,
        _policy: MemberLookupPolicy,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::WrapperDescriptor,
        ))
        .await
    }

    async fn callable_runtime_class(
        &self,
        callable: CallableType<'db>,
    ) -> RunResult<Option<KnownClass>> {
        let fields = callable.field_requests(self.access.endpoint().field_request_context());
        let kind = self.field(fields.kind()).await?;
        self.local(1, 0, || match kind {
            CallableTypeKind::FunctionLike => Some(KnownClass::FunctionType),
            CallableTypeKind::StaticMethodLike => Some(KnownClass::Staticmethod),
            CallableTypeKind::ClassMethodLike => Some(KnownClass::Classmethod),
            CallableTypeKind::Regular
            | CallableTypeKind::DunderParamSpec
            | CallableTypeKind::ParamSpecValue => None,
        })
        .await
    }

    async fn nominal_enum_member(
        &self,
        _instance: NominalInstanceType<'db>,
        _name: &str,
    ) -> RunResult<Option<(ClassLiteral<'db>, &'db EnumMetadata<'db>)>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::NominalEnumMember,
        ))
        .await
    }

    async fn execute(
        &self,
        branch: GeneralMemberBranch<'db>,
        key: MemberLookupKey<'db>,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        match branch {
            GeneralMemberBranch::Bound(ty) => self.local(1, 0, || Place::bound(ty).into()).await,
            GeneralMemberBranch::Undefined => self.local(1, 0, || Place::Undefined.into()).await,
            GeneralMemberBranch::BoolReal(value) => {
                self.local(2, 0, || {
                    Place::bound(Type::int_literal(i64::from(value))).into()
                })
                .await
            }
            GeneralMemberBranch::VersionInfo => {
                let env = self
                    .local(1, 0, || ProgramEnvironment::from_program(self.program))
                    .await?;
                let version = TupleSpecEffects::python_version(self, &env).await?;
                let fields = key.field_requests(self.access.endpoint().field_request_context());
                let name = self.field(fields.name()).await?;
                self.local(5, 0, || {
                    let segment = if name == "major" {
                        version.major
                    } else {
                        version.minor
                    };
                    Place::bound(Type::int_literal(segment.into())).into()
                })
                .await
            }
            GeneralMemberBranch::Module(module) => {
                let fields = key.field_requests(self.access.endpoint().field_request_context());
                let name = self.field(fields.name()).await?;
                let env = self
                    .local(1, 0, || ProgramEnvironment::from_program(self.program))
                    .await?;
                module.static_member_with(self.db(), &env, self, name).await
            }
            GeneralMemberBranch::ClassObject => {
                self.type_parameter_future(|| self.class_object_entry(key, receiver, None))
                    .await?
                    .await
            }
            GeneralMemberBranch::Instance | GeneralMemberBranch::Restricted => {

                let fields = key.field_requests(self.access.endpoint().field_request_context());
                let ty = self.field(fields.ty()).await?;
                let receiver = receiver.unwrap_or(ty);
                if matches!(branch, GeneralMemberBranch::Instance) {
                    self.allocate_future(|| {
                        instance_member_entry_with(key, receiver, LookupFacts, self)
                    })
                    .await?
                    .await
                } else {
                    self.allocate_future(|| {
                        restricted_member_entry_with(key, receiver, LookupFacts, self)
                    })
                    .await?
                    .await
                }
            }
            _ => {
                self.unavailable(SourceOperation::MemberLookup(branch.operation()))
                    .await
            }
        }
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        if receiver.is_some() {
            return self
                .unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::ExplicitReceiver,
                ))
                .await;
        }
        match name {
            GeneralMemberName::Shared(name) => self.access.member_lookup(ty, name, policy).await,
            GeneralMemberName::Text(text) => {
                let work =
                    Self::checked(text.len().checked_mul(2).and_then(|len| len.checked_add(1)))?;
                let bytes = Self::checked(text.len().checked_add(3 * size_of::<usize>()))?;
                let name = self.local(work, bytes, || Name::new(text)).await?;
                self.access.member_lookup(ty, &name, policy).await
            }
        }
    }

    async fn fallback(
        &self,
        ty: Type<'db>,
        name: GeneralMemberName<'_>,
        policy: MemberLookupPolicy,
        receiver: Option<Type<'db>>,
    ) -> RunResult<MemberLookupResult<'db>> {
        member_lookup_entry_with(ty, name, policy, receiver, GeneralMemberFacts, self).await
    }

    async fn dunder_class(&self, _ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::DunderClass,
        ))
        .await
    }

    async fn bound(&self, ty: Type<'db>) -> RunResult<MemberLookupResult<'db>> {
        self.local(1, 0, || Place::bound(ty).into()).await
    }
}
