use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::scope::ScopeId;

use super::{SourceAccess, SourceEffects, SourceOperation};
use crate::ProgramEnvironment;
use crate::analysis::ClassCheckOperation;
use crate::types::class::code_generator::{
    CodeGeneratorEffects, CodeGeneratorFacts, code_generator_of_static_class_with,
};
use crate::types::class::context::explicit_class_bases_with;
use crate::types::class::instance_storage::static_is_typed_dict_with;
use crate::types::class::metaclass_selection::{
    MetaclassSelectionResult, static_try_metaclass_with,
};
use crate::types::class::{CodeGeneratorKind, interpret_class_literal_lookup};
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::{
    ClassBase, ClassType, DataclassParams, DataclassTransformerParams, KnownClass, Specialization,
    StaticClassLiteral, Type,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    pub(in crate::types::infer) async fn infer_code_generator(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        let file = self.static_class_file(class).await?;
        self.check_file_program(file).await?;
        self.allocate_future(|| {
            code_generator_of_static_class_with(class, CodeGeneratorFacts, self)
        })
        .await?
        .await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> CodeGeneratorEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;
    type MroCursor = MroCursor<'db>;
    type ExplicitBasesCursor = std::iter::Copied<std::slice::Iter<'db, Type<'db>>>;

    async fn checkpoint(&self) -> RunResult<()> {
        self.work(8).await
    }

    async fn body_scope(&self, class: StaticClassLiteral<'db>) -> RunResult<ScopeId<'db>> {
        self.field(class.field_requests(self.db()).body_scope())
            .await
    }

    async fn dataclass_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<DataclassParams<'db>>> {
        self.field(class.field_requests(self.db()).dataclass_params())
            .await
    }

    async fn try_metaclass(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<MetaclassSelectionResult<'db>> {
        static_try_metaclass_with(class, self).await
    }

    async fn known_type_class(
        &self,
        env: &ProgramEnvironment<'db>,
    ) -> RunResult<Option<StaticClassLiteral<'db>>> {
        let program = self.environment_program(env).await?;
        let lookup = self
            .access
            .known_class_lookup(program, KnownClass::Type)
            .await?;
        self.local(2, 0, || interpret_class_literal_lookup(lookup))
            .await
    }

    async fn start_mro(&self, class: StaticClassLiteral<'db>) -> RunResult<Self::MroCursor> {
        let bytes = Self::checked(size_of::<MroCursor<'db>>().checked_mul(2))?;
        self.local(1, bytes, || MroCursor::new(class.into(), None))
            .await
    }

    async fn next_mro_base(
        &self,
        cursor: &mut Self::MroCursor,
    ) -> RunResult<Option<ClassBase<'db>>> {
        mro_next_with(
            MroFieldReads::new(self.db()),
            cursor,
            MroDirection::Forward,
            self,
        )
        .await
    }

    async fn static_class_literal(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<Option<(StaticClassLiteral<'db>, Option<Specialization<'db>>)>> {
        self.work(1).await?;
        self.static_class_identity(class).await
    }

    async fn dataclass_transformer_params(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<DataclassTransformerParams<'db>>> {
        self.field(
            class
                .field_requests(self.db())
                .dataclass_transformer_params(),
        )
        .await
    }

    async fn dataclass_transformer_kind(
        &self,
        _class: StaticClassLiteral<'db>,
        _params: DataclassTransformerParams<'db>,
    ) -> RunResult<CodeGeneratorKind<'db>> {
        self.unavailable(SourceOperation::ClassCheck(
            ClassCheckOperation::CodeGenerator,
        ))
        .await
    }

    async fn explicit_bases(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Self::ExplicitBasesCursor> {
        let bases = explicit_class_bases_with(class, self).await?;
        let bytes = Self::checked(size_of::<Self::ExplicitBasesCursor>().checked_mul(2))?;
        self.local(1, bytes, || bases.iter().copied()).await
    }

    async fn next_explicit_base(
        &self,
        cursor: &mut Self::ExplicitBasesCursor,
    ) -> RunResult<Option<Type<'db>>> {
        self.local(2, 0, || cursor.next()).await
    }

    async fn is_typed_dict(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        static_is_typed_dict_with(class, self).await
    }
}
