use crate::{
    TypeQualifiers,
    place::{place_from_bindings, place_from_declarations},
    semantic_index::SemanticIndex,
    types::{context::InferContext, diagnostic::FINAL_WITHOUT_VALUE},
};

/// Check for `Final`-qualified declarations in module/function scopes that are never
/// assigned a value. Class body scopes are handled separately in
/// `check_class_final_without_value`.
pub(crate) fn check_final_without_value<'db>(
    context: &InferContext<'db, '_>,
    index: &SemanticIndex<'db>,
) {
    // In stub files, bare declarations without values are normal.
    if context.in_stub() {
        return;
    }

    // Class body scopes are handled separately in check_class_final_without_value,
    // which has access to the class literal to handle special cases (e.g. dataclasses).
    let db = context.db();
    let file_scope_id = context.scope().file_scope_id(db);
    if index.scope(file_scope_id).kind().is_class() {
        return;
    }

    let use_def = index.use_def_map(file_scope_id);
    let place_table = index.place_table(file_scope_id);

    for (symbol_id, declarations) in use_def.all_end_of_scope_symbol_declarations() {
        let result = place_from_declarations(db, declarations);
        let first_declaration = result.first_declaration;
        let (place_and_quals, _) = result.into_place_and_conflicting_declarations();

        if !place_and_quals.qualifiers.contains(TypeQualifiers::FINAL) {
            continue;
        }

        // Imports inherit the `Final` qualifier from the source module, but the
        // import itself provides the value. Don't flag imported `Final` symbols,
        // even if a later `del` removes the binding at end-of-scope.
        if first_declaration.is_some_and(|decl| decl.kind(db).is_import()) {
            continue;
        }

        // Check if the symbol has any bindings in the current scope.
        let bindings = use_def.end_of_scope_symbol_bindings(symbol_id);
        let binding_place = place_from_bindings(db, bindings);

        if !binding_place.place.is_undefined() {
            continue;
        }

        let place = place_table.place(symbol_id);
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
}
