//! Class `Final` checks retain declaration qualifiers and their diagnostic provenance.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::DeclarationsIterator;
use ty_python_core::definition::Definition;
use ty_python_core::symbol::ScopedSymbolId;

use super::ClassCheckEffects;
use crate::TypeQualifiers;
use crate::analysis::ClassCheckOperation;
use crate::place::{RequiresExplicitReExport, place_from_declarations_with};
use crate::types::StaticClassLiteral;
use crate::types::class::CodeGeneratorKind;
use crate::types::infer::builder::post_inference::static_class::final_values::ClassFinalValueEffects;
use crate::types::infer::builder::post_inference::static_class::phases::StaticClassDefinitionEffects;
use crate::types::infer::builder::post_inference::static_class::slot_checks::ClassSlotCheckEffects;
use crate::types::infer::builder::source_definition::controlled::{
    SourceAccess, SourceEffects, SourceOperation,
};
use crate::types::list_members::scope::ScopeMemberCursor;

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ClassFinalValueEffects<'db>
    for ClassCheckEffects<'_, '_, 'run, 'db, '_, A>
{
    type Error = RunError;

    async fn in_stub(&self) -> RunResult<bool> {
        ClassSlotCheckEffects::in_stub(self).await
    }

    async fn declaration_cursor<'index>(
        &'index self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<ScopeMemberCursor<'index, 'db>>
    where
        'db: 'index,
    {
        let fields = self.source.access.endpoint().field_request_context();
        let scope = self
            .source
            .field(class.field_requests(fields).body_scope())
            .await?;
        self.source
            .scope_member_cursor(scope, self.builder.index)
            .await
    }

    async fn class_kind(
        &self,
        class: StaticClassLiteral<'db>,
    ) -> RunResult<Option<CodeGeneratorKind<'db>>> {
        StaticClassDefinitionEffects::class_kind(self, class).await
    }

    async fn is_protocol(&self, class: StaticClassLiteral<'db>) -> RunResult<bool> {
        StaticClassDefinitionEffects::is_protocol(self, class).await
    }

    async fn next_declaration<'index>(
        &self,
        cursor: &mut ScopeMemberCursor<'index, 'db>,
    ) -> RunResult<Option<(ScopedSymbolId, DeclarationsIterator<'index, 'db>)>> {
        self.source.local(4, 0, || cursor.next_declaration()).await
    }

    async fn declaration(
        &self,
        declarations: DeclarationsIterator<'_, 'db>,
    ) -> RunResult<(TypeQualifiers, Option<Definition<'db>>)> {
        let retirement = SourceEffects::<A>::checked(declarations.traversal_len().checked_add(2))?;
        let result = place_from_declarations_with(
            self.builder.program_environment(),
            self.source,
            declarations,
            RequiresExplicitReExport::No,
            None,
        )
        .await?;
        self.source
            .local(retirement, 0, || {
                let first_declaration = result.first_declaration;
                let (place_and_quals, _) = result.into_place_and_conflicting_declarations();
                (place_and_quals.qualifiers, first_declaration)
            })
            .await
    }

    async fn check_missing_value(
        &self,
        _class: StaticClassLiteral<'db>,
        _symbol: ScopedSymbolId,
        _first_declaration: Option<Definition<'db>>,
    ) -> RunResult<()> {
        self.source
            .unavailable(SourceOperation::ClassCheck(
                ClassCheckOperation::FinalValues,
            ))
            .await
    }
}
