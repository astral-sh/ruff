use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::ScopeId;
use ty_python_core::{DeclarationsIterator, ImportedFinalCandidatesIterator};

use super::{FixedFieldCopy, SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::place::{
    PlaceAndQualifiers, PlaceFromDeclarationsResult, RequiresExplicitReExport,
    place_from_declarations_with,
};
use crate::types::class::implicit_attributes::{
    ImplicitAttribute, ImplicitNameSearchControl, implicit_attribute_bindings_with,
    implicit_name_index_with,
};
use crate::types::class::member_source::{
    ImplicitAttributeEffects, MemberSourceEffects, MemberSourceWork, StaticCodeGeneratorEffects,
    StaticInstanceMemberEffects, instance_field_policy,
};
use crate::types::class::slots::{SlotSelectorEffects, instance_slot_with};
use crate::types::class::{CodeGeneratorKind, MethodDecorator, static_code_generator_with};
use crate::types::instance::{NominalClassFacts, nominal_known_class_with};
use crate::types::member::Member;
use crate::types::member_lookup::general::GeneralMemberOperation;
use crate::types::{DataclassParams, KnownClass, MemberLookupPolicy, StaticClassLiteral, Type};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticInstanceMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    async fn body_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        SlotSelectorEffects::body_scope(self, class).await
    }

    async fn code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        static_code_generator_with(class, self).await
    }

    async fn has_own_named_tuple_field(
        &self,
        _class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> RunResult<bool> {
        self.unavailable(SourceOperation::ClassCheck(ClassCheckOperation::NamedTuple))
            .await
    }

    async fn declaration_place<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'map, 'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        self.environment_program(env).await?;
        self.work(Self::checked(
            declarations
                .traversal_len()
                .checked_mul(4)
                .and_then(|count| count.checked_add(1)),
        )?)
        .await?;
        self.allocate_future(|| {
            place_from_declarations_with(
                env,
                self,
                declarations,
                RequiresExplicitReExport::No,
                None,
            )
        })
        .await?
        .await
    }

    async fn imported_final<'map>(
        &self,
        env: &ProgramEnvironment<'db>,
        result: PlaceFromDeclarationsResult<'db>,
        imported: ImportedFinalCandidatesIterator<'map, 'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        self.environment_program(env).await?;
        self.work(Self::checked(
            imported
                .traversal_len()
                .checked_mul(4)
                .and_then(|count| count.checked_add(1)),
        )?)
        .await?;
        self.allocate_future(|| {
            result.with_imported_final_with(
                env,
                self,
                imported,
                RequiresExplicitReExport::No,
                None,
                false,
            )
        })
        .await?
        .await
    }

    async fn implicit_member(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> RunResult<Member<'db>> {
        let attribute = self
            .allocate_future(|| {
                implicit_attribute_bindings_with(class, name, MethodDecorator::None, self)
            })
            .await?
            .await?;
        self.local(1, 0, || attribute.member()).await
    }

    async fn is_kw_only(&self, ty: Type<'db>) -> RunResult<bool> {
        let Some(instance) = self.local(1, 0, || ty.as_nominal_instance()).await? else {
            return Ok(false);
        };
        let known = nominal_known_class_with(instance, NominalClassFacts, self).await?;
        self.local(1, 0, || known == Some(KnownClass::KwOnly)).await
    }

    async fn is_stub(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        SlotSelectorEffects::is_stub(self, class).await
    }

    async fn has_instance_slot(
        &self,
        class: StaticClassLiteral<'db>,
        name: &str,
    ) -> RunResult<bool> {
        self.allocate_future(|| instance_slot_with(class, name, self))
            .await?
            .await
    }

    async fn is_own_dataclass_instance_field(
        &self,
        class: StaticClassLiteral<'db>,
        _name: &str,
    ) -> RunResult<bool> {
        let generator = StaticInstanceMemberEffects::code_generator(self, class).await?;
        let Some(_field_policy) = self
            .local(
                size_of::<Option<CodeGeneratorKind<'db>>>() * 2 + 1,
                0,
                || instance_field_policy(generator),
            )
            .await?
        else {
            return Ok(false);
        };
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::DataclassFields,
        ))
        .await
    }

    async fn getter_member(
        &self,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
    ) -> RunResult<PlaceAndQualifiers<'db>> {
        self.environment_program(env).await?;
        let name = self.local(1, 0, || Name::new_static("__get__")).await?;
        self.access
            .class_member_lookup(ty, &name, MemberLookupPolicy::default())
            .await
    }

    async fn union_two(
        &self,
        env: &ProgramEnvironment<'db>,
        first: Type<'db>,
        second: Type<'db>,
    ) -> RunResult<Type<'db>> {
        self.environment_program(env).await?;
        self.access.union_from_two_elements(first, second).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImplicitAttributeEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn body_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        SlotSelectorEffects::body_scope(self, class).await
    }

    async fn checkpoint(&self, work: MemberSourceWork) -> RunResult<()> {
        MemberSourceEffects::checkpoint(self, work).await
    }

    async fn names(&self, scope: ScopeId<'db>) -> RunResult<&'db [Name]> {
        self.check_file_program(self.scope_file(scope).await?)
            .await?;
        self.access.implicit_attribute_names(scope).await
    }

    async fn find_name(&self, names: &'db [Name], name: &str) -> RunResult<Option<usize>> {
        let max_comparisons = usize::BITS as usize + 1;
        self.local(max_comparisons * 4 + 8, 0, || {
            implicit_name_index_with(names, name, self)
        })
        .await?
    }

    async fn infer_named_attribute(
        &self,
        _scope: ScopeId<'db>,
        _name: &'db Name,
        _target: MethodDecorator,
    ) -> RunResult<ImplicitAttribute<'db>> {
        self.unavailable(SourceOperation::MemberLookup(
            GeneralMemberOperation::ImplicitAttributeInference,
        ))
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ImplicitNameSearchControl
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    fn comparison(&self, candidate_bytes: usize, requested_bytes: usize) -> RunResult<()> {
        let work = Self::checked(
            candidate_bytes
                .checked_add(requested_bytes)
                .and_then(|bytes| bytes.checked_add(1)),
        )?;
        let endpoint = self.access.endpoint();
        endpoint.admit_work(work)?;
        endpoint.check_completion()
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> StaticCodeGeneratorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<DataclassParams<'db>>> {
        SlotSelectorEffects::dataclass_params(self, class).await
    }

    async fn known(&self, class: StaticClassLiteral<'db>) -> RunResult<Option<KnownClass>> {
        SlotSelectorEffects::known(self, class).await
    }

    async fn has_explicit_bases(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        SlotSelectorEffects::has_explicit_bases(self, class).await
    }

    async fn has_explicit_metaclass(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        self.field_with_profile(
            class
                .field_requests(self.access.endpoint().field_request_context())
                .has_explicit_metaclass(),
            &FixedFieldCopy,
        )
        .await
    }

    async fn checkpoint(&self, work: MemberSourceWork) -> RunResult<()> {
        MemberSourceEffects::checkpoint(self, work).await
    }

    async fn code_generator_query(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        self.check_file_program(self.static_class_file(class).await?)
            .await?;
        self.access.code_generator(class).await
    }
}
