//! Lazy traversal of a scope's declarations followed by its bindings.

use std::convert::Infallible;

use ruff_python_ast::name::Name;
use ty_python_core::definition::Definition;
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;
use ty_python_core::{
    BindingWithConstraintsIterator, DeclarationsIterator, PlaceTable, UseDefMap, place_table,
    use_def_map,
};

use super::{Member, MemberWithDefinition};
use crate::place::{
    PlaceFromDeclarationsResult, PlaceWithDefinition, place_from_bindings, place_from_declarations,
};
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

enum ScopeMemberPass {
    Declarations,
    Bindings,
    Finished,
}

/// Retains only borrowed source tables and fixed-size progress through their two passes.
pub(in crate::types) struct ScopeMemberCursor<'index, 'db> {
    use_def: &'index UseDefMap<'db>,
    table: &'index PlaceTable,
    declarations_env: ProgramEnvironment<'db>,
    bindings_env: ProgramEnvironment<'db>,
    pass: ScopeMemberPass,
    symbol_index: usize,
}

impl<'index, 'db> ScopeMemberCursor<'index, 'db> {
    pub(in crate::types) fn new(
        scope: ScopeId<'db>,
        use_def: &'index UseDefMap<'db>,
        table: &'index PlaceTable,
    ) -> Self {
        Self::new_with_environment(ProgramEnvironment::from_scope(scope), use_def, table)
    }

    pub(in crate::types) fn new_with_environment(
        declarations_env: ProgramEnvironment<'db>,
        use_def: &'index UseDefMap<'db>,
        table: &'index PlaceTable,
    ) -> Self {
        Self {
            use_def,
            table,
            bindings_env: declarations_env.clone(),
            declarations_env,
            pass: ScopeMemberPass::Declarations,
            symbol_index: 0,
        }
    }

    pub(in crate::types) fn next_declaration(
        &mut self,
    ) -> Option<(ScopedSymbolId, DeclarationsIterator<'index, 'db>)> {
        if !matches!(self.pass, ScopeMemberPass::Declarations) {
            return None;
        }
        if let Some(symbol) = self.use_def.end_of_scope_symbol_at(self.symbol_index) {
            self.symbol_index += 1;
            return Some((
                symbol,
                self.use_def.end_of_scope_symbol_declarations(symbol),
            ));
        }
        self.pass = ScopeMemberPass::Bindings;
        self.symbol_index = 0;
        None
    }

    pub(in crate::types) fn next_source(&mut self) -> Option<ScopeMemberSource<'index, 'db>> {
        if let Some((symbol, declarations)) = self.next_declaration() {
            return Some(ScopeMemberSource::Declarations {
                symbol,
                declarations,
            });
        }

        if matches!(self.pass, ScopeMemberPass::Bindings) {
            if let Some(symbol) = self.use_def.end_of_scope_symbol_at(self.symbol_index) {
                self.symbol_index += 1;
                return Some(ScopeMemberSource::Bindings {
                    symbol,
                    bindings: self.use_def.end_of_scope_symbol_bindings(symbol),
                });
            }
            self.pass = ScopeMemberPass::Finished;
        }
        None
    }

    pub(in crate::types) fn symbol_name(&self, symbol: ScopedSymbolId) -> &Name {
        self.table.symbol(symbol).name()
    }
}

pub(in crate::types) enum ScopeMemberSource<'index, 'db> {
    Declarations {
        symbol: ScopedSymbolId,
        declarations: DeclarationsIterator<'index, 'db>,
    },
    Bindings {
        symbol: ScopedSymbolId,
        bindings: BindingWithConstraintsIterator<'index, 'db>,
    },
}

pub(in crate::types) struct ScopeMemberFacts;

