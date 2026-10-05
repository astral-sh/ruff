use salsa::execution_probe::{RunError, RunResult};

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::place::PlaceAndQualifiers;
use crate::types::class::KnownClassInstanceEffects;
use crate::types::class::instance_storage::static_is_typed_dict_with;
use crate::types::generics::binding::TypeVarBindingEffects;
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::member_lookup::mro_dispatch::{
    MroLookupEffects, MroLookupFacts, SubclassMroEffects, find_name_in_mro_with,
    subclass_find_name_in_mro_with,
};
use crate::types::set_theoretic::pair_union::PairUnionEffects;
use crate::types::typevar::TypeVarConstraints;
use crate::types::typevar::bounds::typevar_bounds_with;
use crate::types::{
    BoundTypeVarInstance, ClassLiteral, ClassType, IntersectionType, KnownClass,
    MemberLookupPolicy, NominalInstanceType, ProtocolInstanceType, RecursiveType, SubclassOfInner,
    SubclassOfType, Type, TypeAliasType, TypeVarBoundOrConstraints, UnionType,
    native_class_mro_attribute, property_wrapper_kind,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(super) async fn source_find_name_in_mro(
        &self,
        ty: Type<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        let env = PairUnionEffects::environment(self, self.program).await?;
        self.allocate_future(|| find_name_in_mro_with(ty, &env, name, policy, MroLookupFacts, self))
            .await?
            .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> MroLookupEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        let work = Self::checked(
            size_of::<Type<'db>>()
                .checked_mul(2)
                .and_then(|work| work.checked_add(name.len())),
        )?;
        self.work(work).await
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        _env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.source_find_name_in_mro(ty, name, policy).await
    }

    async fn recursive_var(&self) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::RecursiveVar,
        ))
        .await
    }

    async fn union(
        &self,
        _union: UnionType<'db>,
        _env: &ProgramEnvironment<'db>,
        _name: &str,
        _policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(GeneralMemberOperation::Union))
            .await
    }

    async fn intersection(
        &self,
        _intersection: IntersectionType<'db>,
        _env: &ProgramEnvironment<'db>,
        _name: &str,
        _policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::Intersection,
        ))
        .await
    }

    async fn unfold(
        &self,
        _recursive: RecursiveType<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<Type<'db>>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::Recursive,
        ))
        .await
    }

    async fn is_typed_dict(&self, class: ClassType<'db>) -> RunResult<bool> {
        let literal = match class {
            ClassType::NonGeneric(class) => self.local(1, 0, || class).await?,
            ClassType::Generic(alias) => {
                let origin = self.field(alias.field_requests(self.db()).origin()).await?;
                ClassLiteral::Static(origin)
            }
        };
        match literal {
            ClassLiteral::Static(class) => {
                let file = self.static_class_file(class).await?;
                self.check_file_program(file).await?;
                static_is_typed_dict_with(class, self).await
            }
            ClassLiteral::DynamicTypedDict(_) => self.local(1, 0, || true).await,
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicEnum(_) => self.local(1, 0, || false).await,
        }
    }

    async fn typed_dict_member(
        &self,
        _class: ClassType<'db>,
        _env: &ProgramEnvironment<'db>,
        _name: &str,
        _policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::TypedDict))
            .await
    }

    async fn native_class_attribute(
        &self,
        class: ClassLiteral<'db>,
        name: &str,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        let known = match class {
            ClassLiteral::Static(class) => {
                self.field(class.field_requests(self.db()).known()).await?
            }
            ClassLiteral::Dynamic(_)
            | ClassLiteral::DynamicNamedTuple(_)
            | ClassLiteral::DynamicTypedDict(_)
            | ClassLiteral::DynamicEnum(_) => None,
        };
        self.local(Self::checked(name.len().checked_add(2))?, 0, || {
            native_class_mro_attribute(known, name)
        })
        .await
    }

    async fn class_member(
        &self,
        class: ClassType<'db>,
        _env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.source_class_mro(class, name, policy).await
    }

    async fn property_wrapper(
        &self,
        member: PlaceAndQualifiers<'db>,
        _env: &ProgramEnvironment<'db>,
        name: &str,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        let needs_lookup = self
            .local(Self::checked(name.len().checked_add(2))?, 0, || {
                property_wrapper_kind(name).is_some()
                    && matches!(member.place.raw_type(), Some(Type::FunctionLiteral(_)))
            })
            .await?;
        if needs_lookup {
            return self
                .unavailable(SourceOperation::MemberLookup(
                    GeneralMemberOperation::PropertyWrapper,
                ))
                .await;
        }
        self.local(1, 0, || member).await
    }

    async fn subclass_member(
        &self,
        subclass: SubclassOfType<'db>,
        env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.allocate_future(|| {
            subclass_find_name_in_mro_with(subclass, env, name, policy, MroLookupFacts, self)
        })
        .await?
        .await
    }

    async fn known_class(
        &self,
        class: KnownClass,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Type<'db>> {
        KnownClassInstanceEffects::class_literal(self, class).await
    }

    async fn nominal_is_type(&self, instance: NominalInstanceType<'db>) -> RunResult<bool> {
        let known = nominal_known_class_with(instance, NominalClassFacts, self).await?;
        self.local(1, 0, || known == Some(KnownClass::Type)).await
    }

    async fn alias_value(&self, _alias: TypeAliasType<'db>) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::TypeAlias,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SubclassMroEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn checkpoint(&self, name: &str) -> RunResult<()> {
        MroLookupEffects::checkpoint(self, name).await
    }

    async fn protocol_meta_member(
        &self,
        _protocol: ProtocolInstanceType<'db>,
        _env: &ProgramEnvironment<'db>,
        _name: &str,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::ProtocolMember,
        ))
        .await
    }

    async fn transpose_typevar(
        &self,
        _typevar: BoundTypeVarInstance<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<SubclassOfInner<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::SubclassMro,
        ))
        .await
    }

    async fn protocol_origin(
        &self,
        _protocol: ProtocolInstanceType<'db>,
    ) -> RunResult<Option<ClassType<'db>>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::ProtocolOrigin,
        ))
        .await
    }

    async fn require_bound_or_constraints(
        &self,
        typevar: BoundTypeVarInstance<'db>,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<TypeVarBoundOrConstraints<'db>> {
        let typevar = TypeVarBindingEffects::bound_typevar(self, typevar).await?;
        typevar_bounds_with(typevar, env, self)
            .await?
            .ok_or(RunError::Contract(
                "transposed subclass TypeVar has no bound or constraints",
            ))
    }

    async fn constraint_types(
        &self,
        _constraints: TypeVarConstraints<'db>,
        _env: &ProgramEnvironment<'db>,
    ) -> RunResult<Type<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::SubclassMro,
        ))
        .await
    }

    async fn lookup(
        &self,
        ty: Type<'db>,
        _env: &ProgramEnvironment<'db>,
        name: &str,
        policy: MemberLookupPolicy,
    ) -> RunResult<Option<PlaceAndQualifiers<'db>>> {
        self.source_find_name_in_mro(ty, name, policy).await
    }
}
