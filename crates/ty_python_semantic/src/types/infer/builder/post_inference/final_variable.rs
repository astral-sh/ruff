use std::convert::Infallible;

use crate::{
    TypeQualifiers,
    place::{place_from_bindings, place_from_declarations},
    types::{context::InferContext, diagnostic::FINAL_WITHOUT_VALUE},
};
use ty_mapping_probe_macros::shared_semantic_family;
use ty_python_core::SemanticIndex;
use ty_python_core::definition::Definition;
use ty_python_core::scope::FileScopeId;
use ty_python_core::symbol::ScopedSymbolId;

/// Check for `Final`-qualified declarations in module/function scopes that are never
/// assigned a value. Class body scopes are handled separately in
/// `check_class_final_without_value`.
pub(crate) fn check_final_without_value<'db>(
    context: &InferContext<'db, '_>,
    index: &SemanticIndex<'db>,
) {
    match check_final_without_value_sync(context, index, FinalFacts, &OrdinaryFinalEffects) {
        Ok(()) => {}
        Err(error) => match error {},
    }
}

struct OrdinaryFinalEffects;
pub(in crate::types::infer::builder) struct FinalFacts;

shared_semantic_family! {
    #[synchronous(SynchronousFinalEffects)]
    pub(in crate::types::infer::builder) trait FinalEffects<'db> {
        type Error;
        #[operation(local)]
        async fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn in_class(&self, context: &InferContext<'db, '_>, index: &SemanticIndex<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_symbol(&self, context: &InferContext<'db, '_>, index: &SemanticIndex<'db>, cursor: &mut usize) -> Result<Option<ScopedSymbolId>, Self::Error>;
        #[operation(source)]
        async fn declaration(&self, context: &InferContext<'db, '_>, index: &SemanticIndex<'db>, symbol: ScopedSymbolId) -> Result<(TypeQualifiers, Option<Definition<'db>>), Self::Error>;
        #[operation(source)]
        async fn check_missing_value(&self, context: &InferContext<'db, '_>, index: &SemanticIndex<'db>, symbol: ScopedSymbolId, first_declaration: Option<Definition<'db>>) -> Result<(), Self::Error>;
    }

    #[finite_capability]
    impl FinalFacts {
        fn is_final(&self, qualifiers: TypeQualifiers) -> bool {
            qualifiers.contains(TypeQualifiers::FINAL)
        }
    }

    #[synchronous(check_final_without_value_sync)]
    #[capabilities(effects = FinalEffects, facts = FinalFacts)]
    #[passive_values()]
    pub(in crate::types::infer::builder) async fn check_final_without_value_with<'db, E: FinalEffects<'db>>(
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        facts: FinalFacts,
        effects: &E,
    ) -> Result<(), E::Error> {
        // Bare declarations are normal in stubs; class declarations have their own check.
        if effects.in_stub(context).await? || effects.in_class(context, index).await? {
            return Ok(());
        }
        let mut cursor = 0;
        #[cursor_loop]
        while let Some(symbol) = effects.next_symbol(context, index, &mut cursor).await? {
            let (qualifiers, first_declaration) = effects.declaration(context, index, symbol).await?;
            if !facts.is_final(qualifiers) {
                continue;
            }
            effects.check_missing_value(context, index, symbol, first_declaration).await?;
        }
        Ok(())
    }
}

impl<'db> SynchronousFinalEffects<'db> for OrdinaryFinalEffects {
    type Error = Infallible;

    fn in_stub(&self, context: &InferContext<'db, '_>) -> Result<bool, Self::Error> {
        Ok(context.in_stub())
    }

    fn in_class(
        &self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(index
            .scope(context.scope().file_scope_id(context.db()))
            .kind()
            .is_class())
    }

    fn next_symbol(
        &self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        cursor: &mut usize,
    ) -> Result<Option<ScopedSymbolId>, Self::Error> {
        Ok(next_symbol(
            context.scope().file_scope_id(context.db()),
            index,
            cursor,
        ))
    }

    fn declaration(
        &self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        symbol: ScopedSymbolId,
    ) -> Result<(TypeQualifiers, Option<Definition<'db>>), Self::Error> {
        let declarations = index
            .use_def_map(context.scope().file_scope_id(context.db()))
            .end_of_scope_symbol_declarations(symbol);
        let result =
            place_from_declarations(context.db(), context.program_environment(), declarations);
        let first_declaration = result.first_declaration;
        let (place_and_quals, _) = result.into_place_and_conflicting_declarations();
        Ok((place_and_quals.qualifiers, first_declaration))
    }

    fn check_missing_value(
        &self,
        context: &InferContext<'db, '_>,
        index: &SemanticIndex<'db>,
        symbol: ScopedSymbolId,
        first_declaration: Option<Definition<'db>>,
    ) -> Result<(), Self::Error> {
        check_missing_value(context, index, symbol, first_declaration);
        Ok(())
    }
}

pub(in crate::types::infer::builder) fn next_symbol<'db>(
    file_scope: FileScopeId,
    index: &SemanticIndex<'db>,
    cursor: &mut usize,
) -> Option<ScopedSymbolId> {
    let result = index
        .use_def_map(file_scope)
        .all_end_of_scope_symbol_declarations()
        .nth(*cursor)
        .map(|(symbol, _)| symbol);
    *cursor += usize::from(result.is_some());
    result
}

fn check_missing_value<'db>(
    context: &InferContext<'db, '_>,
    index: &SemanticIndex<'db>,
    symbol: ScopedSymbolId,
    first_declaration: Option<Definition<'db>>,
) {
    let db = context.db();
    let file_scope_id = context.scope().file_scope_id(db);
    let use_def = index.use_def_map(file_scope_id);
    let place_table = index.place_table(file_scope_id);
    let bindings = use_def.end_of_scope_symbol_bindings(symbol);
    let binding_place = place_from_bindings(db, context.program_environment(), bindings);

    if !binding_place.place.is_undefined() {
        return;
    }

    let place = place_table.place(symbol);
    if let Some(first_decl) = first_declaration
        && let Some(builder) = context.report_lint(
            &FINAL_WITHOUT_VALUE,
            first_decl.full_range(db, context.module()),
        )
    {
        builder.into_diagnostic(format_args!(
            "`Final` symbol `{place}` is not assigned a value"
        ));
    }
}