ty_mapping_probe_macros::shared_semantic_family! {
    #[synchronous(SynchronousScopeMemberEffects)]
    pub(in crate::types) trait ScopeMemberEffects<'db> {
        type Error;

        #[operation(local)]
        #[progress]
        async fn next_source<'index>(
            &self,
            cursor: &mut ScopeMemberCursor<'index, 'db>,
        ) -> Result<Option<ScopeMemberSource<'index, 'db>>, Self::Error>;
        #[operation(child)]
        async fn declaration_place(
            &self,
            env: &ProgramEnvironment<'db>,
            declarations: DeclarationsIterator<'_, 'db>,
        ) -> Result<PlaceFromDeclarationsResult<'db>, Self::Error>;
        #[operation(child)]
        async fn binding_place(
            &self,
            env: &ProgramEnvironment<'db>,
            bindings: BindingWithConstraintsIterator<'_, 'db>,
        ) -> Result<PlaceWithDefinition<'db>, Self::Error>;
        #[operation(local)]
        async fn member(
            &self,
            cursor: &ScopeMemberCursor<'_, 'db>,
            symbol: ScopedSymbolId,
            ty: Type<'db>,
            first_reachable_definition: Definition<'db>,
        ) -> Result<MemberWithDefinition<'db>, Self::Error>;
    }

    #[finite_capability]
    impl ScopeMemberFacts {
        fn declarations_environment<'cursor, 'db>(
            &self,
            cursor: &'cursor ScopeMemberCursor<'_, 'db>,
        ) -> &'cursor ProgramEnvironment<'db> {
            &cursor.declarations_env
        }

        fn bindings_environment<'cursor, 'db>(
            &self,
            cursor: &'cursor ScopeMemberCursor<'_, 'db>,
        ) -> &'cursor ProgramEnvironment<'db> {
            &cursor.bindings_env
        }

        fn declared_member<'db>(
            &self,
            place: PlaceFromDeclarationsResult<'db>,
        ) -> Option<(Type<'db>, Definition<'db>)> {
            let definition = place.first_declaration?;
            let ty = place
                .ignore_conflicting_declarations()
                .place
                .ignore_possibly_undefined()?;
            Some((ty, definition))
        }

        fn bound_member<'db>(
            &self,
            place: PlaceWithDefinition<'db>,
        ) -> Option<(Type<'db>, Definition<'db>)> {
            let definition = place.first_definition?;
            let ty = place.place.ignore_possibly_undefined()?;
            Some((ty, definition))
        }
    }

    #[synchronous(scope_member_next_sync)]
    #[capabilities(effects = ScopeMemberEffects, facts = ScopeMemberFacts)]
    #[passive_values(ScopeMemberSource::Declarations, ScopeMemberSource::Bindings)]
    pub(in crate::types) async fn scope_member_next_with<'db, E: ScopeMemberEffects<'db>>(
        cursor: &mut ScopeMemberCursor<'_, 'db>,
        facts: ScopeMemberFacts,
        effects: &E,
    ) -> Result<Option<MemberWithDefinition<'db>>, E::Error> {
        #[cursor_loop]
        while let Some(source) = effects.next_source(cursor).await? {
            let (symbol, member) = match source {
                ScopeMemberSource::Declarations { symbol, declarations } => {
                    let place = effects.declaration_place(
                        facts.declarations_environment(cursor),
                        declarations,
                    ).await?;
                    (symbol, facts.declared_member(place))
                }
                ScopeMemberSource::Bindings { symbol, bindings } => {
                    let place = effects.binding_place(
                        facts.bindings_environment(cursor),
                        bindings,
                    ).await?;
                    (symbol, facts.bound_member(place))
                }
            };
            if let Some((ty, definition)) = member {
                let member = effects.member(cursor, symbol, ty, definition).await?;
                return Ok(Some(member));
            }
        }
        Ok(None)
    }
}

struct OrdinaryScopeMemberEffects<'db> {
    db: &'db dyn Db,
}

impl<'db> SynchronousScopeMemberEffects<'db> for OrdinaryScopeMemberEffects<'db> {
    type Error = Infallible;

    fn next_source<'index>(
        &self,
        cursor: &mut ScopeMemberCursor<'index, 'db>,
    ) -> Result<Option<ScopeMemberSource<'index, 'db>>, Infallible> {
        Ok(cursor.next_source())
    }

    fn declaration_place(
        &self,
        env: &ProgramEnvironment<'db>,
        declarations: DeclarationsIterator<'_, 'db>,
    ) -> Result<PlaceFromDeclarationsResult<'db>, Infallible> {
        Ok(place_from_declarations(self.db, env, declarations))
    }

    fn binding_place(
        &self,
        env: &ProgramEnvironment<'db>,
        bindings: BindingWithConstraintsIterator<'_, 'db>,
    ) -> Result<PlaceWithDefinition<'db>, Infallible> {
        Ok(place_from_bindings(self.db, env, bindings))
    }

    fn member(
        &self,
        cursor: &ScopeMemberCursor<'_, 'db>,
        symbol: ScopedSymbolId,
        ty: Type<'db>,
        first_reachable_definition: Definition<'db>,
    ) -> Result<MemberWithDefinition<'db>, Infallible> {
        Ok(make_member(
            cursor.symbol_name(symbol),
            ty,
            first_reachable_definition,
        ))
    }
}

pub(in crate::types) fn make_member<'db>(
    name: &Name,
    ty: Type<'db>,
    first_reachable_definition: Definition<'db>,
) -> MemberWithDefinition<'db> {
    MemberWithDefinition {
        member: Member {
            name: name.clone(),
            ty,
            is_type_check_only: false,
        },
        first_reachable_definition,
    }
}

pub(super) fn all_end_of_scope_members<'db>(
    db: &'db dyn Db,
    scope: ScopeId<'db>,
) -> impl Iterator<Item = MemberWithDefinition<'db>> + 'db {
    let mut cursor = ScopeMemberCursor::new(scope, use_def_map(db, scope), place_table(db, scope));
    let effects = OrdinaryScopeMemberEffects { db };
    std::iter::from_fn(move || {
        match scope_member_next_sync(&mut cursor, ScopeMemberFacts, &effects) {
            Ok(member) => member,
            Err(never) => match never {},
        }
    })
}
