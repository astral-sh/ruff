//! Scope-member enumeration borrows the caller's retained semantic index.

use salsa::execution_probe::{RunError, RunResult};
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{BindingWithConstraintsIterator, DeclarationsIterator, SemanticIndex};

use super::{SourceAccess, SourceEffects};
use crate::ProgramEnvironment;
use crate::place::{
    PlaceFromDeclarationsResult, PlaceWithDefinition, RequiresExplicitReExport,
    place_from_bindings_with, place_from_declarations_with,
};
use crate::types::Type;
use crate::types::list_members::MemberWithDefinition;
use crate::types::list_members::scope::{
    ScopeMemberCursor, ScopeMemberEffects, ScopeMemberFacts, ScopeMemberSource, make_member,
    scope_member_next_with,
};

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> SourceEffects<'_, 'run, 'db, A> {
    /// Uses an index already retained by the source owner. A scope from another file needs that
    /// file's prepared-source route before its index can be supplied here.
    pub(in crate::types::infer) async fn scope_member_cursor<'index>(
        &self,
        scope: ScopeId<'db>,
        index: &'index SemanticIndex<'db>,
    ) -> RunResult<ScopeMemberCursor<'index, 'db>> {
        let db = self.db();
        let file = self.scope_file(scope).await?;
        let fields = self.access.endpoint().field_request_context();
        let program = self.field(file.read_fields(fields).program()).await?;
        self.check_program(program)?;
        let index_scope = self.local(1, 0, || index.scope_ids().next()).await?;
        let index_file = match index_scope {
            Some(scope) => Some(self.scope_file(scope).await?),
            None => None,
        };
        let file_scope = self.field(scope.read_fields(db).file_scope_id()).await?;
        if index_file != Some(file) {
            return Err(RunError::Contract("retained scope member index is foreign"));
        }
        // The cursor owns two fixed-size environments and borrows both tables. Its creation
        // prepays disposal of that state if a later member reduction suspends or refuses.
        self.local(8, 0, || {
            ScopeMemberCursor::new_with_environment(
                ProgramEnvironment::from_program(program),
                index.use_def_map(file_scope),
                index.place_table(file_scope),
            )
        })
        .await
    }

    pub(in crate::types::infer) async fn next_scope_member(
        &self,
        cursor: &mut ScopeMemberCursor<'_, 'db>,
    ) -> RunResult<Option<MemberWithDefinition<'db>>> {
        scope_member_next_with(cursor, ScopeMemberFacts, self).await
    }
}

impl<'run, 'db: 'run, A: SourceAccess<'run, 'db>> ScopeMemberEffects<'db>
    for SourceEffects<'_, 'run, 'db, A>
{
    type Error = RunError;

    async fn next_source<'index>(
        &self,
        cursor: &mut ScopeMemberCursor<'index, 'db>,
    ) -> RunResult<Option<ScopeMemberSource<'index, 'db>>> {
        // An advance performs at most two bounded symbol lookups, including the transition
        // from declarations to bindings, and retains only borrowed reduction inputs.
        self.local(6, 0, || cursor.next_source()).await
    }

    async fn declaration_place(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'_, 'db>,
    ) -> RunResult<PlaceFromDeclarationsResult<'db>> {
        place_from_declarations_with(env, self, declarations, RequiresExplicitReExport::No, None)
            .await
    }

    async fn binding_place(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'_, 'db>,
    ) -> RunResult<PlaceWithDefinition<'db>> {
        place_from_bindings_with(env, self, bindings, RequiresExplicitReExport::No, None).await
    }

    async fn member(
        &self,
        cursor: &ScopeMemberCursor<'_, 'db>,
        symbol: ScopedSymbolId,
        ty: Type<'db>,
        first_reachable_definition: Definition<'db>,
    ) -> RunResult<MemberWithDefinition<'db>> {
        let name = self.local(1, 0, || cursor.symbol_name(symbol)).await?;
        // Name clones share immutable heap storage with the retained place table. Creation and
        // eventual disposal are fixed-size operations and do not allocate a second name buffer.
        self.local(4, 0, || make_member(name, ty, first_reachable_definition))
            .await
    }
}
