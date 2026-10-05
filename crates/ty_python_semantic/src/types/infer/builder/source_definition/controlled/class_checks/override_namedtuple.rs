//! Supplies inherited NamedTuple field lookup through the existing `SourceEffects` endpoint,
//! which owns execution as in the sibling `override_remaining` adapter.

use ruff_python_ast::name::Name;
use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;

use super::ClassCheckEffects;
use crate::analysis::ClassCheckOperation;
use crate::types::class::{
    CodeGeneratorKind, DynamicNamedTupleAnchor, DynamicNamedTupleLiteral,
    static_code_generator_with,
};
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::mro::base::class_mro_start_with;
use crate::types::mro::field_reads::MroFieldReads;
use crate::types::mro::iteration::{MroCursor, MroDirection, mro_next_with};
use crate::types::overrides::namedtuple_fields::NamedTupleFieldEffects;
use crate::types::{ClassBase, ClassLiteral, ClassType, Specialization, StaticClassLiteral};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> NamedTupleFieldEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn local<T>(
        &self,
        work: Option<usize>,
        bytes: Option<usize>,
        action: impl FnOnce() -> T,
    ) -> RunResult<T> {
        let quote = SourceEffects::<A>::checked(work)
            .and_then(|work| SourceEffects::<A>::checked(bytes).map(|bytes| (work, bytes)));
        self.source.local_quoted(quote, action).await
    }

    async fn next_mro(&self, cursor: &mut MroCursor<'db>) -> RunResult<Option<ClassBase<'db>>> {
        let next = self
            .source
            .allocate_future(|| {
                mro_next_with(
                    MroFieldReads::new(self.builder.db()),
                    cursor,
                    MroDirection::Forward,
                    self.source,
                )
            })
            .await?
            .await?;
        self.step(|| next).await
    }

    async fn class_identity(
        &self,
        class: ClassType<'db>,
    ) -> RunResult<(ClassLiteral<'db>, Option<Specialization<'db>>)> {
        let start = self
            .source
            .allocate_future(|| {
                class_mro_start_with(
                    MroFieldReads::new(self.builder.db()),
                    class,
                    None,
                    self.source,
                )
            })
            .await?
            .await?;
        self.step(|| (start.class, start.specialization)).await
    }

    async fn is_named_tuple(&self, class: ClassLiteral<'db>) -> RunResult<bool> {
        self.step(|| ()).await?;
        match class {
            ClassLiteral::Static(class) => {
                let generator = self
                    .source
                    .allocate_future(|| static_code_generator_with(class, self.source))
                    .await?
                    .await?;
                self.step(|| matches!(generator, Some(CodeGeneratorKind::NamedTuple)))
                    .await
            }
            ClassLiteral::DynamicNamedTuple(_) => self.step(|| true).await,
            ClassLiteral::DynamicTypedDict(_) | ClassLiteral::DynamicEnum(_) => {
                self.step(|| false).await
            }
            ClassLiteral::Dynamic(_) => {
                self.source
                    .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
                    .await
            }
        }
    }

    async fn static_field(
        &self,
        _class: StaticClassLiteral<'db>,
        _specialization: Option<Specialization<'db>>,
        _field_name: &Name,
    ) -> RunResult<Option<Option<Definition<'db>>>> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn dynamic_has_field(
        &self,
        _class: DynamicNamedTupleLiteral<'db>,
        _field_name: &Name,
    ) -> RunResult<bool> {
        self.source
            .unavailable(SourceOperation::ClassCheck(ClassCheckOperation::Overrides))
            .await
    }

    async fn dynamic_definition(
        &self,
        class: DynamicNamedTupleLiteral<'db>,
    ) -> RunResult<Option<Definition<'db>>> {
        let fields = self.source.access.endpoint().field_request_context();
        let anchor = self
            .source
            .field(class.field_requests(fields).anchor())
            .await?;
        self.step(|| match anchor {
            DynamicNamedTupleAnchor::CollectionsDefinition { definition, .. }
            | DynamicNamedTupleAnchor::TypingDefinition(definition) => Some(*definition),
            DynamicNamedTupleAnchor::ScopeOffset { .. } => None,
        })
        .await
    }
}
